//! Keeps the per-application memory between sessions, in
//! `~/.local/state/coquille/memory.json`.

use std::io;
use std::path::{Path, PathBuf};

use coquille_model::AppMemory;
use tracing::warn;

const FILE: &str = "memory.json";

/// Where state is kept when no directory is given.
pub fn default_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))?;
    Some(base.join("coquille"))
}

/// Reads the habits saved in `directory`. No file yet means no habit; an
/// unreadable one is reported and ignored, as habits are not worth failing
/// to start for.
pub fn load(directory: &Path) -> AppMemory {
    let path = directory.join(FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AppMemory::default();
    };
    serde_json::from_str(&text).unwrap_or_else(|error| {
        warn!(path = %path.display(), %error, "unreadable habits, starting without");
        AppMemory::default()
    })
}

/// Writes the habits, replacing the file in one step so that a crash in the
/// middle cannot leave half of it.
pub fn save(directory: &Path, memory: &AppMemory) -> io::Result<()> {
    std::fs::create_dir_all(directory)?;
    let text = serde_json::to_string_pretty(memory)?;
    let draft = directory.join(format!("{FILE}.tmp"));
    std::fs::write(&draft, text)?;
    std::fs::rename(draft, directory.join(FILE))
}

#[cfg(test)]
mod tests {
    use coquille_model::{AppKey, OutputRole, RememberedPlace};

    use super::*;

    /// A fresh directory under the system's temporary one.
    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("coquille-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    fn habits() -> AppMemory {
        let mut memory = AppMemory::default();
        memory.remember(
            AppKey::new("discord"),
            RememberedPlace::in_slot(OutputRole::Left, 0),
        );
        memory
    }

    #[test]
    fn saved_habits_are_read_back() {
        let directory = scratch("round-trip");

        save(&directory, &habits()).unwrap();

        assert_eq!(load(&directory), habits());
    }

    #[test]
    fn saving_creates_the_directory() {
        let directory = scratch("nested").join("deeper");

        save(&directory, &habits()).unwrap();

        assert!(directory.join(FILE).is_file());
    }

    #[test]
    fn saving_again_replaces_the_file() {
        let directory = scratch("replace");
        save(&directory, &habits()).unwrap();

        save(&directory, &AppMemory::default()).unwrap();

        assert!(load(&directory).is_empty());
    }

    #[test]
    fn without_a_file_there_is_no_habit() {
        assert!(load(&scratch("missing")).is_empty());
    }

    #[test]
    fn an_unreadable_file_is_ignored() {
        let directory = scratch("garbage");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(FILE), "not json").unwrap();

        assert!(load(&directory).is_empty());
    }
}
