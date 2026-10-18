//! The Secret Service on the session bus: the objects of the freedesktop
//! specification (`org.freedesktop.Secret.Service`, `Collection`, `Item`,
//! `Session`) and the portal backend that gives sandboxed applications a
//! secret of their own (`org.freedesktop.impl.portal.Secret`).
//!
//! The keyring is always open: `Unlock` has nothing to do, `Lock` does
//! nothing, and no method ever answers with a prompt (the path `/`). Every
//! lasting change is written to disk before the answer goes back.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, error, info, warn};
use zbus::message::Header;
use zbus::names::{OwnedUniqueName, UniqueName};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, fdo, interface};

use crate::store::{Attributes, ItemRef, Keyring, NewItem, StoreError};
use crate::transport::{Transport, TransportError, random};
use crate::vault::Vault;

pub const BUS_NAME: &str = "org.freedesktop.secrets";
const ROOT: &str = "/org/freedesktop/secrets";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const COLLECTION_LABEL: &str = "org.freedesktop.Secret.Collection.Label";
const ITEM_LABEL: &str = "org.freedesktop.Secret.Item.Label";
const ITEM_ATTRIBUTES: &str = "org.freedesktop.Secret.Item.Attributes";
/// The size of the secret the portal makes for an application.
const PORTAL_SECRET: usize = 64;

/// A secret on the bus: session, parameters, value, content type.
type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.Secret.Error")]
pub enum SecretError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NoSuchObject(String),
    NoSession(String),
}

impl SecretError {
    fn fdo(error: fdo::Error) -> Self {
        Self::ZBus(zbus::Error::FDO(Box::new(error)))
    }
}

impl From<StoreError> for SecretError {
    fn from(error: StoreError) -> Self {
        Self::NoSuchObject(error.to_string())
    }
}

impl From<TransportError> for SecretError {
    fn from(error: TransportError) -> Self {
        match error {
            // libsecret falls back to `plain` on this one.
            TransportError::Unsupported(_) => {
                Self::fdo(fdo::Error::NotSupported(error.to_string()))
            }
            _ => Self::fdo(fdo::Error::InvalidArgs(error.to_string())),
        }
    }
}

impl From<zvariant::Error> for SecretError {
    fn from(error: zvariant::Error) -> Self {
        Self::ZBus(error.into())
    }
}

/// Everything the objects share.
pub struct Keeper {
    keyring: Keyring,
    vault: Vault,
    sessions: HashMap<u32, Session>,
    next_session: u32,
}

struct Session {
    transport: Transport,
    /// The client that opened it: the session ends when it leaves the bus.
    owner: Option<OwnedUniqueName>,
}

#[derive(Clone)]
pub struct Shared(Arc<Mutex<Keeper>>);

impl Shared {
    pub fn new(keyring: Keyring, vault: Vault) -> Self {
        Self(Arc::new(Mutex::new(Keeper {
            keyring,
            vault,
            sessions: HashMap::new(),
            next_session: 1,
        })))
    }

    fn with<R>(&self, f: impl FnOnce(&mut Keeper) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Forgets the sessions of a client that left the bus.
    pub async fn client_left(&self, connection: &Connection, name: &UniqueName<'_>) {
        let gone: Vec<u32> = self.with(|keeper| {
            let gone: Vec<u32> = keeper
                .sessions
                .iter()
                .filter(|(_, session)| session.owner.as_deref() == Some(name))
                .map(|(id, _)| *id)
                .collect();
            for id in &gone {
                keeper.sessions.remove(id);
            }
            gone
        });
        for id in gone {
            remove::<SessionObject>(connection, &session_path(id)).await;
        }
    }
}

impl Keeper {
    /// Writes the keyring. A failure is reported to the client: what it
    /// stored would not be there in the next session.
    fn save(&self) -> Result<(), SecretError> {
        self.vault.save(&self.keyring.saved()).map_err(|error| {
            error!(%error, "cannot write the keyring");
            SecretError::fdo(fdo::Error::Failed(format!(
                "cannot write the keyring: {error}"
            )))
        })
    }

