//! The system tray service: watches for StatusNotifierItem (SNI) instances
//! and their DBusMenus over D-Bus using `system-tray`.
//! Converts events into dock updates and dispatches user actions back to items.

use std::sync::{Arc, Mutex};

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream};
use system_tray::client::{ActivateRequest, Client};
use system_tray::data::BaseMap;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, info, warn};

use super::{Backend, Request, Update};
use crate::model::tray::TrayItem;

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

        let client = match Client::new().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "failed to initialize system tray client");
                return;
            }
        };

        info!("system tray client started successfully");
        let mut events = client.subscribe();
        let items_map = client.items();

        // Initial items if any already exist.
        let initial_items = collect_tray_items(&items_map);
        if !initial_items.is_empty() {
            let _ = output.send(Update::Tray(initial_items)).await;
        }

        loop {
            tokio::select! {
                event = events.recv() => {
                    match event {
                        Ok(_ev) => {
                            debug!("tray event received");
                            let items = collect_tray_items(&items_map);
                            let _ = output.send(Update::Tray(items)).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            debug!("tray event stream lagged by {n}");
                            let items = collect_tray_items(&items_map);
                            let _ = output.send(Update::Tray(items)).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("tray event channel closed");
                            break;
                        }
                    }
                }
                Some(req) = asked.recv() => {
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
