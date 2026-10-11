//! Outputs as the model sees them: a role, a size, and a reserved zone.

use serde::{Deserialize, Serialize};

use crate::geometry::{Insets, Rect, Size};
use crate::ids::OutputId;

/// Where an output sits in the three-screen setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputRole {
    Left,
    Center,
    Right,
}

impl OutputRole {
    /// Side screens hold the companions; the center holds what the user does.
    pub const fn is_side(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    /// User-visible name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Écran gauche",
            Self::Center => "Écran central",
            Self::Right => "Écran droit",
        }
    }
}

/// Static description of an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputInfo {
    pub id: OutputId,
    pub role: OutputRole,
    /// Logical size.
    pub size: Size,
    /// Zone kept free for the shell (the side bar), never given to slots.
    pub reserved: Insets,
}

impl OutputInfo {
    pub const fn new(id: OutputId, role: OutputRole, size: Size) -> Self {
        Self {
            id,
            role,
            size,
            reserved: Insets::ZERO,
        }
    }

    /// The whole output.
    pub const fn bounds(&self) -> Rect {
        Rect::from_size(self.size)
    }

    /// The output minus the reserved zone.
    pub fn usable(&self) -> Rect {
        self.bounds().inset(self.reserved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_area_excludes_the_reserved_zone() {
        let mut info = OutputInfo::new(OutputId(1), OutputRole::Left, Size::new(1920, 1080));
        info.reserved = Insets {
            left: 52,
            ..Insets::ZERO
        };

        assert_eq!(info.bounds(), Rect::new(0, 0, 1920, 1080));
        assert_eq!(info.usable(), Rect::new(52, 0, 1868, 1080));
    }

    #[test]
    fn only_left_and_right_are_side_screens() {
        assert!(OutputRole::Left.is_side());
        assert!(OutputRole::Right.is_side());
        assert!(!OutputRole::Center.is_side());
    }
}
