//! The system tray service: watches for StatusNotifierItem (SNI) instances
//! and their DBusMenus over D-Bus using `system-tray`, and holds the X11
//! tray (`xembed`) for Wine and other X11 programs.
//! Converts events into dock updates and dispatches user actions back to items.

use std::sync::{Arc, Mutex};

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream};
use system_tray::client::{ActivateRequest, Client};
use system_tray::data::BaseMap;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, info, warn};

use super::{Backend, Request, Update, xembed};
use crate::model::tray::{TrayItem, x11_window};

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

        // Initial items if any already exist.
        let initial_items = all(&x11_items);
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
                        Some(Ok(_ev)) => {
                            debug!("tray event received");
                            let _ = output.send(Update::Tray(all(&x11_items))).await;
                        }
                        Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                            debug!("tray event stream lagged by {n}");
                            let _ = output.send(Update::Tray(all(&x11_items))).await;
                        }
                        Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) | None => {
                            info!("tray event channel closed");
                            events = None;
                        }
                    }
                }
                Some(icons) = x11_changed.recv() => {
                    x11_items = icons.into_iter().map(TrayItem::from_x11).collect();
                    let _ = output.send(Update::Tray(all(&x11_items))).await;
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

/// Asks an item to show its own menu, which `system-tray` cannot do.
async fn context_menu(address: &str, x: i32, y: i32) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    // `address` is the bus name of the item, maybe followed by its path.
    let (destination, path) = match address.split_once('/') {
        Some((name, path)) => (name, format!("/{path}")),
        None => (address, String::from("/StatusNotifierItem")),
    };
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
