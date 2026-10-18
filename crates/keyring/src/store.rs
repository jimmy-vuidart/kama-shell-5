//! What the keyring holds: collections of items, each a secret with a label
//! and attributes, found again by those attributes. Pure: no D-Bus, no disk.
//!
//! The `login` collection is kept on disk and named by the `default` alias,
//! where applications store what they want back in the next session. The
//! `session` collection lives in memory only, for what must not outlive it.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

pub type Attributes = BTreeMap<String, String>;

/// The collection kept on disk, created at the first start.
pub const LOGIN: &str = "login";
/// The collection forgotten when the keyring stops.
pub const SESSION: &str = "session";
/// The alias under which applications look for where to store.
pub const DEFAULT_ALIAS: &str = "default";

/// The attribute under which libsecret writes the schema of an item.
pub const SCHEMA_ATTRIBUTE: &str = "xdg:schema";
/// The schema of the secrets given to sandboxed applications by the portal.
pub const PORTAL_SCHEMA: &str = "org.freedesktop.impl.portal.Secret";

/// One secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub label: String,
    pub attributes: Attributes,
    pub secret: Vec<u8>,
    pub content_type: String,
    pub created: u64,
    pub modified: u64,
}

/// Never shows the secret, so that it cannot reach a log by accident.
impl fmt::Debug for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Item")
            .field("label", &self.label)
            .field("attributes", &self.attributes)
            .field("secret", &format_args!("<{} bytes>", self.secret.len()))
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub label: String,
    pub created: u64,
    pub modified: u64,
    pub items: BTreeMap<u32, Item>,
    next_item: u32,
}

impl Collection {
    fn new(label: &str, now: u64) -> Self {
        Self {
            label: label.to_owned(),
            created: now,
            modified: now,
            items: BTreeMap::new(),
            next_item: 1,
        }
    }

    /// The items whose attributes include all of `wanted`.
    fn search<'a>(&'a self, wanted: &'a Attributes) -> impl Iterator<Item = u32> + 'a {
        self.items
            .iter()
            .filter(|(_, item)| matches(&item.attributes, wanted))
            .map(|(id, _)| *id)
    }
}

fn matches(attributes: &Attributes, wanted: &Attributes) -> bool {
    wanted
        .iter()
        .all(|(key, value)| attributes.get(key) == Some(value))
}

/// What is written to disk: everything but the session collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub collections: BTreeMap<String, Collection>,
    pub aliases: BTreeMap<String, String>,
}

/// Where an item lives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemRef {
    pub collection: String,
    pub id: u32,
}

/// The new item, and whether it took the place of one with the same
/// attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub item: ItemRef,
    pub replaced: bool,
}

/// What a new item is made of.
#[derive(Clone)]
pub struct NewItem {
    pub label: String,
    pub attributes: Attributes,
    pub secret: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("no such collection")]
    NoCollection,
    #[error("no such item")]
    NoItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyring {
    collections: BTreeMap<String, Collection>,
    aliases: BTreeMap<String, String>,
}

impl Keyring {
    /// The keyring read from disk, or a new one: the login collection and
    /// the session one always exist when it starts.
    pub fn new(saved: Option<Saved>, now: u64) -> Self {
        let Saved {
            mut collections,
            mut aliases,
        } = saved.unwrap_or_default();
        collections.remove(SESSION);
        collections
            .entry(LOGIN.to_owned())
            .or_insert_with(|| Collection::new("Login", now));
        collections.insert(SESSION.to_owned(), Collection::new("Session", now));
        aliases
            .entry(DEFAULT_ALIAS.to_owned())
            .or_insert_with(|| LOGIN.to_owned());
        aliases.insert(SESSION.to_owned(), SESSION.to_owned());
        // An alias that names nothing any more is dropped.
        aliases.retain(|_, name| collections.contains_key(name));
        Self {
            collections,
            aliases,
        }
    }

    /// What must survive the session.
    pub fn saved(&self) -> Saved {
        let mut collections = self.collections.clone();
        collections.remove(SESSION);
        let mut aliases = self.aliases.clone();
        aliases.retain(|_, name| name != SESSION);
        Saved {
            collections,
            aliases,
        }
    }

    pub fn collections(&self) -> impl Iterator<Item = (&str, &Collection)> {
        self.collections
            .iter()
            .map(|(name, collection)| (name.as_str(), collection))
    }

    pub fn collection(&self, name: &str) -> Option<&Collection> {
        self.collections.get(name)
    }

