//! The games installed with Steam, read from its library folders.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
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

/// The folders of the libraries of the user, the one in Steam's own
/// folder included.
fn libraries(root: &Path) -> Vec<PathBuf> {
    let folders = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf"));
    let mut libraries = folders
        .map(|text| parse_libraries(&text))
        .unwrap_or_default();
    if !libraries.iter().any(|library| library == root) {
        libraries.push(root.to_owned());
    }
    libraries
}

/// The manifests of the titles installed in a library.
fn manifests(library: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(library.join("steamapps")) else {
        return Vec::new();
    };
    let paths = entries.flatten().map(|entry| entry.path());
    let paths = paths.filter(|path| {
        let name = path.file_name().and_then(|name| name.to_str());
        name.is_some_and(|name| name.starts_with("appmanifest_"))
    });
    paths.collect()
}

/// Reads the library of the user. Empty when Steam is not installed.
pub fn load() -> Vec<Game> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for path in libraries(&root)
        .iter()
        .flat_map(|library| manifests(library))
    {
        let manifest = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some(mut game) = parse_manifest(&manifest) {
            game.cover = cover(&root, game.id);
            games.push(game);
        }
    }
    sorted(games)
}

/// Which titles are installed, summed up: it changes when one is installed
/// or removed, not while Steam rewrites their manifests (it does during a
/// download).
pub fn fingerprint() -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(root) = steam_root() {
        let mut found: Vec<PathBuf> = libraries(&root)
            .iter()
            .flat_map(|library| manifests(library))
            .collect();
        found.sort();
        found.hash(&mut hasher);
    }
    hasher.finish()
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

