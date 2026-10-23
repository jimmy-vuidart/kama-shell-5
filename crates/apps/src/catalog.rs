//! Desktop entries: reading one, and the catalog of all of them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// An application the user can start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct App {
    /// The name of its desktop file, without the extension: `firefox`,
    /// `org.gnome.Nautilus`.
    pub id: String,
    pub name: String,
    /// The command that starts it, ready for a shell.
    pub command: String,
    /// A name of the icon theme, or the path of an image.
    pub icon: Option<String>,
    /// What its windows call themselves, when that is not `id`.
    pub wm_class: Option<String>,
    /// What kind of application it is (`GenericName`): « Navigateur web ».
    pub generic_name: Option<String>,
    /// Other words it can be found by (`Keywords`): « Internet », « WWW ».
    pub keywords: Vec<String>,
    /// What it can be started to do besides opening (`[Desktop Action]`):
    /// « Nouvelle fenêtre privée ».
    pub actions: Vec<AppAction>,
}

/// One of the actions of a desktop entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppAction {
    pub name: String,
    /// The command that does it, ready for a shell.
    pub command: String,
}

/// Reads a desktop entry. `None` when it is not an application to list:
/// hidden, without a command, or only there to open files.
/// `languages` are tried in order for the translated name (`fr_FR`, `fr`).
pub fn parse_entry(id: &str, text: &str, languages: &[String]) -> Option<App> {
    // The keys of each group: `[Desktop Entry]`, `[Desktop Action new-window]`…
    let mut groups: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
    let mut group = None;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            group = Some(name);
        } else if let Some(group) = group.filter(|_| !line.starts_with('#')) {
            if let Some((key, value)) = line.split_once('=') {
                let fields = groups.entry(group).or_default();
                fields.entry(key.trim()).or_insert_with(|| value.trim());
            }
        }
    }
    let entry = groups.get("Desktop Entry")?;
    let field = |key: &str| entry.get(key).copied().filter(|value| !value.is_empty());
    let is = |key: &str| field(key) == Some("true");
    if field("Type") != Some("Application") || is("NoDisplay") || is("Hidden") {
        return None;
    }
    let terminal = is("Terminal");
    let command = runnable(field("Exec")?, terminal)?;
    let actions = field("Actions").map_or_else(Vec::new, list).into_iter();
    let actions = actions
        .filter_map(|action: String| {
            let fields = groups.get(format!("Desktop Action {action}").as_str())?;
            let name = translated(fields, "Name", languages)?;
            let command = runnable(fields.get("Exec")?, terminal)?;
            Some(AppAction { name, command })
        })
        .collect();
    Some(App {
        id: id.to_owned(),
        name: translated(entry, "Name", languages)?,
        command,
        icon: field("Icon").map(str::to_owned),
        wm_class: field("StartupWMClass").map(str::to_owned),
        generic_name: translated(entry, "GenericName", languages),
        keywords: translated(entry, "Keywords", languages)
            .map_or_else(Vec::new, |words| list(&words)),
        actions,
    })
}

/// The value of `key` in the first of `languages` it is translated into,
/// else as it is written.
fn translated(fields: &BTreeMap<&str, &str>, key: &str, languages: &[String]) -> Option<String> {
    let value = |key: &str| fields.get(key).copied().filter(|value| !value.is_empty());
    languages
        .iter()
        .find_map(|language| value(&format!("{key}[{language}]")))
        .or_else(|| value(key))
        .map(str::to_owned)
}

/// The items of a list value (`Actions`, `Keywords`), separated by `;`
/// (written `\;` inside an item).
fn list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => item.extend(characters.next()),
            ';' => items.push(std::mem::take(&mut item)),
            other => item.push(other),
        }
    }
    items.push(item);
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// The command of an `Exec` line, ready for a shell: in a terminal when the
/// entry says so. `None` when nothing is left once the placeholders go.
fn runnable(exec: &str, terminal: bool) -> Option<String> {
    let command = command_of(exec);
    if command.is_empty() {
        None
    } else if terminal {
        Some(format!("xdg-terminal-exec {command}"))
    } else {
        Some(command)
    }
}

/// The command of an `Exec` line, without the placeholders for files and
/// addresses (`%f`, `%U`…) the dock never fills.
pub fn command_of(exec: &str) -> String {
    let mut command = String::new();
    let mut characters = exec.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            command.push(character);
        } else if characters.next() == Some('%') {
            command.push('%');
        }
    }
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The languages to look translated names up in, from the most to the least
/// specific, given `LANG` (`fr_FR.UTF-8` gives `fr_FR`, `fr`).
pub fn languages(lang: &str) -> Vec<String> {
    let locale = lang.split(['.', '@']).next().unwrap_or_default();
    if locale.is_empty() || locale == "C" || locale == "POSIX" {
        return Vec::new();
    }
    let mut languages = vec![locale.to_owned()];
    if let Some((language, _)) = locale.split_once('_') {
        languages.push(language.to_owned());
    }
    languages
}

/// Every application installed, in alphabetical order of their identifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    apps: Vec<App>,
}

