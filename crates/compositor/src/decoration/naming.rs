//! What a window is called and what stands for its application: the name
//! and the icon of its desktop entry when it has one, else a name made from
//! its `app_id` and a color.

use coquille_apps::Catalog;

/// The name of the application of a window, and the name of its icon when
/// the application is installed with a desktop entry.
pub fn identity(catalog: &Catalog, app_id: &str) -> (String, Option<String>) {
    match catalog.find(app_id) {
        Some(app) => (app.name.clone(), app.icon.clone()),
        None => (display_name(app_id), None),
    }
}

/// A readable name: the last dotted segment of the `app_id`, capitalized.
pub fn display_name(app_id: &str) -> String {
    let segment = app_id.rsplit('.').next().unwrap_or_default();
    let mut letters = segment.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => "Application".to_owned(),
    }
}

/// The badge color of an application: always the same for a given `app_id`.
pub fn badge_color(app_id: &str) -> [u8; 3] {
    coquille_theme::badge_rgb(app_id)
}

#[cfg(test)]
mod tests {
    use coquille_apps::App;
    use coquille_theme::BADGE_PALETTE;

    use super::*;

    fn catalog() -> Catalog {
        Catalog::new(vec![App {
            id: "org.gnome.Nautilus".into(),
            name: "Fichiers".into(),
            command: "nautilus --new-window".into(),
            icon: Some("org.gnome.Nautilus".into()),
            wm_class: None,
            ..App::default()
        }])
    }

    #[test]
    fn a_window_bears_the_name_and_the_icon_of_its_application() {
        let (name, icon) = identity(&catalog(), "org.gnome.Nautilus");

        assert_eq!(name, "Fichiers");
        assert_eq!(icon.as_deref(), Some("org.gnome.Nautilus"));
    }

    #[test]
    fn an_application_is_found_whatever_way_its_window_names_it() {
        assert_eq!(identity(&catalog(), "nautilus").0, "Fichiers");
        assert_eq!(identity(&catalog(), "Nautilus").0, "Fichiers");
    }

    #[test]
    fn an_unknown_application_keeps_a_name_made_from_its_identifier() {
        let (name, icon) = identity(&catalog(), "org.example.editor");

        assert_eq!(name, "Editor");
        assert_eq!(icon, None);
        assert_eq!(identity(&catalog(), "").0, "Application");
    }

    #[test]
    fn a_reverse_domain_app_id_gives_its_last_segment() {
        assert_eq!(display_name("org.mozilla.firefox"), "Firefox");
        assert_eq!(display_name("org.gnome.Ptyxis"), "Ptyxis");
    }

    #[test]
    fn a_plain_app_id_is_capitalized() {
        assert_eq!(display_name("kitty"), "Kitty");
        assert_eq!(display_name("Alacritty"), "Alacritty");
    }

    #[test]
    fn an_accented_first_letter_is_capitalized_too() {
        assert_eq!(display_name("éditeur"), "Éditeur");
    }

    #[test]
    fn a_missing_app_id_gives_a_generic_name() {
        assert_eq!(display_name(""), "Application");
        assert_eq!(display_name("org.example."), "Application");
    }

    #[test]
    fn the_badge_color_is_stable_and_from_the_palette() {
        let color = badge_color("org.mozilla.firefox");

        assert_eq!(color, badge_color("org.mozilla.firefox"));
        assert!(BADGE_PALETTE.contains(&color));
    }

    #[test]
    fn badge_colors_are_spread_over_the_palette() {
        let ids = ["kitty", "Alacritty", "discord", "firefox", "code", "steam"];
        let mut colors: Vec<_> = ids.iter().map(|id| badge_color(id)).collect();
        colors.sort();
        colors.dedup();

        assert!(colors.len() >= 3, "only {} distinct colors", colors.len());
    }
}