    fn transport(&self, session: &ObjectPath<'_>) -> Result<&Transport, SecretError> {
        session_id(session)
            .and_then(|id| self.sessions.get(&id))
            .map(|session| &session.transport)
            .ok_or_else(|| SecretError::NoSession(session.to_string()))
    }

    fn secret_of(&self, item: &ItemRef, session: &ObjectPath<'_>) -> Result<Secret, SecretError> {
        let found = self
            .keyring
            .item(item)
            .ok_or_else(|| SecretError::NoSuchObject(item_path(item).to_string()))?;
        let (parameters, value) = self.transport(session)?.encrypt(&found.secret)?;
        Ok((
            session.to_owned().into(),
            parameters,
            value,
            found.content_type.clone(),
        ))
    }

    fn open(&self, secret: &Secret) -> Result<(Vec<u8>, String), SecretError> {
        let (session, parameters, value, content_type) = secret;
        let plain = self.transport(session)?.decrypt(parameters, value)?;
        Ok((plain, content_type.clone()))
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn path(text: String) -> OwnedObjectPath {
    OwnedObjectPath::try_from(text).expect("paths are made of valid elements")
}

/// The path that means "nothing", and "no prompt needed".
fn nothing() -> OwnedObjectPath {
    path("/".to_owned())
}

fn collection_path(name: &str) -> OwnedObjectPath {
    path(format!("{ROOT}/collection/{name}"))
}

fn item_path(item: &ItemRef) -> OwnedObjectPath {
    path(format!("{ROOT}/collection/{}/{}", item.collection, item.id))
}

fn session_path(id: u32) -> OwnedObjectPath {
    path(format!("{ROOT}/session/s{id}"))
}

/// Aliases get an object too, for clients that use `aliases/default`
/// directly; only those whose name is a valid path element.
fn alias_path(alias: &str) -> Option<OwnedObjectPath> {
    let valid = !alias.is_empty() && alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| path(format!("{ROOT}/aliases/{alias}")))
}

fn session_id(path: &ObjectPath<'_>) -> Option<u32> {
    path.strip_prefix(&format!("{ROOT}/session/s"))?
        .parse()
        .ok()
}

/// The item a path names, aliases included.
fn parse_item(keyring: &Keyring, path: &ObjectPath<'_>) -> Option<ItemRef> {
    let rest = path.strip_prefix(ROOT)?;
    let (collection, id) = rest.rsplit_once('/')?;
    let collection = if let Some(name) = collection.strip_prefix("/collection/") {
        name
    } else {
        keyring.alias(collection.strip_prefix("/aliases/")?)?
    };
    Some(ItemRef {
        collection: collection.to_owned(),
        id: id.parse().ok()?,
    })
}

/// The collection a path names, aliases included.
fn parse_collection(keyring: &Keyring, path: &ObjectPath<'_>) -> Option<String> {
    let rest = path.strip_prefix(ROOT)?;
    let name = if let Some(name) = rest.strip_prefix("/collection/") {
        name
    } else {
        keyring.alias(rest.strip_prefix("/aliases/")?)?
    };
    keyring.collection(name).map(|_| name.to_owned())
}

fn property<T>(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    let value = properties.get(key)?.try_clone().ok()?;
    T::try_from(value).ok()
}

fn to_attributes(map: HashMap<String, String>) -> Attributes {
    map.into_iter().collect()
}

fn from_attributes(attributes: &Attributes) -> HashMap<String, String> {
    attributes.clone().into_iter().collect()
}

async fn remove<I: zbus::object_server::Interface>(connection: &Connection, path: &ObjectPath<'_>) {
    if let Err(error) = connection.object_server().remove::<I, _>(path).await {
        debug!(%error, %path, "object already gone");
    }
}

async fn add<I: zbus::object_server::Interface>(
    connection: &Connection,
    path: &ObjectPath<'_>,
    object: I,
) {
    if let Err(error) = connection.object_server().at(path, object).await {
        warn!(%error, %path, "cannot show an object on the bus");
    }
}

/// Shows the whole keyring on the bus, when it starts.
pub async fn serve(connection: &Connection, shared: &Shared) -> zbus::Result<()> {
    let server = connection.object_server();
    server
        .at(
            ROOT,
            ServiceObject {
                shared: shared.clone(),
            },
        )
        .await?;
    server
        .at(
            PORTAL_PATH,
            PortalObject {
                shared: shared.clone(),
            },
        )
        .await?;
    let (collections, aliases) = shared.with(|keeper| {
        let collections: Vec<String> = keeper
            .keyring
            .collections()
            .map(|(name, _)| name.to_owned())
            .collect();
        let aliases: Vec<String> = keeper
            .keyring
            .aliases()
            .map(|(alias, _)| alias.to_owned())
            .collect();
        (collections, aliases)
    });
    for name in collections {
        add_collection(connection, shared, &name).await;
    }
    for alias in aliases {
        add_alias(connection, shared, &alias).await;
    }
    Ok(())
}

async fn add_collection(connection: &Connection, shared: &Shared, name: &str) {
    let object = CollectionObject {
        shared: shared.clone(),
        target: Target::Name(name.to_owned()),
    };
    add(connection, &collection_path(name), object).await;
    let items: Vec<ItemRef> = shared.with(|keeper| {
        let ids = keeper.keyring.search_in(name, &Attributes::new());
        ids.unwrap_or_default()
            .into_iter()
            .map(|id| ItemRef {
                collection: name.to_owned(),
                id,
            })
            .collect()
    });
    for item in items {
        add_item(connection, shared, item).await;
    }
}

async fn add_alias(connection: &Connection, shared: &Shared, alias: &str) {
    if let Some(path) = alias_path(alias) {
        let object = CollectionObject {
            shared: shared.clone(),
            target: Target::Alias(alias.to_owned()),
        };
        add(connection, &path, object).await;
    }
}

async fn add_item(connection: &Connection, shared: &Shared, item: ItemRef) {
    let path = item_path(&item);
    let object = ItemObject {
        shared: shared.clone(),
        item,
    };
    add(connection, &path, object).await;
}

/// A new item, or a replaced one, is announced by its collection.
async fn announce_item(connection: &Connection, item: &ItemRef, created: bool) {
    let emitter = match SignalEmitter::new(connection, collection_path(&item.collection)) {
        Ok(emitter) => emitter,
        Err(error) => return warn!(%error, "cannot announce an item"),
    };
    let path = item_path(item);
    let sent = if created {
        CollectionObject::item_created(&emitter, path.as_ref()).await
    } else {
        CollectionObject::item_changed(&emitter, path.as_ref()).await
    };
    if let Err(error) = sent {
        warn!(%error, "cannot announce an item");
    }
}

/// The keyring as a whole.
struct ServiceObject {
    shared: Shared,
}

#[interface(name = "org.freedesktop.Secret.Service")]
impl ServiceObject {
    async fn open_session(
        &self,
        algorithm: &str,
        input: Value<'_>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(OwnedValue, OwnedObjectPath), SecretError> {
        let input = Vec::<u8>::try_from(input).unwrap_or_default();
        let (transport, output) = Transport::open(algorithm, &input)?;
        let owner = header.sender().map(|name| name.to_owned().into());
        let id = self.shared.with(|keeper| {
            let id = keeper.next_session;
            keeper.next_session += 1;
            keeper.sessions.insert(id, Session { transport, owner });
            id
        });
        let path = session_path(id);
        let object = SessionObject {
            shared: self.shared.clone(),
            id,
        };
        add(connection, &path, object).await;
        debug!(algorithm, %path, "session opened");
        let output = if output.is_empty() {
            Value::from("").try_to_owned()?
        } else {
            Value::from(output).try_to_owned()?
        };
        Ok((output, path))
    }