impl Catalog {
    pub fn new(mut apps: Vec<App>) -> Self {
        apps.sort_by(|a, b| a.id.cmp(&b.id));
        apps.dedup_by(|a, b| a.id == b.id);
        Self { apps }
    }

    /// Reads the desktop files of the system and of the user.
    pub fn load() -> Self {
        let languages = languages(&std::env::var("LANG").unwrap_or_default());
        let mut apps: BTreeMap<String, App> = BTreeMap::new();
        // The directories that come first win, hence the reverse order.
        for directory in data_directories().into_iter().rev() {
            read_directory(&directory.join("applications"), &languages, &mut apps);
        }
        Self::new(apps.into_values().collect())
    }

    pub fn apps(&self) -> &[App] {
        &self.apps
    }

    /// The application a name stands for: an identifier written in the
    /// configuration (`firefox`, `code`), or what a window calls itself
    /// (`app_id`, `WM_CLASS`). Case does not matter.
    pub fn find(&self, name: &str) -> Option<&App> {
        let wanted = name.to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        let by = |matches: &dyn Fn(&App) -> bool| self.apps.iter().find(|app| matches(app));
        let last_part = |id: &str| id.rsplit('.').next().unwrap_or_default().to_lowercase();
        let program = |app: &App| {
            let first = app.command.split_whitespace().next().unwrap_or_default();
            first.rsplit('/').next().unwrap_or_default().to_lowercase()
        };
        by(&|app| app.id.to_lowercase() == wanted)
            .or_else(|| {
                by(&|app| {
                    app.wm_class
                        .as_ref()
                        .is_some_and(|class| class.to_lowercase() == wanted)
                })
            })
            .or_else(|| by(&|app| last_part(&app.id) == wanted))
            .or_else(|| by(&|app| program(app) == wanted))
    }
}

/// Where desktop files and icons are looked for, the user's own first.
pub fn data_directories() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let user = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(home?.join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    let system = system.split(':').filter(|path| !path.is_empty());
    user.into_iter().chain(system.map(PathBuf::from)).collect()
}

