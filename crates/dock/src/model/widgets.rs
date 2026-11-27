//! Pure data structures, mock widgets, and persistent game notes for the
//! spatial launcher.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::steam::{Game, format_last_played};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyTask {
    pub id: String,
    pub title: String,
    pub meta: String,
    pub done: bool,
    /// Remote tasks are read-only while only `GET` is available.
    pub read_only: bool,
}

/// Loading state of the « Vie quotidienne » tasks card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DailyTasksStatus {
    /// No API token configured in `[spatial] tasks_token`.
    MissingToken,
    /// Initial fetch in flight.
    Loading,
    /// Loaded from the API (or mock defaults).
    Ready,
    /// Last fetch failed with a short French explanation.
    Error(String),
}

/// Loading state of the « Tâches des projets » card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTasksStatus {
    /// No API key configured in `[spatial] foundry_token`.
    MissingToken,
    /// Initial fetch in flight.
    Loading,
    /// Loaded from Kama-Foundry (or mock defaults).
    Ready,
    /// Last fetch failed with a short French explanation.
    Error(String),
}

/// Ticket status in Kama-Foundry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectStatus {
    WaitingHuman,
    Failed,
    InProgress,
    Review,
    Queued,
    Todo,
    Backlog,
    Done,
    Other(String),
}

impl ProjectStatus {
    pub fn from_api(value: &str) -> Self {
        match value {
            "waiting_human" => Self::WaitingHuman,
            "failed" => Self::Failed,
            "in_progress" => Self::InProgress,
            "review" => Self::Review,
            "queued" => Self::Queued,
            "todo" => Self::Todo,
            "backlog" => Self::Backlog,
            "done" => Self::Done,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::WaitingHuman => "Attente",
            Self::Failed => "Échec",
            Self::InProgress => "En cours",
            Self::Review => "Revue",
            Self::Queued => "En file",
            Self::Todo => "À faire",
            Self::Backlog => "Backlog",
            Self::Done => "Terminé",
            Self::Other(label) => label.as_str(),
        }
    }