    async fn create_collection(
        &self,
        properties: HashMap<String, OwnedValue>,
        alias: &str,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), SecretError> {
        let label: String = property(&properties, COLLECTION_LABEL).unwrap_or_default();
        let (name, created) = self.shared.with(|keeper| {
            let made = keeper.keyring.create_collection(&label, alias, now());
            keeper.save().map(|()| made)
        })?;
        if created {
            info!(name, "collection created");
            add_collection(connection, &self.shared, &name).await;
            add_alias(connection, &self.shared, alias).await;
            let emitter = SignalEmitter::new(connection, ROOT)?;
            Self::collection_created(&emitter, collection_path(&name).as_ref()).await?;
        }
        Ok((collection_path(&name), nothing()))
    }

    async fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        let wanted = to_attributes(attributes);
        let found = self.shared.with(|keeper| keeper.keyring.search(&wanted));
        (found.iter().map(item_path).collect(), Vec::new())
    }

    /// Everything is open already.
    async fn unlock(
        &self,
        objects: Vec<OwnedObjectPath>,
    ) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        (objects, nothing())
    }

    /// The keyring stays open for the whole session: nothing is locked.
    async fn lock(&self, objects: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        debug!(
            count = objects.len(),
            "asked to lock: the keyring stays open"
        );
        (Vec::new(), nothing())
    }

