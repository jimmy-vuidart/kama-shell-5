//! The icon theme of the shell: Reversal (`reversal-icon-theme-git` on the
//! AUR), light or dark as the shell is, in the color nearest its accent among
//! the variants installed.

use std::path::PathBuf;

use coquille_theme::{Color, ThemeKind};

use crate::catalog::data_directories;
use crate::icons::icon_theme;

/// The package the icons come from. It installs the plain variant only,
/// `Reversal` and `Reversal-dark`; the colored ones are taken when present.
pub const PACKAGE: &str = "reversal-icon-theme-git";

/// The colors of Reversal: the suffix of the variant, and the color of the
/// front of its folders (`places/scalable/folder.svg`).
const COLORS: [(&str, u32); 12] = [
    ("", 0xff_c8_41),
    ("-black", 0x46_46_46),
    ("-blue", 0x19_8c_ff),
    ("-brown", 0x98_6a_44),
    ("-cyan", 0x00_ff_ff),
    ("-green", 0x14_b4_78),
    ("-grey", 0xb9_b9_b9),
    ("-lightblue", 0x47_8c_d1),
    ("-orange", 0xff_82_0a),
    ("-pink", 0xff_6e_a0),
    ("-purple", 0x82_73_ff),
    ("-red", 0xf0_50_50),
];

/// How much a difference of lightness counts against one of hue and chroma:
/// the accents of the light and the dark theme differ mostly by lightness,
/// and should find the same color.
const LIGHTNESS_WEIGHT: f32 = 0.1;

/// The icon theme for a theme and an accent: the Reversal variant installed
/// that matches best, else the icon theme of the user. This reads a few
/// directories: to do when the theme or the accent changes, not per frame.
pub fn icon_theme_for(kind: ThemeKind, accent: Color) -> String {
    choose(&installed(), kind, accent).unwrap_or_else(icon_theme)
}

/// The variant to use among those `installed`: the nearest color in the
/// brightness of the theme, else in the other one.
fn choose(installed: &[String], kind: ThemeKind, accent: Color) -> Option<String> {
    let colors = by_nearness(accent);
    let dark = kind == ThemeKind::Dark;
    let names = |dark: bool| colors.iter().map(move |color| name(color, dark));
    names(dark)
        .chain(names(!dark))
        .find(|name| installed.contains(name))
}

fn name(color: &str, dark: bool) -> String {
    let brightness = if dark { "-dark" } else { "" };
    format!("Reversal{color}{brightness}")
}

/// The suffixes of the colors of Reversal, the nearest to `accent` first.
fn by_nearness(accent: Color) -> Vec<&'static str> {
    let target = oklab(accent);
    let distance = |hex: u32| {
        let [l, a, b] = oklab(Color::rgb(hex));
        let dl = (l - target[0]) * LIGHTNESS_WEIGHT;
        dl * dl + (a - target[1]).powi(2) + (b - target[2]).powi(2)
    };
    let mut colors = COLORS.to_vec();
    colors.sort_by(|(_, one), (_, other)| distance(*one).total_cmp(&distance(*other)));
    colors.into_iter().map(|(suffix, _)| suffix).collect()
}

/// A color in Oklab, where distances follow what the eye sees.
fn oklab(color: Color) -> [f32; 3] {
    let linear = |channel: u8| {
        let value = f32::from(channel) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(color.r), linear(color.g), linear(color.b));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// The Reversal variants installed, in every directory icon themes are
/// looked for in.
fn installed() -> Vec<String> {
    let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".icons"));
    let directories = data_directories()
        .into_iter()
        .map(|data| data.join("icons"));
    let mut names = Vec::new();
    for directory in home.into_iter().chain(directories) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with("Reversal") && entry.path().join("index.theme").is_file() {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nearest(hex: u32) -> &'static str {
        by_nearness(Color::rgb(hex))[0]
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn each_accent_offered_finds_its_own_color() {
        // The accents of the settings screen, and those of the two themes.
        let expected = [
            (0x5b_63_e6, "-purple"),
            (0x8f_97_ff, "-purple"),
            (0x2f_6f_ed, "-blue"),
            (0x7c_5c_f5, "-purple"),
            (0xc6_40_9f, "-pink"),
            (0xe5_48_4d, "-red"),
            (0xf0_8a_24, "-orange"),
            (0xc9_9a_06, ""),
            (0x2f_9e_63, "-green"),
            (0x0f_9a_a8, "-cyan"),
        ];
        for (accent, color) in expected {
            assert_eq!(nearest(accent), color, "accent #{accent:06x}");
        }
    }

    #[test]
    fn a_grey_accent_finds_grey_or_black_by_its_lightness() {
        assert_eq!(nearest(0x9a_9a_9a), "-grey");
        assert_eq!(nearest(0x2a_2a_2a), "-black");
    }

    #[test]
    fn the_package_alone_follows_light_and_dark() {
        let installed = names(&["Reversal", "Reversal-dark"]);
        let accent = Color::rgb(0x5b_63_e6);

        let light = choose(&installed, ThemeKind::Light, accent);
        let dark = choose(&installed, ThemeKind::Dark, accent);

        assert_eq!(light.as_deref(), Some("Reversal"));
        assert_eq!(dark.as_deref(), Some("Reversal-dark"));
    }

    #[test]
    fn an_installed_color_near_the_accent_is_preferred() {
        let installed = names(&["Reversal", "Reversal-dark", "Reversal-purple-dark"]);

        let chosen = choose(&installed, ThemeKind::Dark, Color::rgb(0x8f_97_ff));

        assert_eq!(chosen.as_deref(), Some("Reversal-purple-dark"));
    }

    #[test]
    fn the_other_brightness_does_when_the_right_one_is_missing() {
        let installed = names(&["Reversal-orange"]);

        let chosen = choose(&installed, ThemeKind::Dark, Color::rgb(0xf0_8a_24));

        assert_eq!(chosen.as_deref(), Some("Reversal-orange"));
    }

    #[test]
    fn without_reversal_nothing_is_chosen() {
        let installed = names(&["Adwaita", "WhiteSur-dark"]);

        assert_eq!(
            choose(&installed, ThemeKind::Light, Color::rgb(0x5b_63_e6)),
            None
        );
    }
}
