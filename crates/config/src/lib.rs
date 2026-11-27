//! `~/.config/coquille/config.toml`, as the programs of the shell read it and
//! rewrite it: one setting at a time, the rest of the file left as it was,
//! comments included.
//!
//! The compositor reads the same file on its own, strictly (an unknown
//! setting is an error there): nothing written from here may surprise it.

mod edit;
mod settings;

pub use edit::{default_path, update, with_value, without};
pub use settings::{
    Change, DockPlace, MouseAccelProfile, PerScreen, Settings, ShellMode, TouchpadClick,
    WallpaperDefinition, WallpaperMode, apply, assign, docks_for, docks_for_mode,
    effective_dock_place,
};
pub use toml_edit::{Array, Value};