    async fn get_secrets(
        &self,
        items: Vec<OwnedObjectPath>,
        session: OwnedObjectPath,
    ) -> Result<HashMap<OwnedObjectPath, Secret>, SecretError> {
        self.shared.with(|keeper| {
            keeper.transport(&session)?;
            let mut secrets = HashMap::new();
            for path in items {
                let Some(item) = parse_item(&keeper.keyring, &path) else {
                    continue;
                };
                if let Ok(secret) = keeper.secret_of(&item, &session) {
                    secrets.insert(path, secret);
                }
            }
            Ok(secrets)
        })
    }

    async fn read_alias(&self, name: &str) -> OwnedObjectPath {
        self.shared
            .with(|keeper| keeper.keyring.alias(name).map(collection_path))
            .unwrap_or_else(nothing)
    }

    async fn set_alias(
        &self,
        name: &str,
        collection: OwnedObjectPath,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(), SecretError> {
        let removing = collection.as_str() == "/";
        self.shared.with(|keeper| {
            let target = if removing {
                None
            } else {
                Some(
                    parse_collection(&keeper.keyring, &collection)
                        .ok_or_else(|| SecretError::NoSuchObject(collection.to_string()))?,
                )
            };
            keeper.keyring.set_alias(name, target.as_deref())?;
            keeper.save()
        })?;
        match (removing, alias_path(name)) {
            (true, Some(path)) => remove::<CollectionObject>(connection, &path).await,
            (false, _) => add_alias(connection, &self.shared, name).await,
            (true, None) => {}
        }
        Ok(())
    }

    #[zbus(property)]
    async fn collections(&self) -> Vec<OwnedObjectPath> {
        self.shared.with(|keeper| {
            keeper
                .keyring
                .collections()
                .map(|(name, _)| collection_path(name))
                .collect()
        })
    }

    #[zbus(signal)]
    async fn collection_created(
        emitter: &SignalEmitter<'_>,
        collection: ObjectPath<'_>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn collection_deleted(
        emitter: &SignalEmitter<'_>,
        collection: ObjectPath<'_>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn collection_changed(
        emitter: &SignalEmitter<'_>,
        collection: ObjectPath<'_>,
    ) -> zbus::Result<()>;
}

/// What a collection object stands for: a collection, or an alias that
/// names one (followed at each call, as aliases can change).
enum Target {
    Name(String),
    Alias(String),
}

struct CollectionObject {
    shared: Shared,
    target: Target,
}

impl CollectionObject {
    fn name(&self, keyring: &Keyring) -> Result<String, SecretError> {
        let name = match &self.target {
            Target::Name(name) => Some(name.as_str()),
            Target::Alias(alias) => keyring.alias(alias),
        };
        name.filter(|name| keyring.collection(name).is_some())
            .map(str::to_owned)
            .ok_or_else(|| SecretError::NoSuchObject("no such collection".to_owned()))
    }

