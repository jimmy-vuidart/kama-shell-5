//! The games installed with Steam, read from its library folders.

use std::path::{Path, PathBuf};

/// A game of the Steam library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub id: u64,
    pub name: String,
    /// When it was played last, in seconds since 1970; 0 if never.
    pub last_played: u64,
    /// Its picture, when Steam has one.
    pub cover: Option<PathBuf>,
}

impl Game {
    /// What the windows of the game call themselves: how the compositor
    /// knows them for games.
    pub fn window_class(&self) -> String {
        format!("steam_app_{}", self.id)
    }

    /// The command that starts the game, through Steam.
    pub fn command(&self) -> String {
        format!("steam steam://rungameid/{}", self.id)
    }
}

/// The number of the game a command starts, when it is one of the commands
/// Steam writes in the desktop files of games
/// (`steam steam://rungameid/1366540`).
pub fn game_of_command(command: &str) -> Option<u64> {
    let (_, number) = command.split_once("steam://rungameid/")?;
    let digits: String = number.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The value of `key` at the top level of a Steam manifest (`"key" "value"`
/// pairs, one per line).
fn manifest_value<'a>(manifest: &'a str, key: &str) -> Option<&'a str> {
    manifest.lines().find_map(|line| {
        let mut quoted = line.trim().split('"');
        // `"key"<tabs>"value"`: empty, key, tabs, value, empty.
        let found = quoted.nth(1)?;
        let _between = quoted.next()?;
        let value = quoted.next()?;
        (found.eq_ignore_ascii_case(key)).then_some(value)
    })
}

/// Reads the manifest of an installed title. `None` for what Steam installs
/// besides games: its runtimes, Proton, redistributables.
pub fn parse_manifest(manifest: &str) -> Option<Game> {
    let name = manifest_value(manifest, "name")?;
    let tools = ["Proton", "Steam Linux Runtime", "Steamworks"];
    if tools.iter().any(|tool| name.starts_with(tool)) {
        return None;
    }
    Some(Game {
        id: manifest_value(manifest, "appid")?.parse().ok()?,
        name: name.to_owned(),
        last_played: manifest_value(manifest, "LastPlayed")
            .and_then(|played| played.parse().ok())
            .unwrap_or(0),
        cover: None,
    })
}

/// The folders games are installed in, from `libraryfolders.vdf`.
pub fn parse_libraries(folders: &str) -> Vec<PathBuf> {
    let paths = folders.lines().filter_map(|line| {
        let mut quoted = line.trim().split('"');
        let key = quoted.nth(1)?;
        let _between = quoted.next()?;
        let value = quoted.next()?;
        (key == "path").then(|| PathBuf::from(value))
    });
    paths.collect()
}

/// The games to show, the one played last first.
pub fn sorted(mut games: Vec<Game>) -> Vec<Game> {
    games.sort_by(|a, b| {
        let recent = b.last_played.cmp(&a.last_played);
        recent.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    games.dedup_by_key(|game| game.id);
    games
}

/// Where Steam keeps its files, if it is installed.
fn steam_root() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let candidates = [".local/share/Steam", ".steam/steam"];
    let mut roots = candidates.iter().map(|path| home.join(path));
    roots.find(|root| root.join("steamapps").is_dir())
}

/// Reads the library of the user. Empty when Steam is not installed.
pub fn load() -> Vec<Game> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let folders = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf"));
    let mut libraries = folders
        .map(|text| parse_libraries(&text))
        .unwrap_or_default();
    if !libraries.contains(&root) {
        libraries.push(root.clone());
    }

    let mut games = Vec::new();
    for library in libraries {
        let Ok(entries) = std::fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            let name = path.file_name().and_then(|name| name.to_str());
            if !name.is_some_and(|name| name.starts_with("appmanifest_")) {
                continue;
            }
            let manifest = std::fs::read_to_string(&path).unwrap_or_default();
            if let Some(mut game) = parse_manifest(&manifest) {
                game.cover = cover(&root, game.id);
                games.push(game);
            }
        }
    }
    sorted(games)
}

/// The picture of a game in Steam's cache. Steam keeps them next to each
/// other or one folder down, depending on its version.
fn cover(root: &Path, id: u64) -> Option<PathBuf> {
    let cache = root.join("appcache/librarycache").join(id.to_string());
    let list = |directory: &Path| -> Vec<PathBuf> {
        let entries = std::fs::read_dir(directory).into_iter().flatten().flatten();
        entries.map(|entry| entry.path()).collect()
    };
    let mut files = list(&cache);
    let nested: Vec<PathBuf> = files
        .iter()
        .filter(|path| path.is_dir())
        .flat_map(|path| list(path))
        .collect();
    files.extend(nested);
    pick_cover(&files)
}

