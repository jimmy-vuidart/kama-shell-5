//! A made-up world, for trying the dock without a compositor nor any
//! service: an evening of raid, as in the mockups.

use coquille_ipc::{Habit, Screen, Snapshot, ThemeName, Window};
use coquille_model::{LayoutKind, OutputRole};

use super::{Audio, AudioSink, Switch};
use crate::model::mixer::Stream;

/// Three screens, a game at the center, the messenger in use on the left.
pub fn snapshot(theme: ThemeName, favorites: &[String]) -> Snapshot {
    let screen = |name: &str, role, width, height, layout, anchored| Screen {
        name: name.to_owned(),
        role,
        x: 0,
        y: 0,
        width,
        height,
        layout,
        anchored,
        scale: Default::default(),
        pixel_width: width,
        pixel_height: height,
    };
    let window = |id, app: &str, title: &str, screen, slot, active| Window {
        id,
        app: app.to_owned(),
        title: title.to_owned(),
        screen,
        slot,
        active,
        minimized: false,
    };
    // The applications of the mockups, under the names of the favorites
    // when there are enough of them.
    let name = |index: usize, fallback: &str| {
        favorites
            .get(index)
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    };
    let (browser, messenger) = (name(0, "firefox"), name(1, "discord"));
    Snapshot {
        theme,
        accent: String::new(),
        simulated: true,
        spatial: false,
        screens: vec![
            screen(
                "mock-left",
                OutputRole::Left,
                1920,
                1080,
                LayoutKind::TwoColumns,
                false,
            ),
            screen(
                "mock-center",
                OutputRole::Center,
                3440,
                1440,
                LayoutKind::Full,
                true,
            ),
            screen(
                "mock-right",
                OutputRole::Right,
                1920,
                1080,
                LayoutKind::TwoColumns,
                false,
            ),
        ],
        windows: vec![
            window(
                2,
                &messenger,
                "# raid-mardi",
                OutputRole::Left,
                Some(0),
                true,
            ),
            window(
                3,
                "steam_app_0",
                "Cendres d'Aube",
                OutputRole::Center,
                None,
                false,
            ),
            window(
                1,
                &browser,
                "Guide du raid",
                OutputRole::Right,
                Some(0),
                false,
            ),
            window(
                4,
                "notes",
                "raid-mardi.md",
                OutputRole::Left,
                Some(1),
                false,
            ),
        ],
        habits: vec![
            Habit {
                app: browser,
                screen: OutputRole::Right,
                slot: 0,
            },
            Habit {
                app: messenger,
                screen: OutputRole::Left,
                slot: 0,
            },
        ],
        x11_scale: Default::default(),
        touchpad: false,
    }
}

pub fn audio(favorites: &[String]) -> Audio {
    let stream = |id, app: &str, media: &str, volume, muted, paused| Stream {
        id,
        app: app.to_owned(),
        media: media.to_owned(),
        volume,
        muted,
        paused,
    };
    let name = |index: usize, fallback: &str| {
        favorites
            .get(index)
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    };
    Audio {
        output: "Casque USB".to_owned(),
        output_name: "mock-headphones".to_owned(),
        volume: 0.55,
        muted: false,
        streams: vec![
            stream(1, "steam_app_0", "Jeu · en cours", 0.8, false, false),
            stream(2, &name(1, "discord"), "Vocal du raid", 0.65, false, false),
            stream(3, &name(0, "firefox"), "", 0.4, false, true),
            stream(4, "musique", "", 0.0, true, false),
            stream(5, "sons-systeme", "Notifications", 0.25, false, false),
        ],
        sinks: vec![
            AudioSink {
                name: "mock-headphones".to_owned(),
                description: "Casque USB".to_owned(),
            },
            AudioSink {
                name: "mock-speakers".to_owned(),
                description: "Enceintes stéréo".to_owned(),
            },
            AudioSink {
                name: "mock-hdmi".to_owned(),
                description: "Écran Dell AW3418DW".to_owned(),
            },
        ],
    }
}

/// How loud the made-up streams are at a moment: they breathe a little.
pub fn levels(audio: &Audio, tick: u64) -> Vec<(u32, f32)> {
    let level = |stream: &Stream| {
        if stream.muted || stream.paused {
            return 0.0;
        }
        let phase = (tick as f32 * 0.35 + stream.id as f32 * 1.7).sin() * 0.5 + 0.5;
        stream.volume * (0.45 + 0.5 * phase)
    };
    audio
        .streams
        .iter()
        .map(|stream| (stream.id, level(stream)))
        .collect()
}

pub fn network() -> Switch {
    Switch {
        on: true,
        detail: "Ethernet".to_owned(),
        available: true,
    }
}

pub fn bluetooth() -> Switch {
    Switch {
        on: true,
        detail: "Casque".to_owned(),
        available: true,
    }
}

/// Unread messages: only the simulation has some for now.
pub fn unread(favorites: &[String]) -> Vec<(String, u32)> {
    let messenger = favorites
        .get(1)
        .cloned()
        .unwrap_or_else(|| "discord".to_owned());
    vec![(messenger, 3)]
}

/// Simulated system tray items for trying the tray without running applications.
pub fn tray() -> Vec<crate::model::tray::TrayItem> {
    use crate::model::tray::TrayItem;
    use system_tray::menu::{MenuItem, MenuType, TrayMenu};

    let item = |address: &str,
                id: &str,
                title: &str,
                tooltip: &str,
                icon: &str,
                menu: Option<TrayMenu>| TrayItem {
        address: address.to_owned(),
        id: id.to_owned(),
        title: title.to_owned(),
        tooltip: Some(tooltip.to_owned()),
        icon_name: Some(icon.to_owned()),
        icon_theme_path: None,
        icon_file: None,
        pixmap: None,
        menu_path: menu.as_ref().map(|_| "/MenuBar".to_owned()),
        menu,
    };

    let cloud_menu = TrayMenu {
        id: 1,
        submenus: vec![
            MenuItem {
                id: 1,
                menu_type: MenuType::Standard,
                label: Some("Ouvrir le dossier".to_owned()),
                enabled: true,
                visible: true,
                ..Default::default()
            },
            MenuItem {
                id: 2,
                menu_type: MenuType::Standard,
                label: Some("Pause de la synchronisation".to_owned()),
                enabled: true,
                visible: true,
                ..Default::default()
            },
            MenuItem {
                id: 3,
                menu_type: MenuType::Separator,
                visible: true,
                ..Default::default()
            },
            MenuItem {
                id: 4,
                menu_type: MenuType::Standard,
                label: Some("Quitter".to_owned()),
                enabled: true,
                visible: true,
                ..Default::default()
            },
        ],
    };

    vec![
        item(
            ":mock/player",
            "mock-player",
            "Musique",
            "Musique · En lecture",
            "audio-player",
            None,
        ),
        item(
            ":mock/cloud",
            "mock-cloud",
            "Cloud",
            "Cloud · Synchronisé",
            "folder-cloud",
            Some(cloud_menu),
        ),
        item(
            ":mock/obs",
            "mock-obs",
            "Enregistrement",
            "Enregistrement · Prêt",
            "camera-video",
            None,
        ),
    ]
}
