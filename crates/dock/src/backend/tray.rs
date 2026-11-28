//! The system tray service: watches for StatusNotifierItem (SNI) instances
//! and their DBusMenus over D-Bus using `system-tray`, and holds the X11
//! tray (`xembed`) for Wine and other X11 programs.
//! Converts events into dock updates and dispatches user actions back to items.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream, StreamExt};
use system_tray::client::{ActivateRequest, Client, Event, UpdateEvent};
use system_tray::data::BaseMap;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, info, warn};

use super::{Backend, Request, Update, xembed};
use crate::model::tray::{TrayItem, find_in_theme_path, x11_window};

pub fn subscription() -> Subscription<Update> {
    Subscription::run(service)
}

fn collect_tray_items(map: &Arc<Mutex<BaseMap>>) -> Vec<TrayItem> {
    let Ok(items) = map.lock() else {
        return Vec::new();
    };

    let mut result: Vec<TrayItem> = items
        .iter()
        .map(|(address, (sni, menu))| TrayItem::from_sni(address.clone(), sni, menu.clone()))
        .collect();

    // Sort items deterministically by ID then address so they stay in place.
    result.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.address.cmp(&b.address)));
    result
}

fn service() -> impl FutureStream<Item = Update> {
    iced::stream::channel(32, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output.send(Update::Link(Backend::Tray, requests)).await;

        // The X11 tray (Wine, Proton) works even without D-Bus.
        let (x11_icons, mut x11_changed) = unbounded_channel();
        let x11 = xembed::start(x11_icons);
        let mut x11_items: Vec<TrayItem> = Vec::new();

        let client = match Client::new().await {
            Ok(c) => {
                info!("system tray client started successfully");
                Some(c)
            }
            Err(e) => {
                warn!(error = %e, "failed to initialize system tray client");
                None
            }
        };
        let bus = match &client {
            Some(_) => zbus::Connection::session().await.ok(),
            None => None,
        };
        let mut theme_signals = match &bus {
            Some(bus) => subscribe_theme_paths(bus).await,
            None => None,
        };
        let mut theme_paths = ThemePaths::new();
        let mut events = client.as_ref().map(Client::subscribe);
        let items_map = client.as_ref().map(Client::items);
        let all = |x11_items: &[TrayItem]| {
            let mut items = items_map
                .as_ref()
                .map(collect_tray_items)
                .unwrap_or_default();
            items.extend_from_slice(x11_items);
            items
        };

        // Every list of items goes out with the icons of their folders.
        macro_rules! send_items {
            ($refresh:expr) => {{
                let mut items = all(&x11_items);
                find_icon_files(&mut items, &mut theme_paths, bus.as_ref(), $refresh).await;
                items
            }};
        }

        // Initial items if any already exist.
        let initial_items = send_items!(Refresh::All);
        if !initial_items.is_empty() {
            let _ = output.send(Update::Tray(initial_items)).await;
        }
        if client.is_none() && x11.is_none() {
            return;
        }

        loop {
            tokio::select! {
                event = async { events.as_mut()?.recv().await.into() }, if events.is_some() => {
                    match event {
                        Some(Ok(ev)) => {
                            debug!("tray event received");
                            let refresh = match &ev {
                                Event::Remove(address) => {
                                    theme_paths.remove(address);
                                    Refresh::None
                                }
                                Event::Add(address, _)
                                | Event::Update(address, UpdateEvent::Icon { .. }) => {
                                    Refresh::One(address.as_str())
                                }
                                _ => Refresh::None,
                            };
                            let _ = output.send(Update::Tray(send_items!(refresh))).await;
                        }
                        Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                            debug!("tray event stream lagged by {n}");
                            let mut items = all(&x11_items);
                            theme_paths.retain(|address, _| {
                                items.iter().any(|item| &item.address == address)
                            });
                            find_icon_files(&mut items, &mut theme_paths, bus.as_ref(), Refresh::All).await;
                            let _ = output.send(Update::Tray(items)).await;
                        }
                        Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) | None => {
                            info!("tray event channel closed");
                            events = None;
                        }
                    }
                }
                signal = async { theme_signals.as_mut()?.next().await }, if theme_signals.is_some() => {
                    match signal {
                        Some(Ok(msg)) => {
                            if let Some(sender) = msg.header().sender().map(ToString::to_string) {
                                let path = match msg.body().deserialize::<String>() {
                                    Ok(path) => Some(path),
                                    Err(_) => match &bus {
                                        Some(bus) => {
                                            let header = msg.header();
                                            let object_path = header.path().map(|p| p.as_str());
                                            icon_theme_path_at(bus, &sender, object_path).await.ok()
                                        }
                                        None => None,
                                    },
                                };
                                if let Some(path) = path {
                                    debug!(address = %sender, path, "tray item updated icon folder");
                                    theme_paths.insert(sender.clone(), path);
                                    let mut items = all(&x11_items);
                                    if items.iter().any(|item| item.address == sender) {
                                        find_icon_files(
                                            &mut items,
                                            &mut theme_paths,
                                            bus.as_ref(),
                                            Refresh::None,
                                        )
                                        .await;
                                        let _ = output.send(Update::Tray(items)).await;
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => debug!(error = %e, "tray icon theme path signal error"),
                        None => theme_signals = None,
                    }
                }
                Some(icons) = x11_changed.recv() => {
                    x11_items = icons.into_iter().map(TrayItem::from_x11).collect();
                    let _ = output.send(Update::Tray(send_items!(Refresh::None))).await;
                }
                Some(req) = asked.recv() => {
                    // An X11 icon gets the click itself, where it happened.
                    let x11_click = match &req {
                        Request::TrayActivate { address, x, y } => Some((address, 1, *x, *y)),
                        Request::TraySecondary { address, x, y } => Some((address, 2, *x, *y)),
                        Request::TrayContextMenu { address, x, y } => Some((address, 3, *x, *y)),
                        _ => None,
                    };
                    if let Some((address, button, x, y)) = x11_click
                        && let Some(window) = x11_window(address)
                    {
                        if let Some(x11) = &x11 {
                            x11.click(window, button, (x, y));
                        }
                        continue;
                    }
                    let Some(client) = &client else {
                        continue;
                    };
                    match req {
                        Request::TrayActivate { address, x, y } => {
                            let activate_req = ActivateRequest::Default { address, x, y };
                            if let Err(e) = client.activate(activate_req).await {
                                error!(error = %e, "failed to activate tray item");
                            }
                        }
                        Request::TraySecondary { address, x, y } => {
                            let activate_req = ActivateRequest::Secondary { address, x, y };
                            if let Err(e) = client.activate(activate_req).await {
                                error!(error = %e, "failed to secondary activate tray item");
                            }
                        }
                        Request::TrayMenuItem { address, menu_path, id } => {
                            let activate_req = ActivateRequest::MenuItem {
                                address,
                                menu_path,
                                submenu_id: id,
                            };
                            if let Err(e) = client.activate(activate_req).await {
                                error!(error = %e, "failed to activate tray menu item");
                            }
                        }
                        Request::TrayContextMenu { address, x, y } => {
                            if let Err(e) = context_menu(&address, x, y).await {
                                error!(error = %e, "failed to open the menu of tray item");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    })
}

/// The icon folder of each item, as it last said (`IconThemePath`).
/// Electron writes each new icon to a new folder and deletes the old one, and
/// may register its item with an empty `IconThemePath` before setting it;
/// `system-tray` drops the `NewIconThemePath` signal and keeps the initial value.
type ThemePaths = HashMap<String, String>;

#[derive(Clone, Copy)]
enum Refresh<'a> {
    None,
    One(&'a str),
    All,
}

impl Refresh<'_> {
    fn matches(self, address: &str) -> bool {
        match self {
            Self::None => false,
            Self::One(target) => target == address,
            Self::All => true,
        }
    }
}

/// Subscribes directly to `NewIconThemePath` signals on the session bus,
/// since `system-tray` ignores them.
async fn subscribe_theme_paths(bus: &zbus::Connection) -> Option<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .member("NewIconThemePath")
        .ok()?
        .build();
    match zbus::MessageStream::for_match_rule(rule, bus, Some(32)).await {
        Ok(stream) => Some(stream),
        Err(e) => {
            debug!(error = %e, "failed to subscribe to NewIconThemePath signals");
            None
        }
    }
}

/// Finds the image of the items that name their icon in a folder of their
/// own, asking the item for its current folder when the one known lacks it
/// or was initially empty.
async fn find_icon_files(
    items: &mut [TrayItem],
    theme_paths: &mut ThemePaths,
    bus: Option<&zbus::Connection>,
    refresh: Refresh<'_>,
) {
    find_icon_files_with(items, theme_paths, refresh, |address| {
        let address = address.to_owned();
        async move {
            let bus = bus?;
            match icon_theme_path(bus, &address).await {
                Ok(path) => Some(path),
                Err(e) => {
                    debug!(error = %e, address, "failed to read the icon folder of tray item");
                    None
                }
            }
        }
    })
    .await;
}

async fn find_icon_files_with<F, Fut>(
    items: &mut [TrayItem],
    theme_paths: &mut ThemePaths,
    refresh: Refresh<'_>,
    mut fetch_theme_path: F,
) where
    F: FnMut(&str) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    for item in items {
        if item.pixmap.is_some() {
            continue;
        }
        let Some(name) = &item.icon_name else {
            continue;
        };
        if Path::new(name).is_absolute() {
            let path = PathBuf::from(name);
            if path.is_file() {
                item.icon_file = Some(path);
            }
            continue;
        }
        let cached = theme_paths.get(&item.address);
        let known = cached
            .or(item.icon_theme_path.as_ref())
            .filter(|path| !path.is_empty());
        if let Some(dir) = known {
            item.icon_file = find_in_theme_path(Path::new(dir), name);
            if item.icon_file.is_some() {
                continue;
            }
        }
        let should_query = known.is_some() || cached.is_none() || refresh.matches(&item.address);
        if !should_query {
            continue;
        }
        match fetch_theme_path(&item.address).await {
            Some(path) => {
                if !path.is_empty() {
                    item.icon_file = find_in_theme_path(Path::new(&path), name);
                    if item.icon_file.is_none() {
                        debug!(address = %item.address, name, path, "tray icon not in its folder");
                    }
                }
                theme_paths.insert(item.address.clone(), path);
            }
            None => {
                theme_paths.entry(item.address.clone()).or_default();
            }
        }
    }
}

/// The bus name and object path of an item, from its address: the bus name,
/// maybe followed by the path.
fn destination(address: &str) -> (&str, String) {
    match address.split_once('/') {
        Some((name, path)) => (name, format!("/{path}")),
        None => (address, String::from("/StatusNotifierItem")),
    }
}

/// Reads the current icon folder of an item.
async fn icon_theme_path(bus: &zbus::Connection, address: &str) -> zbus::Result<String> {
    let (destination, path) = destination(address);
    icon_theme_path_at(bus, destination, Some(path.as_str())).await
}

async fn icon_theme_path_at(
    bus: &zbus::Connection,
    destination: &str,
    path: Option<&str>,
) -> zbus::Result<String> {
    let path = path.unwrap_or("/StatusNotifierItem");
    let reply = bus
        .call_method(
            Some(destination),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.StatusNotifierItem", "IconThemePath"),
        )
        .await?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
    Ok(String::try_from(value)?)
}

/// Asks an item to show its own menu, which `system-tray` cannot do.
async fn context_menu(address: &str, x: i32, y: i32) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let (destination, path) = destination(address);
    connection
        .call_method(
            Some(destination),
            path.as_str(),
            Some("org.kde.StatusNotifierItem"),
            "ContextMenu",
            &(x, y),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "coquille-tray-backend-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tray_item(address: &str, icon_name: &str, icon_theme_path: Option<&str>) -> TrayItem {
        TrayItem {
            address: address.to_owned(),
            id: "discord_status_icon_1".to_owned(),
            title: String::new(),
            tooltip: Some("Discord".to_owned()),
            icon_name: Some(icon_name.to_owned()),
            icon_theme_path: icon_theme_path.map(ToOwned::to_owned),
            icon_file: None,
            pixmap: None,
            menu_path: None,
            menu: None,
        }
    }

    #[tokio::test]
    async fn queries_theme_path_when_initially_empty() {
        let dir = scratch("initially-empty");
        let icon = dir.join("status_icon_1.png");
        std::fs::write(&icon, b"png").unwrap();

        let mut items = vec![tray_item(":1.384", "status_icon_1", None)];
        let mut theme_paths = ThemePaths::new();
        let dir_str = dir.to_string_lossy().into_owned();

        find_icon_files_with(&mut items, &mut theme_paths, Refresh::None, |_| {
            let dir_str = dir_str.clone();
            async move { Some(dir_str) }
        })
        .await;

        assert_eq!(items[0].icon_file, Some(icon));
        assert_eq!(theme_paths.get(":1.384"), Some(&dir_str));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn refreshes_theme_path_when_electron_rotates_folder() {
        let first = scratch("rotate-first");
        let second = scratch("rotate-second");
        let second_icon = second.join("status_icon_1.png");
        std::fs::write(&second_icon, b"png").unwrap();

        let mut items = vec![tray_item(
            ":1.384",
            "status_icon_1",
            Some(first.to_str().unwrap()),
        )];
        let mut theme_paths = ThemePaths::new();
        theme_paths.insert(":1.384".to_owned(), first.to_string_lossy().into_owned());
        let second_str = second.to_string_lossy().into_owned();

        find_icon_files_with(&mut items, &mut theme_paths, Refresh::None, |_| {
            let second_str = second_str.clone();
            async move { Some(second_str) }
        })
        .await;

        assert_eq!(items[0].icon_file, Some(second_icon));
        assert_eq!(theme_paths.get(":1.384"), Some(&second_str));
        std::fs::remove_dir_all(&first).unwrap();
        std::fs::remove_dir_all(&second).unwrap();
    }

    #[tokio::test]
    async fn does_not_requery_empty_theme_path_on_unrelated_events() {
        let mut items = vec![tray_item(":1.50", "network-wireless", None)];
        let mut theme_paths = ThemePaths::new();
        let mut calls = 0usize;

        find_icon_files_with(&mut items, &mut theme_paths, Refresh::None, |_| {
            calls += 1;
            async { Some(String::new()) }
        })
        .await;
        assert_eq!(calls, 1);
        assert_eq!(items[0].icon_file, None);

        find_icon_files_with(&mut items, &mut theme_paths, Refresh::None, |_| {
            calls += 1;
            async { Some(String::new()) }
        })
        .await;
        assert_eq!(calls, 1);

        // An icon update for this item does re-check its theme path.
        find_icon_files_with(&mut items, &mut theme_paths, Refresh::One(":1.50"), |_| {
            calls += 1;
            async { Some(String::new()) }
        })
        .await;
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn absolute_icon_name_resolves_directly() {
        let dir = scratch("absolute");
        let icon = dir.join("tray.png");
        std::fs::write(&icon, b"png").unwrap();

        let mut items = vec![tray_item(":1.99", icon.to_str().unwrap(), None)];
        let mut theme_paths = ThemePaths::new();
        let mut calls = 0usize;

        find_icon_files_with(&mut items, &mut theme_paths, Refresh::All, |_| {
            calls += 1;
            async { Some(String::new()) }
        })
        .await;

        assert_eq!(calls, 0);
        assert_eq!(items[0].icon_file, Some(icon));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
