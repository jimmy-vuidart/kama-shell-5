//! The drawer: sorting, pages, search, where an application will open, and
//! how the arrow keys walk through what is shown.

use coquille_ipc::Habit;
use coquille_model::OutputRole;

use super::Identity;

/// The grid of applications: six columns of three rows per page.
pub const COLUMNS: usize = 6;
pub const ROWS: usize = 3;
pub const PAGE: usize = COLUMNS * ROWS;

/// The width of the drawer unless the user gave it another one.
pub const DEFAULT_WIDTH: u32 = 640;
/// Narrower, the control center no longer fits.
pub const MIN_WIDTH: u32 = 520;
/// What the mixer takes beside the drawer, margins included: the drawer
/// never grows into it.
pub const MIXER_ROOM: u32 = 320;

/// The width the drawer takes when `wanted` is asked for (the width kept in
/// the configuration, or where the user drags its edge) on a screen of a
/// given width: never narrower than its content, never so wide that the
/// mixer would not fit beside it.
pub fn width(wanted: Option<f32>, screen: Option<f32>) -> u32 {
    let wanted = wanted.filter(|wanted| wanted.is_finite());
    let Some(wanted) = wanted else {
        return DEFAULT_WIDTH;
    };
    let widest = match screen.filter(|screen| screen.is_finite()) {
        Some(screen) => (screen.round() as i64 - i64::from(MIXER_ROOM)).max(MIN_WIDTH.into()),
        None => i64::from(u16::MAX),
    };
    let width = (wanted.round() as i64).clamp(MIN_WIDTH.into(), widest);
    u32::try_from(width).unwrap_or(DEFAULT_WIDTH)
}

/// A text as it is compared: without case nor accents, so that "editeur"
/// finds "Éditeur".
pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for character in text.chars().flat_map(char::to_lowercase) {
        let plain = match character {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
            'ç' => "c",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ñ' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'ý' | 'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            'ß' => "ss",
            other => {
                folded.push(other);
                continue;
            }
        };
        folded.push_str(plain);
    }
    folded
}

/// From A to Z, whatever the case and the accents.
pub fn sort_by_name<T>(items: &mut [T], name: impl Fn(&T) -> &str) {
    items.sort_by(|a, b| {
        let folded = fold(name(a)).cmp(&fold(name(b)));
        folded.then_with(|| name(a).cmp(name(b)))
    });
}

/// How many pages `count` applications take; an empty drawer still has one.
pub fn page_count(count: usize) -> usize {
    count.div_ceil(PAGE).max(1)
}

/// The applications of a page, numbered from 0. A page past the last one
/// is the last one.
pub fn page<T>(items: &[T], page: usize) -> &[T] {
    let page = page.min(page_count(items.len()) - 1);
    let start = page * PAGE;
    &items[start.min(items.len())..(start + PAGE).min(items.len())]
}

/// The page shown after a notch of the wheel, or a swipe.
pub fn turn_page(current: usize, count: usize, forward: bool) -> usize {
    let last = page_count(count) - 1;
    if forward {
        (current + 1).min(last)
    } else {
        current.min(last).saturating_sub(1)
    }
}

/// The line under the title of the grid: "47 · de A à Z · page 1 sur 3".
pub fn summary(count: usize, page: usize) -> String {
    let pages = page_count(count);
    let page = page.min(pages - 1) + 1;
    format!("{count} · de A à Z · page {page} sur {pages}")
}

/// What matches a search, best first: the names that start with it, then
/// those with a word that does, then those that contain it. Within each
/// group the order of `items` is kept.
pub fn search<'a, T>(items: &'a [T], name: impl Fn(&T) -> &str, query: &str) -> Vec<&'a T> {
    let query = fold(query.trim());
    if query.is_empty() {
        return Vec::new();
    }
    let rank = |item: &T| {
        let folded = fold(name(item));
        let word_start = || {
            let breaks = |character: char| !character.is_alphanumeric();
            folded.split(breaks).any(|word| word.starts_with(&query))
        };
        if folded.starts_with(&query) {
            Some(0)
        } else if word_start() {
            Some(1)
        } else if folded.contains(&query) {
            Some(2)
        } else {
            None
        }
    };
    let mut hits: Vec<(u8, &T)> = items
        .iter()
        .filter_map(|item| Some((rank(item)?, item)))
        .collect();
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, item)| item).collect()
}

/// Where an application opens when started from the drawer: what the label
/// under a pinned icon says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// On this screen: in the slot the user tidied it in, or, for a game,
    /// at the center.
    Screen(OutputRole),
    /// Floating, on the screen in use.
    Floating,
}

