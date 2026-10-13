//! Changing one setting of the file without touching the rest of it.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

/// Where the configuration is, unless a path is given.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
    Some(base.join("coquille/config.toml"))
}

/// The file with another value for a setting. `path` names it from the top:
/// `["theme"]`, `["game", "vrr"]`, `["dock", "screens", "left"]`. A section
/// the file does not have is added at its end.
///
/// A setting the file only suggests, in a comment (`# vrr = true`), is
/// switched on where it stands, under its explanation.
///
/// `None` when the file is not valid TOML: it is then left alone.
pub fn with_value(text: &str, path: &[&str], new: Value) -> Option<String> {
    let mut document = text.parse::<DocumentMut>().ok()?;
    let (key, sections) = path.split_last()?;
    let suggested = !has(&document, path);
    let mut table = document.as_table_mut();
    for section in sections {
        let inside = table.entry(section).or_insert_with(|| {
            // Without a line of its own while it only holds other sections.
            let mut section = Table::new();
            section.set_implicit(true);
            Item::Table(section)
        });
        table = inside.as_table_mut()?;
    }
    set(table, key, new.clone());
    let changed = document.to_string();
    if suggested {
        let tidier = switched_on(text, path, &new);
        if let Some(tidier) = tidier.filter(|tidier| means_the_same(tidier, &changed)) {
            return Some(tidier);
        }
    }
    Some(changed)
}

/// The file without a setting, which then takes its default value. `None`
/// when the file is not valid TOML.
pub fn without(text: &str, path: &[&str]) -> Option<String> {
    let mut document = text.parse::<DocumentMut>().ok()?;
    let (key, sections) = path.split_last()?;
    let mut table = Some(document.as_table_mut());
    for section in sections {
        table = table
            .and_then(|table| table.get_mut(section))
            .and_then(Item::as_table_mut);
    }
    match table.and_then(|table| table.remove(key)) {
        Some(_) => Some(document.to_string()),
        None => Some(text.to_owned()),
    }
}

fn has(document: &DocumentMut, path: &[&str]) -> bool {
    let mut item = Some(document.as_item());
    for name in path {
        item = item.and_then(|item| item.get(name));
    }
    item.is_some()
}

/// Gives a setting another value. The comments written above it and at the
/// end of its line stay where they are.
fn set(table: &mut Table, key: &str, mut new: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

/// The name of the section a line opens, when it is the title of one.
fn title(line: &str) -> Option<&str> {
    let name = line.trim().strip_prefix('[')?.strip_suffix(']')?;
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
    (!name.is_empty() && name.chars().all(plain)).then_some(name)
}

/// What a comment suggests, without its `#`.
fn suggestion(line: &str) -> Option<&str> {
    let comment = line.trim_start().strip_prefix('#')?;
    Some(comment.strip_prefix(' ').unwrap_or(comment))
}

/// The file with the suggestion of a setting (`# name = value`, under its
/// section, itself suggested or not) turned into the setting. `None` when
/// the file suggests nothing of the kind.
fn switched_on(text: &str, path: &[&str], new: &Value) -> Option<String> {
    let (key, sections) = path.split_last()?;
    let wanted = sections.join(".");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    // The section each line is in, and the line of its title when that
    // title is only suggested.
    let mut section = String::new();
    let mut suggested_title = None;
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(name) = title(line) {
            section = name.to_owned();
            suggested_title = None;
        } else if let Some(name) = suggestion(line).and_then(title) {
            section = name.to_owned();
            suggested_title = Some(index);
        } else if section == wanted {
            let named = suggestion(line)
                .and_then(|setting| setting.strip_prefix(key))
                .is_some_and(|rest| rest.trim_start().starts_with('='));
            if named {
                found = Some((index, suggested_title));
                break;
            }
        }
    }
    let (index, suggested_title) = found?;
    lines[index] = format!("{key} = {}", new.to_string().trim());
    if let Some(index) = suggested_title {
        lines[index] = suggestion(&lines[index])?.to_owned();
    }
    let mut changed = lines.join("\n");
    if text.ends_with('\n') {
        changed.push('\n');
    }
    Some(changed)
}

