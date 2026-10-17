//! The widths the user gave to the columns of each screen, kept between
//! sessions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::layout::{LayoutKind, Split};
use crate::output::OutputRole;

/// For each screen, each layout and each number of columns shown, the
/// proportions the user chose. What is missing has the default ones.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Proportions {
    screens: BTreeMap<OutputRole, ScreenProportions>,
}

/// The proportions of one screen: by layout, then by number of columns.
pub type ScreenProportions = BTreeMap<LayoutKind, BTreeMap<usize, Split>>;

impl Proportions {
    /// What was chosen for `role`, every split repaired if it has to be.
    pub fn of(&self, role: OutputRole) -> ScreenProportions {
        let Some(screen) = self.screens.get(&role) else {
            return ScreenProportions::new();
        };
        screen
            .iter()
            .map(|(kind, splits)| {
                let splits = splits
                    .iter()
                    .filter(|(count, _)| (2..=kind.max_columns()).contains(*count))
                    .map(|(count, split)| {
                        let widths = split.widths().to_vec();
                        (*count, Split::normalized(widths, *kind, *count))
                    })
                    .collect();
                (*kind, splits)
            })
            .collect()
    }

    pub fn set(&mut self, role: OutputRole, screen: ScreenProportions) {
        if screen.is_empty() {
            self.screens.remove(&role);
        } else {
            self.screens.insert(role, screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportions_survive_being_written_and_read() {
        let mut proportions = Proportions::default();
        let mut left = ScreenProportions::new();
        let three = Split::normalized(vec![250, 500, 250], LayoutKind::ThreeColumns, 3);
        left.entry(LayoutKind::ThreeColumns)
            .or_default()
            .insert(3, three.clone());
        proportions.set(OutputRole::Left, left);

        let json = serde_json::to_string(&proportions).unwrap();
        assert_eq!(json, r#"{"left":{"three-columns":{"3":[250,500,250]}}}"#);
        let read: Proportions = serde_json::from_str(&json).unwrap();

        assert_eq!(read, proportions);
        assert_eq!(
            read.of(OutputRole::Left)[&LayoutKind::ThreeColumns][&3],
            three
        );
        assert!(read.of(OutputRole::Right).is_empty());
    }

    #[test]
    fn broken_proportions_are_read_back_as_the_default_ones() {
        let json = r#"{"right":{"two-columns":{"2":[900,100],"5":[1000]}}}"#;
        let read: Proportions = serde_json::from_str(json).unwrap();
        let right = read.of(OutputRole::Right);

        assert_eq!(
            right[&LayoutKind::TwoColumns],
            BTreeMap::from([(2, Split::default_for(LayoutKind::TwoColumns, 2))])
        );
    }
}