    pub fn aliases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases
            .iter()
            .map(|(alias, name)| (alias.as_str(), name.as_str()))
    }

    /// The collection an alias names.
    pub fn alias(&self, alias: &str) -> Option<&str> {
        self.aliases.get(alias).map(String::as_str)
    }

    /// Points `alias` at a collection, or removes it with `None`.
    pub fn set_alias(&mut self, alias: &str, name: Option<&str>) -> Result<(), StoreError> {
        match name {
            Some(name) if !self.collections.contains_key(name) => Err(StoreError::NoCollection),
            Some(name) => {
                self.aliases.insert(alias.to_owned(), name.to_owned());
                Ok(())
            }
            None => {
                self.aliases.remove(alias);
                Ok(())
            }
        }
    }

    /// A new collection, or the one `alias` already names. Says whether it
    /// was created.
    pub fn create_collection(&mut self, label: &str, alias: &str, now: u64) -> (String, bool) {
        if let Some(name) = self.alias(alias).filter(|_| !alias.is_empty()) {
            return (name.to_owned(), false);
        }
        let base = path_element(label);
        let mut name = base.clone();
        let mut suffix = 1;
        while self.collections.contains_key(&name) {
            suffix += 1;
            name = format!("{base}_{suffix}");
        }
        self.collections
            .insert(name.clone(), Collection::new(label, now));
        if !alias.is_empty() {
            self.aliases.insert(alias.to_owned(), name.clone());
        }
        (name, true)
    }

    /// Removes a collection, its items and the aliases that named it.
    pub fn delete_collection(&mut self, name: &str) -> Result<Collection, StoreError> {
        let collection = self
            .collections
            .remove(name)
            .ok_or(StoreError::NoCollection)?;
        self.aliases.retain(|_, target| target != name);
        Ok(collection)
    }

    pub fn set_collection_label(
        &mut self,
        name: &str,
        label: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let collection = self
            .collections
            .get_mut(name)
            .ok_or(StoreError::NoCollection)?;
        collection.label = label.to_owned();
        collection.modified = now;
        Ok(())
    }

    /// Every item, in every collection, whose attributes include `wanted`.
    pub fn search(&self, wanted: &Attributes) -> Vec<ItemRef> {
        self.collections
            .iter()
            .flat_map(|(name, collection)| {
                collection.search(wanted).map(|id| ItemRef {
                    collection: name.clone(),
                    id,
                })
            })
            .collect()
    }

    /// The items of one collection whose attributes include `wanted`.
    pub fn search_in(&self, name: &str, wanted: &Attributes) -> Result<Vec<u32>, StoreError> {
        let collection = self.collections.get(name).ok_or(StoreError::NoCollection)?;
        Ok(collection.search(wanted).collect())
    }

    pub fn item(&self, item: &ItemRef) -> Option<&Item> {
        self.collections.get(&item.collection)?.items.get(&item.id)
    }

    /// Stores a secret. With `replace`, an item with exactly the same
    /// attributes takes the new one, keeping its place.
    pub fn create_item(
        &mut self,
        name: &str,
        new: NewItem,
        replace: bool,
        now: u64,
    ) -> Result<Stored, StoreError> {
        let collection = self
            .collections
            .get_mut(name)
            .ok_or(StoreError::NoCollection)?;
        collection.modified = now;
        let same = replace
            .then(|| {
                collection
                    .items
                    .iter()
                    .find(|(_, item)| item.attributes == new.attributes)
                    .map(|(id, _)| *id)
            })
            .flatten();
        let item = |created| Item {
            label: new.label,
            attributes: new.attributes,
            secret: new.secret,
            content_type: new.content_type,
            created,
            modified: now,
        };
        let (id, replaced) = match same {
            Some(id) => {
                let created = collection.items[&id].created;
                collection.items.insert(id, item(created));
                (id, true)
            }
            None => {
                let id = collection.next_item;
                collection.next_item += 1;
                collection.items.insert(id, item(now));
                (id, false)
            }
        };
        Ok(Stored {
            item: ItemRef {
                collection: name.to_owned(),
                id,
            },
            replaced,
        })
    }

    pub fn delete_item(&mut self, item: &ItemRef, now: u64) -> Result<Item, StoreError> {
        let collection = self
            .collections
            .get_mut(&item.collection)
            .ok_or(StoreError::NoItem)?;
        let removed = collection
            .items
            .remove(&item.id)
            .ok_or(StoreError::NoItem)?;
        collection.modified = now;
        Ok(removed)
    }

    /// Changes an item, and when it was last modified.
    pub fn update_item(
        &mut self,
        item: &ItemRef,
        now: u64,
        change: impl FnOnce(&mut Item),
    ) -> Result<(), StoreError> {
        let found = self
            .collections
            .get_mut(&item.collection)
            .and_then(|collection| collection.items.get_mut(&item.id))
            .ok_or(StoreError::NoItem)?;
        change(found);
        found.modified = now;
        Ok(())
    }

    /// The secret the portal gives a sandboxed application: the same from
    /// one session to the next, made with `fresh` the first time it is
    /// asked for. Says whether it was made now.
    pub fn portal_secret(
        &mut self,
        app_id: &str,
        fresh: impl FnOnce() -> Vec<u8>,
        now: u64,
    ) -> (Vec<u8>, Option<ItemRef>) {
        let attributes = Attributes::from([
            (SCHEMA_ATTRIBUTE.to_owned(), PORTAL_SCHEMA.to_owned()),
            ("app_id".to_owned(), app_id.to_owned()),
        ]);
        let home = self.alias(DEFAULT_ALIAS).unwrap_or(LOGIN).to_owned();
        let found = self.search_in(&home, &attributes).ok().and_then(|ids| {
            let id = *ids.first()?;
            self.item(&ItemRef {
                collection: home.clone(),
                id,
            })
        });
        if let Some(item) = found {
            return (item.secret.clone(), None);
        }
        let secret = fresh();
        let new = NewItem {
            label: format!("Application key for {app_id}"),
            attributes,
            secret: secret.clone(),
            content_type: "application/octet-stream".to_owned(),
        };
        // The default collection always exists: an alias that would name a
        // deleted one is dropped with it, and the login one comes back.
        let home = if self.collections.contains_key(&home) {
            home
        } else {
            self.collections
                .entry(LOGIN.to_owned())
                .or_insert_with(|| Collection::new("Login", now));
            LOGIN.to_owned()
        };
        let stored = self.create_item(&home, new, false, now).ok();
        (secret, stored.map(|stored| stored.item))
    }
}