/// Whether two files say the same thing, whatever their comments and the
/// order of their lines.
fn means_the_same(one: &str, other: &str) -> bool {
    let meaning = |text: &str| {
        let document = text.parse::<DocumentMut>().ok()?;
        let mut settings = Vec::new();
        flatten(document.as_item(), String::new(), &mut settings);
        settings.sort();
        Some(settings)
    };
    let (one, other) = (meaning(one), meaning(other));
    one.is_some() && one == other
}

/// Every setting under `item`, with its full name and its value as written.
fn flatten(item: &Item, name: String, settings: &mut Vec<(String, String)>) {
    match item {
        Item::Table(table) => {
            for (key, inside) in table.iter() {
                flatten(inside, format!("{name}.{key}"), settings);
            }
        }
        Item::ArrayOfTables(tables) => {
            for (index, table) in tables.iter().enumerate() {
                for (key, inside) in table.iter() {
                    flatten(inside, format!("{name}.{index}.{key}"), settings);
                }
            }
        }
        Item::Value(value) => {
            let mut value = value.clone();
            value.decor_mut().clear();
            settings.push((name, value.to_string()));
        }
        Item::None => {}
    }
}

/// Rewrites the file through `change`, which gets its text and returns the
/// new one. A file that cannot be read as TOML is not touched.
pub fn update(path: &Path, change: impl Fn(&str) -> Option<String>) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let Some(changed) = change(&text) else {
        return Err(std::io::Error::other("the configuration is not valid TOML"));
    };
    if changed == text {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    // Written next to the file, then moved over it: never half a file.
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, changed)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"# Ma configuration.
theme = "dark"          # sombre le soir

[outputs]
left = "HDMI-A-1"

[game]
# Bords collants, en millisecondes.
# sticky_edges_ms = 300

# Fréquence variable.
# vrr = true

