//! `org.freedesktop.Notifications` on the session bus, same pattern as the tray.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, interface};

use super::{Backend, Request, Update};
use crate::model::notify::{
    CloseReason, Icon, Notice, actions_from_flat, expire_from_timeout, pixels_from_image_data,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const SPEC_VERSION: &str = "1.2";

pub fn subscription() -> Subscription<Update> {
    Subscription::run(service)
}

fn service() -> impl FutureStream<Item = Update> {
    iced::stream::channel(32, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output
            .send(Update::Link(Backend::Notifications, requests))
            .await;

        let connection = match Connection::session().await {
            Ok(connection) => connection,
            Err(error) => {
                warn!(%error, "notifications: no session bus");
                return;
            }
        };

        let (to_ui, mut incoming) = unbounded_channel::<Update>();
        let server = Server {
            next_id: AtomicU32::new(1),
            to_ui,
        };

        if let Err(error) = connection.object_server().at(OBJECT_PATH, server).await {
            warn!(%error, "notifications: could not export the object");
            return;
        }

        loop {
            match connection.request_name(BUS_NAME).await {
                Ok(_) => {
                    info!("notifications: owning {BUS_NAME}");
                    break;
                }
                Err(error) => {
                    warn!(
                        %error,
                        "notifications: {BUS_NAME} is taken; retrying"
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        let iface = match connection
            .object_server()
            .interface::<_, Server>(OBJECT_PATH)
            .await
        {
            Ok(iface) => iface,
            Err(error) => {
                warn!(%error, "notifications: no interface after claiming the name");
                return;
            }
        };
        let emitter = iface.signal_emitter().clone();

        loop {
            tokio::select! {
                Some(update) = incoming.recv() => {
                    let _ = output.send(update).await;
                }
                Some(req) = asked.recv() => {
                    match req {
                        Request::NotifyClose { id, reason } => {
                            if let Err(error) =
                                Server::notification_closed(&emitter, id, reason).await
                            {
                                debug!(%error, id, "notifications: NotificationClosed failed");
                            }
                        }
                        Request::NotifyAction { id, key } => {
                            if let Err(error) =
                                Server::action_invoked(&emitter, id, &key).await
                            {
                                debug!(%error, id, "notifications: ActionInvoked failed");
                            }
                        }
                        _ => {}
                    }
                }
                else => break,
            }
        }
    })
}

struct Server {
    next_id: AtomicU32,
    to_ui: UnboundedSender<Update>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
    async fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "body-markup".into(),
            "actions".into(),
            "action-icons".into(),
            "icon-static".into(),
            "persistence".into(),
        ]
    }

    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "coquille".into(),
            "coquille-dock".into(),
            env!("CARGO_PKG_VERSION").into(),
            SPEC_VERSION.into(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id == 0 {
            next_id(&self.next_id)
        } else {
            let _ = self
                .next_id
                .fetch_max(replaces_id.saturating_add(1), Ordering::Relaxed);
            replaces_id
        };

        let (icon, image) = icons_from_hints(&app_icon, &hints);
        let notice = Notice {
            id,
            app: app_name,
            summary,
            body,
            icon,
            actions: actions_from_flat(&actions),
            expire_ms: expire_from_timeout(expire_timeout),
            image,
        };

        let _ = self.to_ui.send(Update::Notify(notice));
        id
    }

    async fn close_notification(&self, id: u32) {
        let _ = self.to_ui.send(Update::NotifyClosed {
            id,
            reason: CloseReason::Requested.as_u32(),
        });
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

fn next_id(counter: &AtomicU32) -> u32 {
    loop {
        let id = counter.fetch_add(1, Ordering::Relaxed);
        if id != 0 {
            return id;
        }
    }
}

fn icons_from_hints(
    app_icon: &str,
    hints: &HashMap<String, OwnedValue>,
) -> (Option<Icon>, Option<Icon>) {
    let image = hint(hints, &["image-data", "image_data", "icon_data"])
        .and_then(|value| decode_image_data(&value));
    let path = hint_string(hints, &["image-path", "image_path"]);
    let image = image.or_else(|| path.map(Icon::File));
    let icon = if app_icon.is_empty() {
        None
    } else if app_icon.starts_with('/') || app_icon.starts_with("file:") {
        Some(Icon::File(app_icon.to_string()))
    } else {
        Some(Icon::Named(app_icon.to_string()))
    };
    (icon, image)
}

fn hint(hints: &HashMap<String, OwnedValue>, keys: &[&str]) -> Option<OwnedValue> {
    keys.iter().find_map(|key| hints.get(*key).cloned())
}

fn hint_string(hints: &HashMap<String, OwnedValue>, keys: &[&str]) -> Option<String> {
    let value = hint(hints, keys)?;
    String::try_from(value).ok()
}

fn decode_image_data(value: &OwnedValue) -> Option<Icon> {
    let value = Value::try_from(value).ok()?;
    let decoded: (i32, i32, i32, bool, i32, i32, Vec<u8>) = value.try_into().ok()?;
    let (width, height, rowstride, has_alpha, bits, channels, data) = decoded;
    const MAX_BYTES: usize = 4 * 512 * 512;
    if data.len() > MAX_BYTES {
        return None;
    }
    pixels_from_image_data(width, height, rowstride, has_alpha, bits, channels, &data)
}