fn read_directory(directory: &Path, languages: &[String], apps: &mut BTreeMap<String, App>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|name| name.to_str());
        let Some(id) = name.and_then(|name| name.strip_suffix(".desktop")) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // A file that hides an application hides the ones it shadows too.
        match parse_entry(id, &text, languages) {
            Some(app) => apps.insert(id.to_owned(), app),
            None => apps.remove(id),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAUTILUS: &str = "\
[Desktop Entry]
Name=Files
Name[fr]=Fichiers
Name[fr_CA]=Dossiers
Exec=nautilus --new-window %U
Icon=org.gnome.Nautilus
Type=Application
StartupWMClass=org.gnome.Nautilus

[Desktop Action new-window]
Name=New Window
Exec=nautilus --other
";

    fn french() -> Vec<String> {
        languages("fr_FR.UTF-8")
    }

    fn app(id: &str, command: &str, wm_class: Option<&str>) -> App {
        App {
            id: id.into(),
            name: id.into(),
            command: command.into(),
            wm_class: wm_class.map(str::to_owned),
            ..App::default()
        }
    }

    const FIREFOX: &str = "\
[Desktop Entry]
Name=Firefox
GenericName=Web Browser
GenericName[fr]=Navigateur web
Keywords=Internet;WWW;Browser;
Keywords[fr]=Internet;WWW;Navigateur;Web\\;Toile;
Exec=firefox %u
Terminal=false
Type=Application
Actions=new-window;new-private-window;missing;

[Desktop Action new-private-window]
Name=New Private Window
Name[fr]=Nouvelle fenêtre de navigation privée
Exec=firefox --private-window %u

[Desktop Action new-window]
Name=New Window
Name[fr]=Nouvelle fenêtre
Exec=firefox --new-window %u

[Desktop Action profile-manager]
Name=Profile Manager
Exec=firefox --ProfileManager
";

    #[test]
    fn an_entry_gives_its_kind_and_keywords_in_the_language_of_the_user() {
        let app = parse_entry("firefox", FIREFOX, &french()).unwrap();

        assert_eq!(app.generic_name.as_deref(), Some("Navigateur web"));
        assert_eq!(app.keywords, ["Internet", "WWW", "Navigateur", "Web;Toile"]);
        let english = parse_entry("firefox", FIREFOX, &[]).unwrap();
        assert_eq!(english.generic_name.as_deref(), Some("Web Browser"));
        assert_eq!(english.keywords, ["Internet", "WWW", "Browser"]);
    }

    #[test]
    fn the_actions_listed_by_an_entry_are_given_in_its_order() {
        let app = parse_entry("firefox", FIREFOX, &french()).unwrap();

        let actions: Vec<_> = app
            .actions
            .iter()
            .map(|action| (action.name.as_str(), action.command.as_str()))
            .collect();
        // `missing` has no group and `profile-manager` is not listed.
        assert_eq!(
            actions,
            [
                ("Nouvelle fenêtre", "firefox --new-window"),
                (
                    "Nouvelle fenêtre de navigation privée",
                    "firefox --private-window"
                ),
            ]
        );
    }

    #[test]
    fn an_entry_without_actions_has_none() {
        // Nautilus has an action group, but does not list it.
        assert!(
            parse_entry("nautilus", NAUTILUS, &[])
                .unwrap()
                .actions
                .is_empty()
        );
    }

    #[test]
    fn the_actions_of_a_terminal_application_run_in_a_terminal() {
        let text = "[Desktop Entry]\nName=Htop\nExec=htop\nType=Application\nTerminal=true\n\
                    Actions=tree;\n[Desktop Action tree]\nName=Arbre\nExec=htop --tree\n";

        let app = parse_entry("htop", text, &[]).unwrap();

        assert_eq!(app.actions[0].command, "xdg-terminal-exec htop --tree");
    }

    #[test]
    fn an_entry_gives_its_translated_name_command_and_icon() {
        let app = parse_entry("org.gnome.Nautilus", NAUTILUS, &french()).unwrap();

        assert_eq!(app.name, "Fichiers");
        assert_eq!(app.command, "nautilus --new-window");
        assert_eq!(app.icon.as_deref(), Some("org.gnome.Nautilus"));
        assert_eq!(app.wm_class.as_deref(), Some("org.gnome.Nautilus"));
    }

    #[test]
    fn the_most_specific_translation_wins() {
        let app = parse_entry("nautilus", NAUTILUS, &languages("fr_CA.UTF-8")).unwrap();

        assert_eq!(app.name, "Dossiers");
    }

    #[test]
    fn without_translation_the_plain_name_is_used() {
        let app = parse_entry("nautilus", NAUTILUS, &languages("de_DE.UTF-8")).unwrap();

        assert_eq!(app.name, "Files");
        assert_eq!(parse_entry("n", NAUTILUS, &[]).unwrap().name, "Files");
    }

    #[test]
    fn hidden_entries_and_entries_that_are_not_applications_are_left_out() {
        let entry = |extra: &str| format!("[Desktop Entry]\nName=X\nExec=x\n{extra}\n");

        assert!(parse_entry("x", &entry("Type=Application"), &[]).is_some());
        assert!(parse_entry("x", &entry("Type=Application\nNoDisplay=true"), &[]).is_none());
        assert!(parse_entry("x", &entry("Type=Application\nHidden=true"), &[]).is_none());
        assert!(parse_entry("x", &entry("Type=Link"), &[]).is_none());
        assert!(parse_entry("x", "[Desktop Entry]\nName=X\nType=Application\n", &[]).is_none());
    }

    #[test]
    fn a_terminal_application_is_started_in_a_terminal() {
        let text = "[Desktop Entry]\nName=Htop\nExec=htop\nType=Application\nTerminal=true\n";

        let app = parse_entry("htop", text, &[]).unwrap();

        assert_eq!(app.command, "xdg-terminal-exec htop");
    }

    #[test]
    fn placeholders_are_removed_from_commands() {
        assert_eq!(command_of("firefox %u"), "firefox");
        assert_eq!(command_of("code --unity-launch %F"), "code --unity-launch");
        assert_eq!(command_of("sh -c \"echo 100%%\""), "sh -c \"echo 100%\"");
        assert_eq!(command_of("%c"), "");
    }

    #[test]
    fn languages_go_from_the_region_to_the_language() {
        assert_eq!(languages("fr_FR.UTF-8"), ["fr_FR", "fr"]);
        assert_eq!(languages("fr"), ["fr"]);
        assert_eq!(languages("ca_ES@valencia"), ["ca_ES", "ca"]);
        assert!(languages("C.UTF-8").is_empty());
        assert!(languages("").is_empty());
    }

    #[test]
    fn an_application_is_found_by_its_identifier_whatever_the_case() {
        let catalog = Catalog::new(vec![app("firefox", "/usr/lib/firefox/firefox", None)]);

        assert_eq!(catalog.find("firefox").unwrap().id, "firefox");
        assert_eq!(catalog.find("Firefox").unwrap().id, "firefox");
        assert!(catalog.find("chromium").is_none());
        assert!(catalog.find("").is_none());
    }

    #[test]
    fn a_window_finds_its_application_by_class_short_name_or_program() {
        let catalog = Catalog::new(vec![
            app("code-oss", "/usr/bin/code-oss --flag", Some("Code")),
            app("org.gnome.Nautilus", "nautilus --new-window", None),
            app(
                "com.discordapp.Discord",
                "flatpak run discord",
                Some("discord"),
            ),
            app("kitty-open", "kitty +open", None),
        ]);

        assert_eq!(catalog.find("code").unwrap().id, "code-oss");
        assert_eq!(catalog.find("nautilus").unwrap().id, "org.gnome.Nautilus");
        assert_eq!(
            catalog.find("discord").unwrap().id,
            "com.discordapp.Discord"
        );
        assert_eq!(catalog.find("kitty").unwrap().id, "kitty-open");
    }

    #[test]
    fn an_exact_identifier_wins_over_a_looser_match() {
        let catalog = Catalog::new(vec![
            app("kitty", "kitty", None),
            app("kitty-open", "kitty +open", None),
        ]);

        assert_eq!(catalog.find("kitty").unwrap().id, "kitty");
    }
}
