//! The applications installed on the system, read from their `.desktop`
//! files: what the drawer of the dock lists, and how a window is given its
//! real name and icon, in the dock as in the title bars of the compositor.

mod catalog;
mod icons;
mod reversal;

pub use catalog::{App, Catalog, command_of, data_directories, languages, parse_entry};
pub use icons::find_icon;
pub use reversal::{PACKAGE as ICON_PACKAGE, icon_theme_for};