/// A D-Bus object path element made of a label: letters, digits and `_`.
fn path_element(label: &str) -> String {
    let element: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if element.is_empty() {
        "collection".to_owned()
    } else {
        element
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attributes(pairs: &[(&str, &str)]) -> Attributes {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn new_item(label: &str, pairs: &[(&str, &str)], secret: &[u8]) -> NewItem {
        NewItem {
            label: label.to_owned(),
            attributes: attributes(pairs),
            secret: secret.to_vec(),
            content_type: "text/plain".to_owned(),
        }
    }

    #[test]
    fn a_new_keyring_has_a_login_and_a_session_collection() {
        let keyring = Keyring::new(None, 1);

        assert!(keyring.collection(LOGIN).is_some());
        assert!(keyring.collection(SESSION).is_some());
        assert_eq!(keyring.alias(DEFAULT_ALIAS), Some(LOGIN));
        assert_eq!(keyring.alias(SESSION), Some(SESSION));
    }

    #[test]
    fn a_search_finds_the_items_that_have_all_the_attributes_asked_for() {
        let mut keyring = Keyring::new(None, 1);
        let chrome = new_item("Chrome", &[("application", "chrome"), ("v", "2")], b"a");
        let discord = new_item("Discord", &[("application", "discord")], b"b");
        keyring.create_item(LOGIN, chrome, false, 2).unwrap();
        keyring.create_item(LOGIN, discord, false, 2).unwrap();

        let found = keyring.search(&attributes(&[("application", "chrome")]));

        assert_eq!(found.len(), 1);
        assert_eq!(keyring.item(&found[0]).unwrap().secret, b"a");
        assert_eq!(keyring.search(&Attributes::new()).len(), 2);
        assert!(keyring.search(&attributes(&[("other", "x")])).is_empty());
    }

    #[test]
    fn storing_again_with_replace_keeps_one_item_with_the_new_secret() {
        let mut keyring = Keyring::new(None, 1);
        let first = keyring
            .create_item(LOGIN, new_item("a", &[("k", "v")], b"old"), true, 2)
            .unwrap();

        let second = keyring
            .create_item(LOGIN, new_item("b", &[("k", "v")], b"new"), true, 3)
            .unwrap();

        assert!(second.replaced);
        assert_eq!(first.item, second.item);
        let item = keyring.item(&second.item).unwrap();
        assert_eq!((item.secret.as_slice(), item.created), (&b"new"[..], 2));
        assert_eq!(keyring.search(&Attributes::new()).len(), 1);
    }

    #[test]
    fn storing_again_without_replace_keeps_both() {
        let mut keyring = Keyring::new(None, 1);
        let new = || new_item("a", &[("k", "v")], b"s");
        keyring.create_item(LOGIN, new(), false, 2).unwrap();
        let second = keyring.create_item(LOGIN, new(), false, 2).unwrap();

        assert!(!second.replaced);
        assert_eq!(keyring.search(&attributes(&[("k", "v")])).len(), 2);
    }

    #[test]
    fn a_deleted_item_is_gone_and_its_number_is_not_given_again() {
        let mut keyring = Keyring::new(None, 1);
        let new = || new_item("a", &[], b"s");
        let first = keyring.create_item(LOGIN, new(), false, 2).unwrap().item;

        keyring.delete_item(&first, 3).unwrap();
        let second = keyring.create_item(LOGIN, new(), false, 4).unwrap().item;

        assert!(keyring.item(&first).is_none());
        assert_ne!(first.id, second.id);
        assert_eq!(keyring.delete_item(&first, 5), Err(StoreError::NoItem));
    }

    #[test]
    fn the_session_collection_is_not_saved() {
        let mut keyring = Keyring::new(None, 1);
        keyring
            .create_item(SESSION, new_item("a", &[], b"s"), false, 2)
            .unwrap();
        keyring
            .create_item(LOGIN, new_item("b", &[], b"t"), false, 2)
            .unwrap();

        let saved = keyring.saved();
        let again = Keyring::new(Some(saved.clone()), 3);

        assert!(!saved.collections.contains_key(SESSION));
        assert!(!saved.aliases.contains_key(SESSION));
        assert_eq!(again.search(&Attributes::new()).len(), 1);
        assert!(again.collection(SESSION).unwrap().items.is_empty());
    }

    #[test]
    fn creating_a_collection_under_a_taken_alias_returns_the_existing_one() {
        let mut keyring = Keyring::new(None, 1);

        assert_eq!(
            keyring.create_collection("Other", DEFAULT_ALIAS, 2),
            (LOGIN.to_owned(), false)
        );
        let (name, created) = keyring.create_collection("Mes clés", "", 2);
        assert!(created);
        assert_eq!(name, "Mes_cl_s");
        let (twin, _) = keyring.create_collection("Mes clés", "", 2);
        assert_eq!(twin, "Mes_cl_s_2");
    }

    #[test]
    fn deleting_a_collection_drops_the_aliases_that_named_it() {
        let mut keyring = Keyring::new(None, 1);
        let (name, _) = keyring.create_collection("Work", "work", 2);

        keyring.delete_collection(&name).unwrap();

        assert_eq!(keyring.alias("work"), None);
        assert!(keyring.collection(&name).is_none());
    }

    #[test]
    fn a_deleted_login_collection_comes_back_at_the_next_start() {
        let mut keyring = Keyring::new(None, 1);
        keyring.delete_collection(LOGIN).unwrap();

        let again = Keyring::new(Some(keyring.saved()), 2);

        assert!(again.collection(LOGIN).is_some());
        assert_eq!(again.alias(DEFAULT_ALIAS), Some(LOGIN));
    }

    #[test]
    fn an_alias_cannot_name_a_missing_collection() {
        let mut keyring = Keyring::new(None, 1);

        assert_eq!(
            keyring.set_alias("x", Some("nowhere")),
            Err(StoreError::NoCollection)
        );
        keyring.set_alias(DEFAULT_ALIAS, None).unwrap();
        assert_eq!(keyring.alias(DEFAULT_ALIAS), None);
    }

    #[test]
    fn the_portal_gives_an_application_the_same_secret_each_time() {
        let mut keyring = Keyring::new(None, 1);

        let (first, made) = keyring.portal_secret("com.opera.Opera", || vec![7; 64], 2);
        let (again, made_again) = keyring.portal_secret("com.opera.Opera", || vec![9; 64], 3);
        let (other, _) = keyring.portal_secret("dev.goats.xivlauncher", || vec![8; 64], 3);

        assert!(made.is_some());
        assert!(made_again.is_none());
        assert_eq!(first, again);
        assert_ne!(first, other);
    }

    #[test]
    fn the_debug_form_of_an_item_hides_its_secret() {
        let item = Item {
            label: "l".to_owned(),
            attributes: Attributes::new(),
            secret: b"hunter2".to_vec(),
            content_type: String::new(),
            created: 0,
            modified: 0,
        };

        let shown = format!("{item:?}");

        assert!(!shown.contains("hunter2"));
        assert!(shown.contains("7 bytes"));
    }
}