[dock]
# Mes applis de tous les jours.
favorites = ["firefox", "discord"]
"##;

    fn value_of(text: &str, path: &[&str]) -> Option<String> {
        let document = text.parse::<DocumentMut>().ok()?;
        let mut item = Some(document.as_item());
        for name in path {
            item = item.and_then(|item| item.get(name));
        }
        let mut value = item?.as_value()?.clone();
        value.decor_mut().clear();
        Some(value.to_string())
    }

    #[test]
    fn a_value_is_replaced_where_it_stands_with_its_comment() {
        let changed = with_value(FILE, &["theme"], Value::from("light")).unwrap();

        assert!(
            changed.starts_with("# Ma configuration.\ntheme = \"light\"          # sombre le soir")
        );
        assert_eq!(changed.replace("light", "dark"), FILE);
    }

    #[test]
    fn a_setting_is_added_to_its_section() {
        let changed = with_value(FILE, &["outputs", "center"], Value::from("DP-2")).unwrap();

        assert_eq!(
            value_of(&changed, &["outputs", "center"]).unwrap(),
            "\"DP-2\""
        );
        assert_eq!(
            value_of(&changed, &["outputs", "left"]).unwrap(),
            "\"HDMI-A-1\""
        );
        assert!(changed.contains("# Mes applis de tous les jours."));
    }

    #[test]
    fn a_general_setting_is_added_before_the_sections() {
        let changed = with_value("[outputs]\nleft = \"DP-1\"\n", &["terminal"], "foot".into());

        // Written after a section, it would belong to it.
        let changed = changed.unwrap();
        assert!(changed.find("terminal").unwrap() < changed.find("[outputs]").unwrap());
        assert_eq!(value_of(&changed, &["terminal"]).unwrap(), "\"foot\"");
    }

    #[test]
    fn a_missing_section_is_added() {
        let changed = with_value("theme = \"dark\"\n", &["game", "vrr"], false.into()).unwrap();

        assert_eq!(changed, "theme = \"dark\"\n\n[game]\nvrr = false\n");
    }

    #[test]
    fn a_section_inside_another_gets_one_title() {
        let path = ["dock", "screens", "right"];

        let alone = with_value("", &path, "horizontal".into()).unwrap();
        let beside = with_value(FILE, &path, "horizontal".into()).unwrap();

        assert_eq!(alone, "[dock.screens]\nright = \"horizontal\"\n");
        assert_eq!(value_of(&beside, &path).unwrap(), "\"horizontal\"");
        assert_eq!(beside.matches("[dock]").count(), 1);
        assert!(beside.contains("favorites = [\"firefox\", \"discord\"]"));
    }

    #[test]
    fn a_suggested_setting_is_switched_on_under_its_explanation() {
        let changed = with_value(FILE, &["game", "sticky_edges_ms"], 500.into()).unwrap();

        assert!(changed.contains("# Bords collants, en millisecondes.\nsticky_edges_ms = 500\n"));
        assert!(changed.contains("# vrr = true"));
        assert!(!changed.contains("# sticky_edges_ms"));
    }

    #[test]
    fn a_suggestion_of_another_section_is_left_alone() {
        let file = "# vrr = true\n\n[game]\n# Rien ici.\n";

        let changed = with_value(file, &["game", "vrr"], false.into()).unwrap();

        assert!(changed.starts_with("# vrr = true\n"));
        assert_eq!(value_of(&changed, &["game", "vrr"]).unwrap(), "false");
        assert_eq!(value_of(&changed, &["vrr"]), None);
    }

    #[test]
    fn a_suggested_section_is_switched_on_with_its_setting() {
        let file = "# Disposition du clavier.\n# [keyboard]\n# layout = \"fr\"\n# variant = \"\"\n\n[game]\n";

        let changed = with_value(file, &["keyboard", "layout"], "de".into()).unwrap();

        assert_eq!(
            changed,
            "# Disposition du clavier.\n[keyboard]\nlayout = \"de\"\n# variant = \"\"\n\n[game]\n"
        );
    }

    #[test]
    fn a_suggested_section_that_would_swallow_settings_stays_a_comment() {
        // Switching the title on would move `left` into `[keyboard]`.
        let file = "[outputs]\n# [keyboard]\n# layout = \"fr\"\nleft = \"DP-1\"\n";

        let changed = with_value(file, &["keyboard", "layout"], "de".into()).unwrap();

        assert_eq!(
            value_of(&changed, &["outputs", "left"]).unwrap(),
            "\"DP-1\""
        );
        assert_eq!(
            value_of(&changed, &["keyboard", "layout"]).unwrap(),
            "\"de\""
        );
        assert!(changed.contains("# [keyboard]"));
    }

    #[test]
    fn a_setting_that_is_set_is_not_looked_for_among_the_comments() {
        let file = "# theme = \"light\"\ntheme = \"dark\"\n";

        let changed = with_value(file, &["theme"], "light".into()).unwrap();

        assert_eq!(changed, "# theme = \"light\"\ntheme = \"light\"\n");
    }

    #[test]
    fn a_setting_is_removed_and_the_rest_stays() {
        let changed = without(FILE, &["outputs", "left"]).unwrap();

        assert_eq!(value_of(&changed, &["outputs", "left"]), None);
        assert_eq!(value_of(&changed, &["theme"]).unwrap(), "\"dark\"");
        assert_eq!(without(FILE, &["terminal"]).unwrap(), FILE);
        assert_eq!(without(FILE, &["keyboard", "layout"]).unwrap(), FILE);
    }

    #[test]
    fn a_broken_file_is_never_rewritten() {
        assert_eq!(
            with_value("theme = \n[dock", &["theme"], "dark".into()),
            None
        );
        assert_eq!(without("= 3", &["theme"]), None);
    }

    #[test]
    fn the_file_is_written_whole_or_not_at_all() {
        let directory =
            std::env::temp_dir().join(format!("coquille-config-{}", std::process::id()));
        let path = directory.join("nested/config.toml");

        update(&path, |text| with_value(text, &["theme"], "dark".into())).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "theme = \"dark\"\n"
        );

        std::fs::write(&path, "theme = \n").unwrap();
        assert!(update(&path, |text| with_value(text, &["theme"], "light".into())).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theme = \n");
        let _ = std::fs::remove_dir_all(directory);
    }
}