    fn read<R>(&self, f: impl FnOnce(&crate::store::Collection) -> R) -> Result<R, SecretError> {
        self.shared.with(|keeper| {
            let name = self.name(&keeper.keyring)?;
            let collection = keeper
                .keyring
                .collection(&name)
                .ok_or(StoreError::NoCollection)?;
            Ok(f(collection))
        })
    }
}

#[interface(name = "org.freedesktop.Secret.Collection")]
impl CollectionObject {
    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<OwnedObjectPath, SecretError> {
        let (name, items, aliases) = self.shared.with(|keeper| {
            let name = self.name(&keeper.keyring)?;
            let before: Vec<String> = keeper
                .keyring
                .aliases()
                .map(|(a, _)| a.to_owned())
                .collect();
            let removed = keeper.keyring.delete_collection(&name)?;
            keeper.save()?;
            let gone: Vec<String> = before
                .into_iter()
                .filter(|alias| keeper.keyring.alias(alias).is_none())
                .collect();
            let items: Vec<ItemRef> = removed
                .items
                .keys()
                .map(|id| ItemRef {
                    collection: name.clone(),
                    id: *id,
                })
                .collect();
            Ok::<_, SecretError>((name, items, gone))
        })?;
        info!(name, "collection deleted");
        for item in &items {
            remove::<ItemObject>(connection, &item_path(item)).await;
        }
        for alias in aliases.iter().filter_map(|alias| alias_path(alias)) {
            remove::<CollectionObject>(connection, &alias).await;
        }
        let path = collection_path(&name);
        remove::<CollectionObject>(connection, &path).await;
        let emitter = SignalEmitter::new(connection, ROOT)?;
        ServiceObject::collection_deleted(&emitter, path.as_ref()).await?;
        Ok(nothing())
    }

    async fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> Result<Vec<OwnedObjectPath>, SecretError> {
        let wanted = to_attributes(attributes);
        self.shared.with(|keeper| {
            let name = self.name(&keeper.keyring)?;
            let ids = keeper.keyring.search_in(&name, &wanted)?;
            Ok(ids
                .into_iter()
                .map(|id| {
                    item_path(&ItemRef {
                        collection: name.clone(),
                        id,
                    })
                })
                .collect())
        })
    }

    async fn create_item(
        &self,
        properties: HashMap<String, OwnedValue>,
        secret: Secret,
        replace: bool,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), SecretError> {
        let label: String = property(&properties, ITEM_LABEL).unwrap_or_default();
        let attributes: HashMap<String, String> =
            property(&properties, ITEM_ATTRIBUTES).unwrap_or_default();
        let stored = self.shared.with(|keeper| {
            let name = self.name(&keeper.keyring)?;
            let (secret, content_type) = keeper.open(&secret)?;
            let new = NewItem {
                label,
                attributes: to_attributes(attributes),
                secret,
                content_type,
            };
            let stored = keeper.keyring.create_item(&name, new, replace, now())?;
            keeper.save()?;
            Ok::<_, SecretError>(stored)
        })?;
        debug!(item = %item_path(&stored.item), replaced = stored.replaced, "item stored");
        if !stored.replaced {
            add_item(connection, &self.shared, stored.item.clone()).await;
        }
        announce_item(connection, &stored.item, !stored.replaced).await;
        Ok((item_path(&stored.item), nothing()))
    }

