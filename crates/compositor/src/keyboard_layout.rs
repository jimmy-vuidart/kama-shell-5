//! Which keyboard layout to use.
//!
//! Nested, the host only hands over key positions, so the compositor needs
//! its own layout. Without one, XKB falls back to QWERTY.

use std::fs;

const XORG_KEYBOARD_CONF: &str = "/etc/X11/xorg.conf.d/00-keyboard.conf";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayout {
    pub layout: String,
    pub variant: String,
    pub options: Option<String>,
}

/// Reads the layout that `localectl` writes for X11.
pub fn parse_xorg_conf(text: &str) -> Option<KeyboardLayout> {
    let mut found = KeyboardLayout::default();
    for line in text.lines().map(str::trim) {
        // Lines look like: Option "XkbLayout" "fr"
        let mut quoted = line.split('"').skip(1).step_by(2);
        if !line.starts_with("Option") {
            continue;
        }
        let (Some(name), Some(value)) = (quoted.next(), quoted.next()) else {
            continue;
        };
        match name {
            "XkbLayout" => found.layout = value.to_owned(),
            "XkbVariant" => found.variant = value.to_owned(),
            "XkbOptions" => found.options = Some(value.to_owned()),
            _ => {}
        }
    }
    (!found.layout.is_empty()).then_some(found)
}

/// The layout given on the command line, else `XKB_DEFAULT_LAYOUT` (handled
/// by XKB itself when the layout is empty), else the system's X11 layout.
pub fn detect(requested: Option<&str>, variant: Option<&str>) -> KeyboardLayout {
    if let Some(layout) = requested {
        return KeyboardLayout {
            layout: layout.to_owned(),
            variant: variant.unwrap_or_default().to_owned(),
            options: None,
        };
    }
    if std::env::var_os("XKB_DEFAULT_LAYOUT").is_some() {
        return KeyboardLayout::default();
    }
    fs::read_to_string(XORG_KEYBOARD_CONF)
        .ok()
        .and_then(|text| parse_xorg_conf(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRENCH: &str = r#"
# Written by systemd-localed(8), read by systemd-localed and Xorg.
Section "InputClass"
        Identifier "system-keyboard"
        MatchIsKeyboard "on"
        Option "XkbLayout" "fr"
EndSection
"#;

    #[test]
    fn the_layout_is_read_from_the_xorg_conf() {
        assert_eq!(
            parse_xorg_conf(FRENCH),
            Some(KeyboardLayout {
                layout: "fr".into(),
                variant: String::new(),
                options: None,
            })
        );
    }

    #[test]
    fn variant_and_options_are_read_too() {
        let conf = r#"
            Option "XkbLayout" "us"
            Option "XkbVariant" "intl"
            Option "XkbOptions" "caps:escape,compose:ralt"
        "#;

        assert_eq!(
            parse_xorg_conf(conf),
            Some(KeyboardLayout {
                layout: "us".into(),
                variant: "intl".into(),
                options: Some("caps:escape,compose:ralt".into()),
            })
        );
    }

    #[test]
    fn a_conf_without_layout_gives_nothing() {
        assert_eq!(
            parse_xorg_conf("Section \"InputClass\"\nEndSection\n"),
            None
        );
    }

    #[test]
    fn commented_lines_are_ignored() {
        let conf = "# Option \"XkbLayout\" \"de\"\nOption \"XkbLayout\" \"fr\"\n";

        assert_eq!(parse_xorg_conf(conf).unwrap().layout, "fr");
    }
}