/// Parses playtime in minutes per Steam app ID from `localconfig.vdf`.
#[allow(dead_code)]
pub fn parse_playtimes(vdf: &str) -> HashMap<u64, u32> {
    let mut playtimes = HashMap::new();
    let tokens = vdf_tokens(vdf);
    let mut depth: usize = 0;
    let mut apps_depth: Option<usize> = None;
    let mut current_app: Option<u64> = None;
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            VdfToken::Open => {
                depth += 1;
                i += 1;
            }
            VdfToken::Close => {
                if apps_depth == Some(depth) {
                    apps_depth = None;
                    current_app = None;
                } else if apps_depth.is_some_and(|ad| depth == ad + 1) {
                    current_app = None;
                }
                depth = depth.saturating_sub(1);
                i += 1;
            }
            VdfToken::Text(key) => {
                let next = tokens.get(i + 1);
                match next {
                    Some(VdfToken::Open) => {
                        if key.eq_ignore_ascii_case("apps") && apps_depth.is_none() {
                            apps_depth = Some(depth + 1);
                        } else if apps_depth == Some(depth) {
                            current_app = key.parse::<u64>().ok();
                        }
                        depth += 1;
                        i += 2;
                    }
                    Some(VdfToken::Text(value)) => {
                        if let (Some(ad), Some(app_id)) = (apps_depth, current_app)
                            && depth == ad + 1
                            && (key.eq_ignore_ascii_case("Playtime")
                                || key.eq_ignore_ascii_case("PlaytimeForever"))
                            && let Ok(mins) = value.parse::<u32>()
                        {
                            playtimes
                                .entry(app_id)
                                .and_modify(|existing: &mut u32| *existing = (*existing).max(mins))
                                .or_insert(mins);
                        }
                        i += 2;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        }
    }

    playtimes
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
enum VdfToken {
    Open,
    Close,
    Text(String),
}

#[allow(dead_code)]
fn vdf_tokens(input: &str) -> Vec<VdfToken> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some(&(idx, ch)) = chars.peek() {
        match ch {
            c if c.is_whitespace() => {
                chars.next();
            }
            '/' if input[idx..].starts_with("//") => {
                while let Some(&(_, c)) = chars.peek() {
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            }
            '{' => {
                tokens.push(VdfToken::Open);
                chars.next();
            }
            '}' => {
                tokens.push(VdfToken::Close);
                chars.next();
            }
            '"' => {
                chars.next();
                let mut text = String::new();
                while let Some((_, c)) = chars.next() {
                    if c == '"' {
                        break;
                    }
                    if c == '\\'
                        && let Some((_, escaped)) = chars.next()
                    {
                        text.push(escaped);
                        continue;
                    }
                    text.push(c);
                }
                tokens.push(VdfToken::Text(text));
            }
            _ => {
                let mut text = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    text.push(c);
                    chars.next();
                }
                if !text.is_empty() {
                    tokens.push(VdfToken::Text(text));
                }
            }
        }
    }

    tokens
}

/// Scans `<steam_root>/userdata/*/config/localconfig.vdf` and merges max playtime per app ID.
#[allow(dead_code)]
pub fn load_playtimes() -> HashMap<u64, u32> {
    let mut out = HashMap::new();
    let Some(root) = steam_root() else {
        return out;
    };
    let Ok(users) = std::fs::read_dir(root.join("userdata")) else {
        return out;
    };
    for entry in users.flatten() {
        let path = entry.path().join("config/localconfig.vdf");
        if let Ok(content) = std::fs::read_to_string(path) {
            for (id, mins) in parse_playtimes(&content) {
                out.entry(id)
                    .and_modify(|existing: &mut u32| *existing = (*existing).max(mins))
                    .or_insert(mins);
            }
        }
    }
    out
}

/// Formats a French pill for a game's last-played date and total playtime.
#[allow(dead_code)]
pub fn format_last_played(last_played: u64, playtime_mins: u32, now_secs: i64) -> String {
    let day_part = if last_played > 0 {
        let diff_secs = now_secs.saturating_sub(last_played as i64).max(0);
        let days = diff_secs / 86_400;
        let label = if days == 0 {
            "Aujourd'hui".to_owned()
        } else if days == 1 {
            "Hier".to_owned()
        } else if days < 30 {
            format!("Il y a {days} j")
        } else {
            let months = days / 30;
            format!("Il y a {months} mois")
        };
        Some(label)
    } else {
        None
    };

    let time_part = if playtime_mins >= 60 {
        Some(format!("{} h", playtime_mins / 60))
    } else if playtime_mins > 0 {
        Some(format!("{playtime_mins} min"))
    } else {
        None
    };

    match (day_part, time_part) {
        (Some(d), Some(t)) => format!("{d} · {t}"),
        (Some(d), None) => d,
        (None, Some(t)) => t,
        (None, None) => "Installé".to_owned(),
    }
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

    #[test]
    fn parse_playtimes_extracts_playtime_and_ignores_malformed_sections() {
        let vdf = r#"
"UserLocalConfigStore"
{
    "Software"
    {
        "Valve"
        {
            "Steam"
            {
                "apps"
                {
                    "427520"
                    {
                        "Playtime"		"8520"
                        "LastPlayed"	"1732600000"
                        "nested"
                        {
                            "Playtime"	"999999"
                        }
                    }
                    "1284210"
                    {
                        "PlaytimeForever"	"45"
                    }
                    "not_an_id"
                    {
                        "Playtime"	"100"
                    }
                }
            }
        }
    }
}
"#;
        let map = parse_playtimes(vdf);
        assert_eq!(map.get(&427_520), Some(&8520));
        assert_eq!(map.get(&1_284_210), Some(&45));
        assert_eq!(map.len(), 2);

        assert!(parse_playtimes("").is_empty());
        assert!(parse_playtimes("\"apps\" { \"123\" { \"Playtime\" \"bad\" }").is_empty());
    }

    #[test]
    fn format_last_played_formats_french_pills() {
        let now = 1_732_700_000i64;
        assert_eq!(format_last_played(0, 0, now), "Installé");
        assert_eq!(format_last_played(0, 45, now), "45 min");
        assert_eq!(format_last_played(0, 120, now), "2 h");
        assert_eq!(
            format_last_played((now - 3_600) as u64, 90, now),
            "Aujourd'hui · 1 h"
        );
        assert_eq!(
            format_last_played((now - 90_000) as u64, 8_520, now),
            "Hier · 142 h"
        );
        assert_eq!(
            format_last_played((now - 5 * 86_400) as u64, 0, now),
            "Il y a 5 j"
        );
        assert_eq!(
            format_last_played((now - 65 * 86_400) as u64, 30, now),
            "Il y a 2 mois · 30 min"
        );
    }
}