/// Among the pictures Steam has of a game, the one that suits a wide card
/// best: the header first, a tall capsule as a last resort.
pub fn pick_cover(files: &[PathBuf]) -> Option<PathBuf> {
    let wanted = [
        "header.jpg",
        "library_header.jpg",
        "library_hero.jpg",
        "library_capsule.jpg",
        "library_600x900.jpg",
    ];
    let named = |name: &str| {
        files
            .iter()
            .find(|file| file.file_name().is_some_and(|own| own == name))
    };
    wanted.iter().find_map(|name| named(name)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"1366540\"\n\t\"Universe\"\t\t\"1\"\n\t\"name\"\t\t\"Dyson Sphere Program\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"LastPlayed\"\t\t\"1790027315\"\n\t\"InstalledDepots\"\n\t{\n\t\t\"1366541\"\n\t\t{\n\t\t\t\"manifest\"\t\t\"1\"\n\t\t}\n\t}\n}\n";

    fn game(id: u64, name: &str, last_played: u64) -> Game {
        Game {
            id,
            name: name.into(),
            last_played,
            cover: None,
        }
    }

    #[test]
    fn a_manifest_gives_the_game() {
        assert_eq!(
            parse_manifest(MANIFEST),
            Some(game(1_366_540, "Dyson Sphere Program", 1_790_027_315))
        );
    }

    #[test]
    fn a_game_never_played_is_still_a_game() {
        let manifest = "\t\"appid\"\t\t\"12\"\n\t\"name\"\t\t\"Factorio\"\n";

        assert_eq!(parse_manifest(manifest), Some(game(12, "Factorio", 0)));
    }

    #[test]
    fn what_steam_installs_for_itself_is_not_a_game() {
        for name in [
            "Proton 9.0 (Beta)",
            "Proton Voice Files",
            "Steam Linux Runtime 3.0 (sniper)",
            "Steamworks Common Redistributables",
        ] {
            let manifest = format!("\"appid\"\t\"5\"\n\"name\"\t\"{name}\"\n");
            assert_eq!(parse_manifest(&manifest), None, "{name}");
        }
    }

    #[test]
    fn a_broken_manifest_gives_nothing() {
        assert_eq!(parse_manifest(""), None);
        assert_eq!(parse_manifest("\"name\"\t\"Factorio\"\n"), None);
        assert_eq!(
            parse_manifest("\"appid\"\t\"abc\"\n\"name\"\t\"X\"\n"),
            None
        );
    }

    #[test]
    fn a_game_is_known_by_the_class_of_its_windows() {
        let game = game(1_366_540, "Dyson Sphere Program", 0);

        assert_eq!(game.window_class(), "steam_app_1366540");
        assert_eq!(game.command(), "steam steam://rungameid/1366540");
    }

    #[test]
    fn the_shortcut_of_a_game_gives_its_number() {
        assert_eq!(
            game_of_command("steam steam://rungameid/1366540"),
            Some(1_366_540)
        );
        assert_eq!(game_of_command("steam steam://rungameid/42 %U"), Some(42));
        assert_eq!(game_of_command("steam -silent"), None);
        assert_eq!(game_of_command("steam steam://rungameid/"), None);
    }

    #[test]
    fn library_folders_are_listed() {
        let folders = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/me/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games/SteamLibrary\"\n\t}\n}\n";

        assert_eq!(
            parse_libraries(folders),
            [
                PathBuf::from("/home/me/.local/share/Steam"),
                PathBuf::from("/mnt/games/SteamLibrary")
            ]
        );
    }

    #[test]
    fn the_wide_picture_of_a_game_is_preferred_wherever_it_is() {
        let files = [
            PathBuf::from("cache/7/logo.png"),
            PathBuf::from("cache/7/abc/library_capsule.jpg"),
            PathBuf::from("cache/7/def/library_header.jpg"),
            PathBuf::from("cache/7/def/library_hero_blur.jpg"),
        ];

        assert_eq!(
            pick_cover(&files),
            Some(PathBuf::from("cache/7/def/library_header.jpg"))
        );
        assert_eq!(pick_cover(&files[..2]), Some(files[1].clone()));
        assert_eq!(pick_cover(&files[..1]), None);
    }

    #[test]
    fn games_are_sorted_from_the_one_played_last_then_by_name() {
        let games = sorted(vec![
            game(1, "Zelda", 0),
            game(2, "Factorio", 50),
            game(3, "anno", 0),
            game(4, "No Man's Sky", 90),
            game(2, "Factorio", 50),
        ]);

        let names: Vec<&str> = games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["No Man's Sky", "Factorio", "anno", "Zelda"]);
    }
}