impl Place {
    pub fn label(self) -> &'static str {
        match self {
            Self::Screen(OutputRole::Left) => "Gauche",
            Self::Screen(OutputRole::Center) => "Centre",
            Self::Screen(OutputRole::Right) => "Droite",
            Self::Floating => "Flottant",
        }
    }
}

/// Games always open at the center; an application the user tidied reopens
/// on its screen; anything else floats.
pub fn place(identity: &Identity, habits: &[Habit]) -> Place {
    if identity.game {
        return Place::Screen(OutputRole::Center);
    }
    let named = |app: &str| {
        identity
            .names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(app))
    };
    let habit = habits.iter().find(|habit| named(&habit.app));
    habit.map_or(Place::Floating, |habit| Place::Screen(habit.screen))
}

/// An arrow key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// The item selected after an arrow key, among `count` items laid out in
/// rows of `columns`. The first key press selects the first item; the
/// selection stops at the edges.
pub fn step(selected: Option<usize>, count: usize, columns: usize, step: Step) -> Option<usize> {
    if count == 0 || columns == 0 {
        return None;
    }
    let Some(current) = selected.filter(|index| *index < count) else {
        return Some(0);
    };
    let last = count - 1;
    Some(match step {
        Step::Left => current.saturating_sub(1),
        Step::Right => (current + 1).min(last),
        Step::Up if current >= columns => current - columns,
        Step::Up => current,
        // Under a full row, a shorter last row is still reachable.
        Step::Down if current / columns < last / columns => (current + columns).min(last),
        Step::Down => current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("app {index:02}")).collect()
    }

    fn identity(name: &str, game: bool) -> Identity {
        Identity {
            key: name.into(),
            name: name.into(),
            icon: None,
            cover: None,
            command: None,
            names: vec![name.into(), format!("{name}-bin")],
            game,
        }
    }

    #[test]
    fn the_drawer_is_640_wide_unless_told_otherwise() {
        assert_eq!(width(None, Some(1920.0)), 640);
        assert_eq!(width(Some(800.0), Some(1920.0)), 800);
    }

    #[test]
    fn the_drawer_cannot_be_dragged_narrower_than_its_content() {
        assert_eq!(width(Some(120.0), Some(1920.0)), MIN_WIDTH);
        assert_eq!(width(Some(-40.0), None), MIN_WIDTH);
    }

    #[test]
    fn a_wide_drawer_leaves_room_for_the_mixer_beside_it() {
        assert_eq!(width(Some(5000.0), Some(1920.0)), 1920 - MIXER_ROOM);
        // Dragged to a whole number of pixels.
        assert_eq!(width(Some(700.6), Some(1920.0)), 701);
    }

    #[test]
    fn on_a_narrow_screen_the_drawer_keeps_its_smallest_width() {
        assert_eq!(width(Some(900.0), Some(700.0)), MIN_WIDTH);
    }

    #[test]
    fn a_width_that_is_not_a_number_gives_the_usual_one() {
        assert_eq!(width(Some(f32::NAN), Some(1920.0)), DEFAULT_WIDTH);
    }

    #[test]
    fn folding_drops_case_and_accents() {
        assert_eq!(fold("Éditeur de code"), "editeur de code");
        assert_eq!(fold("ÇA Où Œuvre"), "ca ou oeuvre");
        assert_eq!(fold("Straße"), "strasse");
    }

    #[test]
    fn applications_are_sorted_as_a_dictionary_would() {
        let mut apps = vec!["navigateur", "Éditeur", "agenda", "Zoom", "editeur"];

        sort_by_name(&mut apps, |name| name);

        assert_eq!(apps, ["agenda", "editeur", "Éditeur", "navigateur", "Zoom"]);
    }

    #[test]
    fn forty_seven_applications_take_three_pages() {
        let apps = names(47);

        assert_eq!(page_count(apps.len()), 3);
        assert_eq!(page(&apps, 0).len(), 18);
        assert_eq!(page(&apps, 1).first().unwrap(), "app 18");
        assert_eq!(page(&apps, 2).len(), 11);
        assert_eq!(page(&apps, 2).last().unwrap(), "app 46");
    }

    #[test]
    fn a_full_last_page_does_not_add_an_empty_one() {
        assert_eq!(page_count(36), 2);
        assert_eq!(page_count(37), 3);
    }

    #[test]
    fn an_empty_drawer_has_one_empty_page() {
        let apps = names(0);

        assert_eq!(page_count(0), 1);
        assert!(page(&apps, 0).is_empty());
    }

    #[test]
    fn a_page_past_the_end_is_the_last_one() {
        let apps = names(20);

        assert_eq!(page(&apps, 7), ["app 18", "app 19"]);
    }

    #[test]
    fn the_wheel_turns_pages_and_stops_at_both_ends() {
        assert_eq!(turn_page(0, 47, true), 1);
        assert_eq!(turn_page(2, 47, true), 2);
        assert_eq!(turn_page(1, 47, false), 0);
        assert_eq!(turn_page(0, 47, false), 0);
        assert_eq!(turn_page(9, 47, false), 1);
    }

    #[test]
    fn the_summary_counts_applications_and_pages() {
        assert_eq!(summary(47, 0), "47 · de A à Z · page 1 sur 3");
        assert_eq!(summary(47, 2), "47 · de A à Z · page 3 sur 3");
        assert_eq!(summary(0, 0), "0 · de A à Z · page 1 sur 1");
    }

    #[test]
    fn searching_ignores_accents_on_both_sides() {
        let apps = ["Éditeur de code", "Agenda", "Lecteur PDF"];

        assert_eq!(search(&apps, |name| name, "editeur"), [&"Éditeur de code"]);
        assert_eq!(search(&apps, |name| name, "ÉDIT"), [&"Éditeur de code"]);
    }

    #[test]
    fn names_that_start_with_the_search_come_first() {
        let apps = ["Capture d'écran", "Éditeur de code", "Code", "Encodeur"];

        let hits = search(&apps, |name| name, "cod");

        assert_eq!(hits, [&"Code", &"Éditeur de code", &"Encodeur"]);
    }

    #[test]
    fn an_empty_search_finds_nothing_and_an_unknown_word_neither() {
        let apps = ["Agenda", "Archives"];

        assert!(search(&apps, |name| name, "   ").is_empty());
        assert!(search(&apps, |name| name, "zzz").is_empty());
    }

    #[test]
    fn a_tidied_application_opens_on_its_screen() {
        let habits = [Habit {
            app: "Firefox-BIN".into(),
            screen: OutputRole::Right,
            slot: 0,
        }];

        let place = place(&identity("firefox", false), &habits);

        assert_eq!(place, Place::Screen(OutputRole::Right));
        assert_eq!(place.label(), "Droite");
    }

    #[test]
    fn an_application_never_tidied_opens_floating() {
        let place = place(&identity("gimp", false), &[]);

        assert_eq!(place, Place::Floating);
        assert_eq!(place.label(), "Flottant");
    }

    #[test]
    fn a_game_opens_at_the_center_whatever_its_habit() {
        let habits = [Habit {
            app: "steam_app_7".into(),
            screen: OutputRole::Left,
            slot: 0,
        }];

        let place = place(&identity("steam_app_7", true), &habits);

        assert_eq!(place, Place::Screen(OutputRole::Center));
        assert_eq!(place.label(), "Centre");
    }

    #[test]
    fn the_first_arrow_selects_the_first_item() {
        assert_eq!(step(None, 18, COLUMNS, Step::Down), Some(0));
        assert_eq!(step(Some(40), 18, COLUMNS, Step::Left), Some(0));
        assert_eq!(step(None, 0, COLUMNS, Step::Right), None);
    }

    #[test]
    fn arrows_move_in_the_grid_and_stop_at_its_edges() {
        assert_eq!(step(Some(0), 18, COLUMNS, Step::Right), Some(1));
        assert_eq!(step(Some(0), 18, COLUMNS, Step::Left), Some(0));
        assert_eq!(step(Some(1), 18, COLUMNS, Step::Down), Some(7));
        assert_eq!(step(Some(7), 18, COLUMNS, Step::Up), Some(1));
        assert_eq!(step(Some(1), 18, COLUMNS, Step::Up), Some(1));
        assert_eq!(step(Some(17), 18, COLUMNS, Step::Right), Some(17));
        assert_eq!(step(Some(13), 18, COLUMNS, Step::Down), Some(13));
    }

    #[test]
    fn going_down_reaches_a_shorter_last_row() {
        // Eleven items: a full row of six, then five.
        assert_eq!(step(Some(5), 11, COLUMNS, Step::Down), Some(10));
        assert_eq!(step(Some(2), 11, COLUMNS, Step::Down), Some(8));
        assert_eq!(step(Some(8), 11, COLUMNS, Step::Down), Some(8));
    }
}
