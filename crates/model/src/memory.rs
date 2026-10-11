//! Per-application memory: where the user tidied each application.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::AppKey;
use crate::output::OutputRole;

/// The screen and slot an application reopens in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedPlace {
    pub role: OutputRole,
    pub slot: usize,
}

/// Habits created by the user tidying windows. Windows left floating never
/// appear here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppMemory {
    places: BTreeMap<AppKey, RememberedPlace>,
}

impl AppMemory {
    pub fn get(&self, app: &AppKey) -> Option<RememberedPlace> {
        self.places.get(app).copied()
    }

    pub fn remember(&mut self, app: AppKey, place: RememberedPlace) {
        self.places.insert(app, place);
    }

    pub fn forget(&mut self, app: &AppKey) {
        self.places.remove(app);
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discord() -> AppKey {
        AppKey::new("discord")
    }

    const LEFT_FIRST: RememberedPlace = RememberedPlace {
        role: OutputRole::Left,
        slot: 0,
    };

    #[test]
    fn an_unknown_application_has_no_place() {
        assert_eq!(AppMemory::default().get(&discord()), None);
    }

    #[test]
    fn a_remembered_place_is_returned() {
        let mut memory = AppMemory::default();

        memory.remember(discord(), LEFT_FIRST);

        assert_eq!(memory.get(&discord()), Some(LEFT_FIRST));
    }

    #[test]
    fn remembering_again_replaces_the_place() {
        let mut memory = AppMemory::default();
        let right_second = RememberedPlace {
            role: OutputRole::Right,
            slot: 1,
        };

        memory.remember(discord(), LEFT_FIRST);
        memory.remember(discord(), right_second);

        assert_eq!(memory.get(&discord()), Some(right_second));
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn a_forgotten_application_has_no_place() {
        let mut memory = AppMemory::default();
        memory.remember(discord(), LEFT_FIRST);

        memory.forget(&discord());

        assert!(memory.is_empty());
    }

    #[test]
    fn memory_survives_serialization() {
        let mut memory = AppMemory::default();
        memory.remember(discord(), LEFT_FIRST);
        memory.remember(
            AppKey::new("org.mozilla.firefox"),
            RememberedPlace {
                role: OutputRole::Right,
                slot: 1,
            },
        );

        let json = serde_json::to_string(&memory).unwrap();
        let restored: AppMemory = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, memory);
        assert!(
            json.contains(r#""discord":{"role":"left","slot":0}"#),
            "{json}"
        );
    }
}
