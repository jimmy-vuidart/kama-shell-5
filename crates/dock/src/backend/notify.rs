//! `org.freedesktop.Notifications` on the session bus, same pattern as the tray.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream, StreamExt};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};
use zbus::fdo::{self, RequestNameFlags, RequestNameReply};
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

        let ours = in_coquille_session();
        if !claim_bus(&connection, ours).await {
            info!("notifications: {BUS_NAME} belongs to the desktop this dock runs in; not served");
            return;
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

        let dbus = match fdo::DBusProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(error) => {
                warn!(%error, "notifications: no DBus proxy");
                return;
            }
        };
        let mut acquired = match dbus.receive_name_acquired().await {
            Ok(stream) => stream,
            Err(error) => {
                warn!(%error, "notifications: cannot watch NameAcquired");
                return;
            }
        };
        let mut lost = match dbus.receive_name_lost().await {
            Ok(stream) => stream,
            Err(error) => {
                warn!(%error, "notifications: cannot watch NameLost");
                return;
            }
        };

        loop {
            tokio::select! {
                Some(signal) = acquired.next() => {
                    if signal.args().is_ok_and(|args| args.name() == BUS_NAME) {
                        info!("notifications: owning {BUS_NAME}");
                    }
                }
                Some(signal) = lost.next() => {
                    if signal.args().is_ok_and(|args| args.name() == BUS_NAME) {
                        warn!("notifications: lost {BUS_NAME}; rejoining the queue");
                        claim_bus(&connection, ours).await;
                    }
                }
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

/// Whether coquille is the desktop of this session (`coquille-session` and
/// the compositor on real screens say so). Nested in another desktop, the
/// session bus is that desktop's: its notification server is left alone.
fn in_coquille_session() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|desktops| desktops.split(':').any(|desktop| desktop == "coquille"))
}

/// Asks for the name. In a coquille session, a server left over gives it up
/// when it allows it, or the dock waits in line behind it; nested, the dock
/// only takes a free name. `false` when it is not ours to serve.
async fn claim_bus(connection: &Connection, ours: bool) -> bool {
    let flags = if ours {
        RequestNameFlags::ReplaceExisting.into()
    } else {
        RequestNameFlags::DoNotQueue.into()
    };
    match connection.request_name_with_flags(BUS_NAME, flags).await {
        Ok(RequestNameReply::InQueue) => {
            warn!("notifications: {BUS_NAME} is taken; waiting in line");
            true
        }
        Ok(RequestNameReply::Exists) => false,
        Ok(_) => {
            info!("notifications: owning {BUS_NAME}");
            true
        }
        Err(error) => {
            warn!(%error, "notifications: could not request {BUS_NAME}");
            ours
        }
    }
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
        let id = id_for_notify(&self.next_id, replaces_id);

        let (icon, image) = icons_from_hints(&app_icon, &hints);
        // A critical notice stays until it is dealt with.
        let critical = hint_u8(&hints, "urgency") == Some(2);
        let notice = Notice {
            id,
            app: app_name,
            summary,
            body,
            icon,
            actions: actions_from_flat(&actions),
            expire_ms: expire_from_timeout(expire_timeout).filter(|_| !critical),
            image,
            received: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            desktop_entry: hint_string(&hints, &["desktop-entry"]),
            transient: hint_bool(&hints, "transient"),
            resident: hint_bool(&hints, "resident"),
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

fn id_for_notify(counter: &AtomicU32, replaces_id: u32) -> u32 {
    if replaces_id == 0 {
        next_id(counter)
    } else {
        let _ = counter.fetch_max(replaces_id.saturating_add(1), Ordering::Relaxed);
        replaces_id
    }
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

fn hint_u8(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    u8::try_from(hints.get(key)?).ok()
}

/// A boolean hint; some clients send it as a number.
fn hint_bool(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    let Some(value) = hints.get(key) else {
        return false;
    };
    bool::try_from(value)
        .or_else(|_| u8::try_from(value).map(|n| n != 0))
        .or_else(|_| i32::try_from(value).map(|n| n != 0))
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn owned_image(
        width: i32,
        height: i32,
        rowstride: i32,
        has_alpha: bool,
        data: Vec<u8>,
    ) -> OwnedValue {
        let channels = if has_alpha { 4 } else { 3 };
        let value = Value::from((width, height, rowstride, has_alpha, 8i32, channels, data));
        OwnedValue::try_from(value).expect("image-data value")
    }

    #[test]
    fn next_id_skips_zero_and_replaces_advances() {
        let counter = AtomicU32::new(0);
        assert_eq!(next_id(&counter), 1);
        let counter = AtomicU32::new(1);
        assert_eq!(id_for_notify(&counter, 0), 1);
        assert_eq!(id_for_notify(&counter, 0), 2);
        assert_eq!(id_for_notify(&counter, 42), 42);
        assert_eq!(id_for_notify(&counter, 0), 43);
    }

    #[test]
    fn icons_prefer_image_data_over_path_and_app_icon() {
        let mut hints = HashMap::new();
        hints.insert(
            "image-path".into(),
            OwnedValue::try_from(Value::from("/tmp/hint.png")).unwrap(),
        );
        hints.insert(
            "image-data".into(),
            owned_image(1, 1, 3, false, vec![9, 8, 7]),
        );
        let (icon, image) = icons_from_hints("dialog-info", &hints);
        assert_eq!(icon, Some(Icon::Named("dialog-info".into())));
        assert!(matches!(
            image,
            Some(Icon::Pixels {
                width: 1,
                height: 1,
                ..
            })
        ));

        let mut path_only = HashMap::new();
        path_only.insert(
            "image_path".into(),
            OwnedValue::try_from(Value::from("/tmp/hint.png")).unwrap(),
        );
        let (icon, image) = icons_from_hints("file:///tmp/app.png", &path_only);
        assert_eq!(icon, Some(Icon::File("file:///tmp/app.png".into())));
        assert_eq!(image, Some(Icon::File("/tmp/hint.png".into())));

        let (icon, image) = icons_from_hints("/usr/share/icon.png", &HashMap::new());
        assert_eq!(icon, Some(Icon::File("/usr/share/icon.png".into())));
        assert!(image.is_none());
    }

    #[test]
    fn hints_read_urgency_and_flags() {
        let mut hints = HashMap::new();
        hints.insert("urgency".into(), OwnedValue::from(2u8));
        hints.insert("transient".into(), OwnedValue::from(true));
        hints.insert("resident".into(), OwnedValue::from(1i32));
        assert_eq!(hint_u8(&hints, "urgency"), Some(2));
        assert!(hint_bool(&hints, "transient"));
        assert!(hint_bool(&hints, "resident"));
        assert!(!hint_bool(&hints, "missing"));
    }

    #[test]
    fn decode_image_data_rejects_oversize_and_accepts_tuple() {
        let ok = owned_image(1, 1, 3, false, vec![1, 2, 3]);
        assert!(decode_image_data(&ok).is_some());
        let too_big = owned_image(2, 2, 6, false, vec![0; 4 * 512 * 512 + 1]);
        assert!(decode_image_data(&too_big).is_none());
    }
}