    #[zbus(property)]
    async fn items(&self) -> fdo::Result<Vec<OwnedObjectPath>> {
        let name = self.shared.with(|keeper| self.name(&keeper.keyring));
        let name = name.map_err(|error| fdo::Error::UnknownObject(error.to_string()))?;
        self.read(|collection| {
            collection
                .items
                .keys()
                .map(|id| {
                    item_path(&ItemRef {
                        collection: name.clone(),
                        id: *id,
                    })
                })
                .collect()
        })
        .map_err(|error| fdo::Error::UnknownObject(error.to_string()))
    }

    #[zbus(property)]
    async fn label(&self) -> fdo::Result<String> {
        self.read(|collection| collection.label.clone())
            .map_err(|error| fdo::Error::UnknownObject(error.to_string()))
    }

    #[zbus(property)]
    async fn set_label(&self, label: String) -> fdo::Result<()> {
        self.shared
            .with(|keeper| {
                let name = self.name(&keeper.keyring)?;
                keeper.keyring.set_collection_label(&name, &label, now())?;
                keeper.save()
            })
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    #[zbus(property)]
    async fn locked(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn created(&self) -> fdo::Result<u64> {
        self.read(|collection| collection.created)
            .map_err(|error| fdo::Error::UnknownObject(error.to_string()))
    }

    #[zbus(property)]
    async fn modified(&self) -> fdo::Result<u64> {
        self.read(|collection| collection.modified)
            .map_err(|error| fdo::Error::UnknownObject(error.to_string()))
    }

    #[zbus(signal)]
    async fn item_created(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_deleted(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_changed(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;
}

struct ItemObject {
    shared: Shared,
    item: ItemRef,
}

impl ItemObject {
    fn read<R>(&self, f: impl FnOnce(&crate::store::Item) -> R) -> fdo::Result<R> {
        self.shared.with(|keeper| {
            keeper
                .keyring
                .item(&self.item)
                .map(f)
                .ok_or_else(|| fdo::Error::UnknownObject(item_path(&self.item).to_string()))
        })
    }

    fn change(&self, change: impl FnOnce(&mut crate::store::Item)) -> Result<(), SecretError> {
        self.shared.with(|keeper| {
            keeper.keyring.update_item(&self.item, now(), change)?;
            keeper.save()
        })
    }
}

#[interface(name = "org.freedesktop.Secret.Item")]
impl ItemObject {
    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<OwnedObjectPath, SecretError> {
        self.shared.with(|keeper| {
            keeper.keyring.delete_item(&self.item, now())?;
            keeper.save()
        })?;
        let path = item_path(&self.item);
        debug!(item = %path, "item deleted");
        let emitter = SignalEmitter::new(connection, collection_path(&self.item.collection))?;
        CollectionObject::item_deleted(&emitter, path.as_ref()).await?;
        // Not from inside its own call: the object goes once it is answered.
        let connection = connection.clone();
        tokio::spawn(async move { remove::<ItemObject>(&connection, &path).await });
        Ok(nothing())
    }

    /// One structure out, not its four fields: hence the tuple around it.
    async fn get_secret(&self, session: OwnedObjectPath) -> Result<(Secret,), SecretError> {
        self.shared
            .with(|keeper| keeper.secret_of(&self.item, &session))
            .map(|secret| (secret,))
    }

    async fn set_secret(
        &self,
        secret: Secret,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(), SecretError> {
        let (plain, content_type) = self.shared.with(|keeper| keeper.open(&secret))?;
        self.change(|item| {
            item.secret = plain;
            item.content_type = content_type;
        })?;
        announce_item(connection, &self.item, false).await;
        Ok(())
    }

    #[zbus(property)]
    async fn locked(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn attributes(&self) -> fdo::Result<HashMap<String, String>> {
        self.read(|item| from_attributes(&item.attributes))
    }

    #[zbus(property)]
    async fn set_attributes(&self, attributes: HashMap<String, String>) -> fdo::Result<()> {
        self.change(|item| item.attributes = to_attributes(attributes))
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    #[zbus(property)]
    async fn label(&self) -> fdo::Result<String> {
        self.read(|item| item.label.clone())
    }

    #[zbus(property)]
    async fn set_label(&self, label: String) -> fdo::Result<()> {
        self.change(|item| item.label = label)
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    /// The schema of the item, as gnome-keyring shows it.
    #[zbus(property)]
    async fn r#type(&self) -> fdo::Result<String> {
        self.read(|item| {
            let schema = item.attributes.get(crate::store::SCHEMA_ATTRIBUTE);
            schema.cloned().unwrap_or_default()
        })
    }

    #[zbus(property)]
    async fn created(&self) -> fdo::Result<u64> {
        self.read(|item| item.created)
    }

    #[zbus(property)]
    async fn modified(&self) -> fdo::Result<u64> {
        self.read(|item| item.modified)
    }
}

struct SessionObject {
    shared: Shared,
    id: u32,
}

#[interface(name = "org.freedesktop.Secret.Session")]
impl SessionObject {
    async fn close(&self, #[zbus(connection)] connection: &Connection) {
        self.shared.with(|keeper| keeper.sessions.remove(&self.id));
        let path = session_path(self.id);
        let connection = connection.clone();
        tokio::spawn(async move { remove::<SessionObject>(&connection, &path).await });
    }
}

/// The backend of the Secret portal: each sandboxed application gets a
/// secret of its own, the same from one session to the next.
struct PortalObject {
    shared: Shared,
}

#[interface(name = "org.freedesktop.impl.portal.Secret")]
impl PortalObject {
    async fn retrieve_secret(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        fd: zvariant::OwnedFd,
        _options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> (u32, HashMap<String, OwnedValue>) {
        const SUCCESS: u32 = 0;
        const FAILED: u32 = 2;
        if app_id.is_empty() {
            warn!("the portal asked a secret for an application without identity");
            return (FAILED, HashMap::new());
        }
        let made = self.shared.with(|keeper| {
            let fresh = || {
                let mut secret = vec![0; PORTAL_SECRET];
                random(&mut secret).map(|()| secret)
            };
            let fresh = match fresh() {
                Ok(secret) => secret,
                Err(error) => {
                    error!(%error, "cannot make a secret for an application");
                    return None;
                }
            };
            let (secret, created) = keeper.keyring.portal_secret(app_id, || fresh, now());
            if created.is_some() && keeper.save().is_err() {
                return None;
            }
            Some((secret, created))
        });
        let Some((secret, created)) = made else {
            return (FAILED, HashMap::new());
        };
        if let Some(item) = created {
            info!(app_id, "a secret was made for a sandboxed application");
            add_item(connection, &self.shared, item.clone()).await;
            announce_item(connection, &item, true).await;
        }
        let mut file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
        match file.write_all(&secret) {
            Ok(()) => (SUCCESS, HashMap::new()),
            Err(error) => {
                warn!(%error, app_id, "cannot hand its secret to an application");
                (FAILED, HashMap::new())
            }
        }
    }

    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LOGIN, NewItem};

    #[test]
    fn paths_name_items_and_collections_through_aliases_too() {
        let mut keyring = Keyring::new(None, 1);
        let new = NewItem {
            label: String::new(),
            attributes: Attributes::new(),
            secret: Vec::new(),
            content_type: String::new(),
        };
        let stored = keyring.create_item(LOGIN, new, false, 1).unwrap();
        let by_alias = path(format!("{ROOT}/aliases/default/{}", stored.item.id));

        assert_eq!(
            parse_item(&keyring, &item_path(&stored.item)),
            Some(stored.item.clone())
        );
        assert_eq!(parse_item(&keyring, &by_alias), Some(stored.item));
        assert_eq!(
            parse_collection(&keyring, &path(format!("{ROOT}/aliases/default"))),
            Some(LOGIN.to_owned())
        );
        assert_eq!(
            parse_collection(&keyring, &collection_path("nowhere")),
            None
        );
        assert_eq!(session_id(&session_path(7)), Some(7));
        assert_eq!(alias_path("not valid"), None);
    }
}
