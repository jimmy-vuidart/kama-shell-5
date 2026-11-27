//! Pure layout and filtering rules for the spatial launcher overlay.

use coquille_apps::App;
use coquille_ipc::Window;
use coquille_model::OutputRole;

use super::drawer::fold;

pub const SIDEBAR_WIDTH: u32 = 56;
pub const MAX_CONTENT_WIDTH: f32 = 1440.0;
pub const MAX_APPS_WIDTH: f32 = 1140.0;
pub const SEARCH_MAX_WIDTH: f32 = 460.0;
/// Fixed height of one application tile in the spatial launcher grid.
pub const APP_TILE_HEIGHT: f32 = 104.0;
/// Vertical gap between application grid rows.
pub const APP_ROW_GAP: f32 = 10.0;

/// Responsive column/row counts and horizontal padding for the spatial launcher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialLayout {
    pub two_columns: bool,
    pub game_columns: usize,
    pub calendar_columns: usize,
    pub app_columns: usize,
    pub app_rows: usize,
    pub padding_x: f32,
}

impl SpatialLayout {
    pub fn for_width(usable_width: f32) -> Self {
        Self::for_size(usable_width, 1080.0)
    }

    pub fn for_size(usable_width: f32, usable_height: f32) -> Self {
        let two_columns = usable_width >= 1080.0;
        let overhead = if two_columns { 668.0 } else { 980.0 };
        let avail = (usable_height - overhead).max(0.0);
        let rows = if avail >= APP_TILE_HEIGHT {
            1 + ((avail - APP_TILE_HEIGHT) / (APP_TILE_HEIGHT + APP_ROW_GAP)).floor() as usize
        } else {
            1
        };
        Self {
            two_columns,
            game_columns: if usable_width >= 720.0 { 3 } else { 1 },
            calendar_columns: if usable_width >= 720.0 { 2 } else { 1 },
            app_columns: if usable_width >= 1120.0 {
                8
            } else if usable_width >= 740.0 {
                6
            } else {
                4
            },
            app_rows: rows.clamp(1, 5),
            padding_x: if usable_width >= 1200.0 { 40.0 } else { 20.0 },
        }
    }

    /// Number of application tiles displayed on a single page.
    pub fn page_size(&self) -> usize {
        self.app_columns.max(1) * self.app_rows.max(1)
    }
}

/// Returns `true` iff `screen` has at least one non-minimized window.
pub fn screen_has_visible_window(windows: &[Window], screen: OutputRole) -> bool {
    windows
        .iter()
        .any(|window| window.screen == screen && !window.minimized)
}

/// Filters applications using accent-insensitive, case-insensitive matching
/// against `name`, `generic_name`, `keywords`, and `id`.
pub fn filter_apps<'a>(apps: &'a [App], query: &str) -> Vec<&'a App> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return apps.iter().collect();
    }
    let needle = fold(trimmed);
    apps.iter()
        .filter(|app| {
            fold(&app.name).contains(&needle)
                || app
                    .generic_name
                    .as_deref()
                    .is_some_and(|generic| fold(generic).contains(&needle))
                || app
                    .keywords
                    .iter()
                    .any(|keyword| fold(keyword).contains(&needle))
                || fold(&app.id).contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, generic: Option<&str>, keywords: &[&str]) -> App {
        App {
            id: id.into(),
            name: name.into(),
            generic_name: generic.map(str::to_owned),
            keywords: keywords.iter().map(|s| (*s).to_owned()).collect(),
            ..App::default()
        }
    }

    fn window(id: u64, screen: OutputRole, minimized: bool) -> Window {
        Window {
            id,
            app: "app".into(),
            title: String::new(),
            screen,
            slot: None,
            active: false,
            minimized,
        }
    }

    #[test]
    fn spatial_layout_adapts_to_screen_widths() {
        let ultrawide = SpatialLayout::for_width(3440.0);
        assert!(ultrawide.two_columns);
        assert_eq!(ultrawide.game_columns, 3);
        assert_eq!(ultrawide.calendar_columns, 2);
        assert_eq!(ultrawide.app_columns, 8);
        assert_eq!(ultrawide.padding_x, 40.0);

        let full_hd = SpatialLayout::for_width(1920.0);
        assert_eq!(full_hd, ultrawide);

        let laptop = SpatialLayout::for_width(1280.0);
        assert!(laptop.two_columns);
        assert_eq!(laptop.game_columns, 3);
        assert_eq!(laptop.calendar_columns, 2);
        assert_eq!(laptop.app_columns, 8);
        assert_eq!(laptop.padding_x, 40.0);

        let medium = SpatialLayout::for_width(960.0);
        assert!(!medium.two_columns);
        assert_eq!(medium.game_columns, 3);
        assert_eq!(medium.calendar_columns, 2);
        assert_eq!(medium.app_columns, 6);
        assert_eq!(medium.padding_x, 20.0);

        let narrow = SpatialLayout::for_width(640.0);
        assert!(!narrow.two_columns);
        assert_eq!(narrow.game_columns, 1);
        assert_eq!(narrow.calendar_columns, 1);
        assert_eq!(narrow.app_columns, 4);
        assert_eq!(narrow.padding_x, 20.0);

        assert_eq!(SpatialLayout::for_size(1600.0, 900.0).app_rows, 2);
        assert_eq!(SpatialLayout::for_size(1920.0, 1080.0).app_rows, 3);
        assert_eq!(SpatialLayout::for_size(3440.0, 1440.0).app_rows, 5);
        assert_eq!(SpatialLayout::for_size(1280.0, 720.0).app_rows, 1);
    }

    #[test]
    fn screen_has_visible_window_ignores_minimized() {
        let windows = [
            window(1, OutputRole::Left, true),
            window(2, OutputRole::Center, false),
        ];
        assert!(!screen_has_visible_window(&windows, OutputRole::Left));
        assert!(screen_has_visible_window(&windows, OutputRole::Center));
        assert!(!screen_has_visible_window(&windows, OutputRole::Right));
    }

    #[test]
    fn filter_apps_matches_accents_case_keywords_and_id() {
        let apps = [
            app(
                "org.gnome.Nautilus",
                "Fichiers",
                Some("Gestionnaire de fichiers"),
                &["dossier"],
            ),
            app(
                "code-oss",
                "Éditeur de code",
                Some("Développement"),
                &["ide", "rust"],
            ),
            app("firefox", "Firefox", Some("Navigateur web"), &["internet"]),
        ];

        assert_eq!(filter_apps(&apps, "").len(), 3);
        assert_eq!(filter_apps(&apps, "editeur")[0].id, "code-oss");
        assert_eq!(filter_apps(&apps, "RUST")[0].id, "code-oss");
        assert_eq!(filter_apps(&apps, "nautilus")[0].id, "org.gnome.Nautilus");
        assert_eq!(filter_apps(&apps, "navigateur")[0].id, "firefox");
        assert!(filter_apps(&apps, "inconnu").is_empty());
    }
}