    /// Priority order for the dashboard card (attention-needing items first).
    pub fn sort_rank(&self) -> u8 {
        match self {
            Self::WaitingHuman => 0,
            Self::Failed => 1,
            Self::InProgress => 2,
            Self::Review => 3,
            Self::Queued => 4,
            Self::Todo => 5,
            Self::Backlog => 6,
            Self::Other(_) => 7,
            Self::Done => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTask {
    pub id: String,
    pub project: String,
    pub title: String,
    pub status: ProjectStatus,
    pub assigned_agent: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: String,
    pub day_offset: i64,
    pub time: String,
    pub title: String,
    pub accent_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedGame {
    pub key: String,
    pub steam_id: Option<u64>,
    pub title: String,
    pub note: String,
    pub meta_pill: String,
    pub cover: Option<PathBuf>,
    pub command: Option<String>,
}

/// Persistent user notes for games, stored in `game_notes.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameNotes {
    notes: HashMap<String, String>,
}

impl GameNotes {
    pub fn load(state_dir: &Path) -> Self {
        let path = state_dir.join("game_notes.json");
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self, state_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("game_notes.json");
        let tmp = state_dir.join(format!("game_notes.{}.tmp", std::process::id()));
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.notes.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: String, note: String) {
        self.notes.insert(key, note.trim().to_owned());
    }
}

/// Widget state for the spatial launcher: daily tasks, project tickets,
/// 7-day agenda, and editable game notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetsState {
    pub daily_tasks: Vec<DailyTask>,
    pub daily_tasks_status: DailyTasksStatus,
    pub project_tasks: Vec<ProjectTask>,
    pub project_tasks_status: ProjectTasksStatus,
    pub calendar_events: Vec<CalendarEvent>,
    pub game_notes: GameNotes,
}

impl WidgetsState {
    pub fn mock_default(notes: GameNotes) -> Self {
        Self {
            daily_tasks: vec![
                DailyTask {
                    id: "daily_1".into(),
                    title: "Faire les courses (légumes, café)".into(),
                    meta: "Avant 18 h".into(),
                    done: false,
                    read_only: false,
                },
                DailyTask {
                    id: "daily_2".into(),
                    title: "Sortir les poubelles jaunes".into(),
                    meta: "Ce soir".into(),
                    done: false,
                    read_only: false,
                },
                DailyTask {
                    id: "daily_3".into(),
                    title: "Appeler le dentiste pour décembre".into(),
                    meta: "Matin".into(),
                    done: true,
                    read_only: false,
                },
                DailyTask {
                    id: "daily_4".into(),
                    title: "Arroser les plantes du bureau".into(),
                    meta: "Fait".into(),
                    done: true,
                    read_only: false,
                },
            ],
            daily_tasks_status: DailyTasksStatus::Ready,
            project_tasks: vec![
                ProjectTask {
                    id: "proj_1".into(),
                    project: "coquille".into(),
                    title: "Finaliser le mode spatial du dock".into(),
                    status: ProjectStatus::InProgress,
                    assigned_agent: Some("antigravity".into()),
                    url: None,
                },
                ProjectTask {
                    id: "proj_2".into(),
                    project: "coquille".into(),
                    title: "Tester le flou Dual Kawase sur portable Intel".into(),
                    status: ProjectStatus::Todo,
                    assigned_agent: None,
                    url: None,
                },
                ProjectTask {
                    id: "proj_3".into(),
                    project: "atelier".into(),
                    title: "Mettre à jour les dépendances Rust".into(),
                    status: ProjectStatus::Review,
                    assigned_agent: Some("claude".into()),
                    url: None,
                },
                ProjectTask {
                    id: "proj_4".into(),
                    project: "notes".into(),
                    title: "Rédiger le bilan de la semaine".into(),
                    status: ProjectStatus::Backlog,
                    assigned_agent: None,
                    url: None,
                },
            ],
            project_tasks_status: ProjectTasksStatus::Ready,
            calendar_events: vec![
                CalendarEvent {
                    id: "cal_1".into(),
                    day_offset: 0,
                    time: "14:00".into(),
                    title: "Point synchro coquille".into(),
                    accent_hex: "#5b63e6".into(),
                },
                CalendarEvent {
                    id: "cal_2".into(),
                    day_offset: 0,
                    time: "20:30".into(),
                    title: "Raid MMO avec la guilde".into(),
                    accent_hex: "#e07a5f".into(),
                },
                CalendarEvent {
                    id: "cal_3".into(),
                    day_offset: 1,
                    time: "10:00".into(),
                    title: "Revue d'architecture Wayland".into(),
                    accent_hex: "#3d9970".into(),
                },
                CalendarEvent {
                    id: "cal_4".into(),
                    day_offset: 2,
                    time: "16:30".into(),
                    title: "Livraison colis".into(),
                    accent_hex: "#d99b26".into(),
                },
                CalendarEvent {
                    id: "cal_5".into(),
                    day_offset: 4,
                    time: "11:00".into(),
                    title: "Déjeuner d'équipe".into(),
                    accent_hex: "#5b63e6".into(),
                },
                CalendarEvent {
                    id: "cal_6".into(),
                    day_offset: 5,
                    time: "18:00".into(),
                    title: "Session Factorio coop".into(),
                    accent_hex: "#3d9970".into(),
                },
            ],
            game_notes: notes,
        }
    }

    pub fn toggle_daily_task(&mut self, id: &str) {
        if let Some(task) = self.daily_tasks.iter_mut().find(|task| task.id == id)
            && !task.read_only
        {
            task.done = !task.done;
        }
    }

    pub fn remaining_daily_tasks(&self) -> usize {
        self.daily_tasks.iter().filter(|task| !task.done).count()
    }

    /// Returns up to 6 started games from Steam (preferring recently played),
    /// or the 6 prototype mock games when `steam_games` is empty and
    /// `use_mock_fallback` is `true`.
    pub fn started_games(
        &self,
        steam_games: &[Game],
        playtimes: &HashMap<u64, u32>,
        now_secs: i64,
        use_mock_fallback: bool,
    ) -> Vec<StartedGame> {
        const MAX_GAMES: usize = 24;
        if steam_games.is_empty() {
            if !use_mock_fallback {
                return Vec::new();
            }
            return mock_games(&self.game_notes);
        }

        let mut selected: Vec<&Game> = steam_games
            .iter()
            .filter(|game| game.last_played > 0)
            .take(MAX_GAMES)
            .collect();
        if selected.len() < MAX_GAMES {
            for game in steam_games.iter().filter(|game| game.last_played == 0) {
                if selected.len() >= MAX_GAMES {
                    break;
                }
                selected.push(game);
            }
        }

        selected
            .into_iter()
            .map(|game| {
                let key = format!("steam_{}", game.id);
                let note = self.game_notes.get(&key).unwrap_or("").to_owned();
                let mins = playtimes.get(&game.id).copied().unwrap_or(0);
                StartedGame {
                    key,
                    steam_id: Some(game.id),
                    title: game.name.clone(),
                    note,
                    meta_pill: format_last_played(game.last_played, mins, now_secs),
                    cover: game.cover.clone(),
                    command: Some(game.command()),
                }
            })
            .collect()
    }
}

fn mock_games(notes: &GameNotes) -> Vec<StartedGame> {
    let defaults = [
        (
            "ffxiv",
            Some(39_210),
            "Final Fantasy XIV",
            "Finir les quêtes de Dawntrail et préparer le raid",
            "Hier · 412 h",
        ),
        (
            "factorio",
            Some(427_520),
            "Factorio",
            "Automatiser les circuits bleus sur Gleba",
            "Il y a 2 j · 142 h",
        ),
        (
            "gw2",
            Some(1_284_210),
            "Guild Wars 2",
            "Terminer les succès d'exploration de Janthir",
            "Il y a 5 j · 89 h",
        ),
        (
            "satisfactory",
            Some(526_870),
            "Satisfactory",
            "Raccorder la centrale nucléaire du nord",
            "Il y a 6 j · 215 h",
        ),
        (
            "hades2",
            Some(1_145_350),
            "Hades II",
            "Essayer le build bâton sur l'Olympe",
            "Il y a 9 j · 48 h",
        ),
        (
            "balatro",
            Some(2_379_780),
            "Balatro",
            "Débloquer le deck plasma en mise or",
            "Il y a 12 j · 64 h",
        ),
    ];

    defaults
        .into_iter()
        .map(|(key, steam_id, title, default_note, pill)| StartedGame {
            key: key.to_owned(),
            steam_id,
            title: title.to_owned(),
            note: notes.get(key).unwrap_or(default_note).to_owned(),
            meta_pill: pill.to_owned(),
            cover: None,
            command: steam_id.map(|id| format!("steam steam://rungameid/{id}")),
        })
        .collect()
}

/// Formats a day offset relative to `today` in French (`"Aujourd'hui"`,
/// `"Demain"`, or e.g. `"sam 29 nov"`).
pub fn day_label(day_offset: i64, today: NaiveDate) -> String {
    match day_offset {
        0 => "Aujourd'hui".to_owned(),
        1 => "Demain".to_owned(),
        _ => {
            let date = today + Duration::days(day_offset);
            format!(
                "{} {} {}",
                french_weekday(date.weekday()),
                date.day(),
                french_month(date.month())
            )
        }
    }
}

/// Formats the 7-day range starting at `today` (e.g. `"27 nov — 3 déc"`).
pub fn range_label(today: NaiveDate) -> String {
    let end = today + Duration::days(6);
    format!(
        "{} {} — {} {}",
        today.day(),
        french_month(today.month()),
        end.day(),
        french_month(end.month())
    )
}

fn french_weekday(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "lun",
        Weekday::Tue => "mar",
        Weekday::Wed => "mer",
        Weekday::Thu => "jeu",
        Weekday::Fri => "ven",
        Weekday::Sat => "sam",
        Weekday::Sun => "dim",
    }
}

fn french_month(month: u32) -> &'static str {
    match month {
        1 => "jan",
        2 => "fév",
        3 => "mars",
        4 => "avr",
        5 => "mai",
        6 => "juin",
        7 => "juil",
        8 => "août",
        9 => "sep",
        10 => "oct",
        11 => "nov",
        12 => "déc",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "coquille_dock_test_{name}_{}_{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn game_notes_roundtrip_and_clear() {
        let dir = unique_temp_dir("notes");
        assert_eq!(GameNotes::load(&dir), GameNotes::default());

        let mut notes = GameNotes::default();
        notes.set("steam_427520".into(), "  Faire les sciences  ".into());
        notes.set("ffxiv".into(), "".into());
        notes.save(&dir).expect("save notes");

        let loaded = GameNotes::load(&dir);
        assert_eq!(loaded.get("steam_427520"), Some("Faire les sciences"));
        assert_eq!(loaded.get("ffxiv"), Some(""));
        assert_eq!(loaded.get("unknown"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toggle_daily_task_updates_remaining_count() {
        let mut state = WidgetsState::mock_default(GameNotes::default());
        assert_eq!(state.remaining_daily_tasks(), 2);

        state.toggle_daily_task("daily_1");
        assert_eq!(state.remaining_daily_tasks(), 1);

        state.toggle_daily_task("daily_1");
        assert_eq!(state.remaining_daily_tasks(), 2);
    }

    #[test]
    fn started_games_prefers_steam_and_applies_custom_notes() {
        let mut notes = GameNotes::default();
        notes.set("steam_42".into(), "Finir le chapitre 3".into());
        notes.set("ffxiv".into(), "Note personnalisée".into());
        let state = WidgetsState::mock_default(notes);

        let now = 1_732_700_000i64;
        let steam_games = vec![
            Game {
                id: 42,
                name: "Hades".into(),
                last_played: (now - 90_000) as u64,
                cover: None,
            },
            Game {
                id: 99,
                name: "Celeste".into(),
                last_played: 0,
                cover: None,
            },
        ];
        let mut playtimes = HashMap::new();
        playtimes.insert(42, 180);

        let games = state.started_games(&steam_games, &playtimes, now, true);
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].key, "steam_42");
        assert_eq!(games[0].note, "Finir le chapitre 3");
        assert_eq!(games[0].meta_pill, "Hier · 3 h");
        assert_eq!(games[1].key, "steam_99");
        assert_eq!(games[1].meta_pill, "Installé");

        let mock = state.started_games(&[], &playtimes, now, true);
        assert_eq!(mock.len(), 6);
        assert_eq!(mock[0].key, "ffxiv");
        assert_eq!(mock[0].note, "Note personnalisée");

        let empty = state.started_games(&[], &playtimes, now, false);
        assert!(empty.is_empty());
    }

    #[test]
    fn day_and_range_labels_format_in_french() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        assert_eq!(day_label(0, today), "Aujourd'hui");
        assert_eq!(day_label(1, today), "Demain");
        assert_eq!(day_label(2, today), "dim 29 nov");
        assert_eq!(day_label(5, today), "mer 2 déc");
        assert_eq!(range_label(today), "27 nov — 3 déc");
    }
}
