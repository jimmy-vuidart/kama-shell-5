//! What a window is called and which color stands for its application,
//! derived from its `app_id` until real desktop entries are read.

/// Colors of the application badge, as in the mock-ups.
pub const BADGE_PALETTE: [[u8; 3]; 8] = [
    [0x7c, 0x3a, 0xed],
    [0x25, 0x63, 0xeb],
    [0x0d, 0x94, 0x88],
    [0xea, 0x58, 0x0c],
    [0xca, 0x8a, 0x04],
    [0x4f, 0x46, 0xe5],
    [0xdc, 0x26, 0x26],
    [0x47, 0x55, 0x69],
];

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
    // FNV-1a: tiny, and stable across runs unlike the standard hasher.
    let hash = app_id
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    BADGE_PALETTE[(hash % BADGE_PALETTE.len() as u64) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

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
