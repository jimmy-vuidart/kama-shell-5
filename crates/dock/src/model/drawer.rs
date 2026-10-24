//! The drawer: sorting, pages, search, where an application will open, and
//! how the arrow keys walk through what is shown.

use coquille_ipc::Habit;
use coquille_model::OutputRole;

use super::Identity;

/// The grid of applications: six columns of three rows per page, more
/// columns in a wide horizontal launcher.
pub const COLUMNS: usize = 6;
pub const ROWS: usize = 3;
/// How many rows of results a search shows.
pub const RESULT_ROWS: usize = 4;

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

/// The height of the horizontal launcher unless the user gave it another,
/// on a screen of 1080: a taller screen gives it as much more.
pub const DEFAULT_HEIGHT: u32 = 600;
const DEFAULT_SCREEN_HEIGHT: f32 = 1080.0;
/// Lower, its control center and the mixer under it no longer fit.
pub const MIN_HEIGHT: u32 = 580;
/// What stays free of the horizontal launcher on its screen: the dock under
/// it, and above it a strip of the screen, clicked to close it.
const FREE_HEIGHT: u32 = 200;

/// The height the horizontal launcher takes when `wanted` is asked for, on
/// a screen of a given height; without it, a share of the screen.
pub fn height(wanted: Option<f32>, screen: Option<f32>) -> u32 {
    let screen = screen.filter(|screen| screen.is_finite() && *screen > 0.0);
    let wanted = wanted.filter(|wanted| wanted.is_finite());
    let Some(wanted) = wanted else {
        let Some(screen) = screen else {
            return DEFAULT_HEIGHT;
        };
        let share = DEFAULT_HEIGHT as f32 * screen / DEFAULT_SCREEN_HEIGHT;
        return height(Some(share), Some(screen));
    };
    let highest = match screen {
        Some(screen) => (screen.round() as i64 - i64::from(FREE_HEIGHT)).max(MIN_HEIGHT.into()),
        None => i64::from(u16::MAX),
    };
    let height = (wanted.round() as i64).clamp(MIN_HEIGHT.into(), highest);
    u32::try_from(height).unwrap_or(DEFAULT_HEIGHT)
}

/// The margin of the horizontal launcher, and the gap between its columns.
pub const LYING_PADDING: f32 = 14.0;
pub const LYING_GAP: f32 = 14.0;
/// The columns of the horizontal launcher beside the applications.
const GAMES_COLUMN: f32 = 260.0;
const SIDE_COLUMN: f32 = 340.0;
/// The narrowest the notifications and the control center get.
const NARROWEST_SIDE: f32 = 280.0;
/// Narrower, the six columns of applications are no longer readable: the
/// games give their place first.
const NARROWEST_APPS: f32 = 540.0;
/// Up to this width (what a 16:9 screen of 1920 gives them), the
/// applications take what the screen adds; beyond, they share it equally
/// with the games.
const ROOMY_APPS: f32 = 910.0;
/// The row of columns is at most this many times as wide as the screen is
/// high: a 16:9 or a 21:9 screen is filled from edge to edge, a wider one
/// keeps it together, centered.
const WIDEST_SHAPE: f32 = 2.4;
/// The narrowest a cell of applications gets before the grid takes one
/// more column, the gap after it included.
const APP_CELL: f32 = 144.0;
/// The gap between the cells of applications.
pub const APP_GAP: f32 = 4.0;
/// Likewise for a card of games, the gap between cards included.
const GAME_CARD: f32 = 266.0;
/// The gap between the cards of games.
pub const GAME_GAP: f32 = 6.0;
/// The heights inside the horizontal launcher, fixed so that it knows how
/// many rows of applications, games and results its height holds: the
/// search field and the gap under it, the margin above the bar.
pub const SEARCH_HEIGHT: f32 = 44.0;
pub const SEARCH_GAP: f32 = 12.0;
pub const LYING_FOOT: f32 = 6.0;
/// A title and its pager, and the gap under it.
pub const HEADING_HEIGHT: f32 = 28.0;
pub const HEADING_GAP: f32 = 8.0;
/// The pinned applications, and the gap under them.
pub const PINNED_HEIGHT: f32 = 136.0;
pub const PINNED_GAP: f32 = 10.0;
/// A row of applications or of results.
pub const APP_ROW: f32 = 44.0;
/// A card of games: its picture and its name.
pub const GAME_CARD_HEIGHT: f32 = 92.0;
/// The button to the whole library, under the games.
pub const LIBRARY_HEIGHT: f32 = 26.0;

/// How the horizontal launcher, as wide as its screen, shares it out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LyingLayout {
    /// The width of the row of columns, centered on the screen.
    pub row: f32,
    pub apps: f32,
    /// None when the screen is too narrow to show the games beside.
    pub games: Option<f32>,
    pub notices: f32,
    pub control: f32,
    /// How many columns the grid of applications has.
    pub app_columns: usize,
    /// How many columns of cards the games have.
    pub game_columns: usize,
    /// How many columns the results of a search have: they take the place
    /// of the applications and the games.
    pub result_columns: usize,
    /// The height of the columns, under the search field. The control
    /// center keeps its own; the notifications take all of it.
    pub body: f32,
    /// How many rows of applications, of games and of results fit in it.
    pub app_rows: usize,
    pub game_rows: usize,
    pub result_rows: usize,
}

impl LyingLayout {
    /// How many applications a page of the grid holds.
    pub fn page(&self) -> usize {
        self.app_columns * self.app_rows
    }

    /// The width the results of a search take.
    pub fn results(&self) -> f32 {
        self.apps + self.games.map_or(0.0, |games| games + LYING_GAP)
    }

    /// How many games the column shows.
    pub fn games_shown(&self) -> usize {
        self.game_columns * self.game_rows
    }

    /// How many results a search shows.
    pub fn results_shown(&self) -> usize {
        self.result_columns * self.result_rows
    }
}

/// How many cells fit in `width`, each `cell` wide with the gap after it,
/// never fewer than `least`.
fn fitting(width: f32, cell: f32, gap: f32, least: usize) -> usize {
    let fit = ((width + gap) / cell).floor();
    (fit.max(0.0) as usize).max(least)
}

/// How many rows `row` high, `gap` apart, fit in `room`; at least one.
fn rows(room: f32, row: f32, gap: f32) -> usize {
    let fit = ((room + gap) / (row + gap)).floor();
    (fit.max(0.0) as usize).max(1)
}

/// The columns of the horizontal launcher, `panel` high, on a screen of a
/// given size: applications (under the `pinned` ones, if any), games,
/// notifications, control center. The control center keeps its size; the
/// applications and the games grow with the screen, in width and height,
/// and the notifications in height.
pub fn lying_layout(width: f32, height: f32, panel: f32, pinned: bool) -> LyingLayout {
    let widest = if height.is_finite() && height > 0.0 {
        height * WIDEST_SHAPE
    } else {
        f32::MAX
    };
    let row = (width - 2.0 * LYING_PADDING).clamp(0.0, widest);
    let free = row - GAMES_COLUMN - 2.0 * SIDE_COLUMN - 3.0 * LYING_GAP;
    let (apps, games, side) = if free >= NARROWEST_APPS {
        let extra = ((free - ROOMY_APPS) / 2.0).max(0.0);
        (free - extra, Some(GAMES_COLUMN + extra), SIDE_COLUMN)
    } else {
        let apps = row - 2.0 * SIDE_COLUMN - 2.0 * LYING_GAP;
        let shrink = ((NARROWEST_APPS - apps) / 2.0).clamp(0.0, SIDE_COLUMN - NARROWEST_SIDE);
        ((apps + 2.0 * shrink).max(0.0), None, SIDE_COLUMN - shrink)
    };
    let mut layout = LyingLayout {
        row,
        apps,
        games,
        notices: side,
        control: side,
        app_columns: fitting(apps, APP_CELL, APP_GAP, COLUMNS),
        game_columns: games.map_or(0, |games| fitting(games, GAME_CARD, GAME_GAP, 1)),
        result_columns: COLUMNS,
        body: 0.0,
        app_rows: ROWS,
        game_rows: 1,
        result_rows: 1,
    };
    layout.result_columns = fitting(layout.results(), APP_CELL, APP_GAP, COLUMNS);
    let body = (panel - LYING_PADDING - SEARCH_HEIGHT - SEARCH_GAP - LYING_FOOT).max(0.0);
    let grid = body - HEADING_HEIGHT - HEADING_GAP;
    let above = if pinned {
        PINNED_HEIGHT + PINNED_GAP
    } else {
        0.0
    };
    layout.body = body;
    layout.app_rows = rows(grid - above, APP_ROW, APP_GAP);
    layout.result_rows = rows(grid, APP_ROW, APP_GAP);
    let cards = grid - GAME_GAP - LIBRARY_HEIGHT;
    layout.game_rows = rows(cards, GAME_CARD_HEIGHT, GAME_GAP);
    layout
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

/// How many pages `count` applications take, `size` to a page; an empty
/// drawer still has one.
pub fn page_count(count: usize, size: usize) -> usize {
    count.div_ceil(size.max(1)).max(1)
}

/// How many page buttons the pager shows at most.
pub const PAGE_BUTTONS: usize = 5;

/// The pages that have a button: all of them when they are few, else the
/// ones around `page`, as many on each side when the ends allow.
pub fn page_buttons(page: usize, pages: usize) -> std::ops::Range<usize> {
    if pages <= PAGE_BUTTONS {
        return 0..pages;
    }
    let first = page
        .saturating_sub(PAGE_BUTTONS / 2)
        .min(pages - PAGE_BUTTONS);
    first..first + PAGE_BUTTONS
}

/// The applications of a page, numbered from 0, `size` to a page. A page
/// past the last one is the last one.
pub fn page<T>(items: &[T], page: usize, size: usize) -> &[T] {
    let size = size.max(1);
    let page = page.min(page_count(items.len(), size) - 1);
    let start = page * size;
    &items[start.min(items.len())..(start + size).min(items.len())]
}

/// The page shown after a notch of the wheel, or a swipe.
pub fn turn_page(current: usize, count: usize, size: usize, forward: bool) -> usize {
    let last = page_count(count, size) - 1;
    if forward {
        (current + 1).min(last)
    } else {
        current.min(last).saturating_sub(1)
    }
}

/// The line under the title of the grid: "47 · de A à Z · page 1 sur 3".
pub fn summary(count: usize, page: usize, size: usize) -> String {
    let pages = page_count(count, size);
    let page = page.min(pages - 1) + 1;
    format!("{count} · de A à Z · page {page} sur {pages}")
}

/// What matches a search, best first: the names that start with it, then
/// those with a word that does, then those that contain it. Within each
/// group the order of `items` is kept.
pub fn search<'a, T>(items: &'a [T], name: impl Fn(&T) -> &str, query: &str) -> Vec<&'a T> {
    search_with(items, name, |_| Vec::new(), query)
}

/// Like [`search`], and after the names, the items with a word of `also`
/// that starts with the search: what kind of application it is (« Navigateur
/// web ») and the keywords of its desktop entry.
pub fn search_with<'a, T>(
    items: &'a [T],
    name: impl Fn(&T) -> &str,
    also: impl Fn(&T) -> Vec<&str>,
    query: &str,
) -> Vec<&'a T> {
    let query = fold(query.trim());
    if query.is_empty() {
        return Vec::new();
    }
    let word_start = |text: &str| {
        let breaks = |character: char| !character.is_alphanumeric();
        fold(text)
            .split(breaks)
            .any(|word| word.starts_with(&query))
    };
    let rank = |item: &T| {
        let folded = fold(name(item));
        if folded.starts_with(&query) {
            Some(0)
        } else if word_start(name(item)) {
            Some(1)
        } else if folded.contains(&query) {
            Some(2)
        } else if also(item).into_iter().any(word_start) {
            Some(3)
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
    /// What the label says; with a `lone` screen, there is no screen to
    /// name, only whether the application reopens tidied.
    pub fn label(self, lone: bool) -> &'static str {
        match self {
            Self::Screen(_) if lone => "Rangée",
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

    const PAGE: usize = COLUMNS * ROWS;

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
            actions: Vec::new(),
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
    fn the_horizontal_launcher_is_600_high_unless_told_otherwise() {
        assert_eq!(height(None, Some(1080.0)), 600);
        assert_eq!(height(None, None), 600);
        assert_eq!(height(Some(640.0), Some(1080.0)), 640);
    }

    #[test]
    fn on_a_taller_screen_the_horizontal_launcher_is_taller() {
        assert_eq!(height(None, Some(1440.0)), 800);
        assert_eq!(height(None, Some(2160.0)), 1200);
        // Never so low that its content does not fit.
        assert_eq!(height(None, Some(800.0)), MIN_HEIGHT);
    }

    #[test]
    fn the_horizontal_launcher_keeps_between_its_content_and_the_screen() {
        assert_eq!(height(Some(100.0), Some(1080.0)), MIN_HEIGHT);
        // The dock under it and a strip above stay free.
        assert_eq!(height(Some(5000.0), Some(1080.0)), 1080 - 200);
        assert_eq!(height(Some(900.0), Some(480.0)), MIN_HEIGHT);
        assert_eq!(height(Some(f32::NAN), None), DEFAULT_HEIGHT);
    }

    #[test]
    fn the_horizontal_launcher_gives_the_applications_what_the_screen_adds() {
        let layout = lying_layout(1920.0, 1080.0, 500.0, false);
        assert_eq!(layout.row, 1892.0);
        assert_eq!(layout.games, Some(260.0));
        assert_eq!((layout.notices, layout.control), (340.0, 340.0));
        assert_eq!(layout.apps, 1892.0 - 260.0 - 680.0 - 42.0);
        // Six columns of applications, one of games, as before.
        assert_eq!((layout.app_columns, layout.game_columns), (6, 1));
        assert_eq!(layout.games_shown(), 3);
    }

    #[test]
    fn a_wider_screen_shares_what_it_adds_between_applications_and_games() {
        let narrow = lying_layout(1920.0, 1080.0, 500.0, false);
        let wide = lying_layout(2560.0, 1440.0, 500.0, false);
        let games = |layout: LyingLayout| layout.games.unwrap();
        assert_eq!(wide.row, 2532.0);
        assert_eq!(wide.apps - narrow.apps, games(wide) - games(narrow));
        assert_eq!((wide.notices, wide.control), (340.0, 340.0));
        assert!(wide.app_columns > narrow.app_columns);
        assert!(wide.game_columns > narrow.game_columns);
        let used = wide.apps + games(wide) + wide.notices + wide.control + 3.0 * LYING_GAP;
        assert_eq!(used, wide.row);
    }

    #[test]
    fn an_ultra_wide_screen_is_filled_from_edge_to_edge() {
        let layout = lying_layout(3440.0, 1440.0, 500.0, false);
        assert_eq!(layout.row, 3440.0 - 2.0 * LYING_PADDING);
        assert_eq!((layout.notices, layout.control), (340.0, 340.0));
        assert_eq!(
            layout.apps - ROOMY_APPS,
            layout.games.unwrap() - GAMES_COLUMN
        );
        assert_eq!((layout.app_columns, layout.game_columns), (11, 3));
        assert_eq!(layout.page(), 11 * layout.app_rows);
        assert_eq!(layout.games_shown(), 9);
        // The results of a search take both columns.
        assert!(layout.result_columns > layout.app_columns);
    }

    #[test]
    fn on_a_wider_screen_still_the_columns_keep_together_centered() {
        let layout = lying_layout(5120.0, 1440.0, 500.0, false);
        assert_eq!(layout.row, 1440.0 * WIDEST_SHAPE);
    }

    #[test]
    fn on_a_narrow_screen_the_games_give_their_place_first() {
        let layout = lying_layout(1280.0, 800.0, 500.0, false);
        assert_eq!(layout.games, None);
        assert_eq!(layout.game_columns, 0);
        assert_eq!(layout.notices, 340.0);
        assert!(layout.apps >= NARROWEST_APPS);
        assert_eq!(layout.app_columns, COLUMNS);
        // Narrower still, the notifications and the control center shrink.
        let layout = lying_layout(1024.0, 768.0, 500.0, false);
        assert_eq!(layout.notices, NARROWEST_SIDE);
        assert_eq!(layout.control, NARROWEST_SIDE);
        let used = layout.apps + layout.notices + layout.control + 2.0 * LYING_GAP;
        assert_eq!(used, layout.row);
    }

    #[test]
    fn a_launcher_of_500_shows_what_fits_under_the_search() {
        let layout = lying_layout(1920.0, 1080.0, 500.0, false);
        assert_eq!(layout.body, 500.0 - 14.0 - 44.0 - 12.0 - 6.0);
        // 424 under the search: the heading, then rows of 44, 4 apart.
        assert_eq!(layout.app_rows, 8);
        assert_eq!(layout.result_rows, 8);
        assert_eq!(layout.game_rows, 3);
        assert_eq!(layout.page(), 48);
        // The pinned applications take rows from the grid only.
        let pinned = lying_layout(1920.0, 1080.0, 500.0, true);
        assert_eq!(pinned.app_rows, 5);
        assert_eq!((pinned.game_rows, pinned.result_rows), (3, 8));
    }

    #[test]
    fn a_higher_launcher_shows_more_rows() {
        let low = lying_layout(3440.0, 1440.0, 500.0, true);
        let high = lying_layout(3440.0, 1440.0, 700.0, true);
        assert_eq!(high.body - low.body, 200.0);
        assert!(high.app_rows > low.app_rows);
        assert!(high.game_rows > low.game_rows);
        assert_eq!(high.games_shown(), high.game_rows * high.game_columns);
        // The columns do not change with the height.
        assert_eq!(high.app_columns, low.app_columns);
    }

    #[test]
    fn rows_never_overflow_the_body() {
        for panel in [480.0, 500.0, 567.0, 701.0, 990.0] {
            for pinned in [false, true] {
                let layout = lying_layout(1920.0, 1080.0, panel, pinned);
                let above = if pinned {
                    PINNED_HEIGHT + PINNED_GAP
                } else {
                    0.0
                };
                let rows = layout.app_rows as f32;
                let apps =
                    above + HEADING_HEIGHT + HEADING_GAP + rows * APP_ROW + (rows - 1.0) * APP_GAP;
                assert!(apps <= layout.body, "{panel} {pinned}");
                let rows = layout.game_rows as f32;
                let games = HEADING_HEIGHT
                    + HEADING_GAP
                    + rows * GAME_CARD_HEIGHT
                    + rows * GAME_GAP
                    + LIBRARY_HEIGHT;
                assert!(games <= layout.body, "{panel} {pinned}");
            }
        }
    }

    #[test]
    fn the_pager_shows_the_pages_around_the_current_one() {
        assert_eq!(page_buttons(0, 3), 0..3);
        assert_eq!(page_buttons(0, 18), 0..5);
        assert_eq!(page_buttons(1, 18), 0..5);
        assert_eq!(page_buttons(7, 18), 5..10);
        assert_eq!(page_buttons(17, 18), 13..18);
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

        assert_eq!(page_count(apps.len(), PAGE), 3);
        assert_eq!(page(&apps, 0, PAGE).len(), 18);
        assert_eq!(page(&apps, 1, PAGE).first().unwrap(), "app 18");
        assert_eq!(page(&apps, 2, PAGE).len(), 11);
        assert_eq!(page(&apps, 2, PAGE).last().unwrap(), "app 46");
    }

    #[test]
    fn a_full_last_page_does_not_add_an_empty_one() {
        assert_eq!(page_count(36, PAGE), 2);
        assert_eq!(page_count(37, PAGE), 3);
    }

    #[test]
    fn an_empty_drawer_has_one_empty_page() {
        let apps = names(0);

        assert_eq!(page_count(0, PAGE), 1);
        assert!(page(&apps, 0, PAGE).is_empty());
    }

    #[test]
    fn a_page_past_the_end_is_the_last_one() {
        let apps = names(20);

        assert_eq!(page(&apps, 7, PAGE), ["app 18", "app 19"]);
    }

    #[test]
    fn the_wheel_turns_pages_and_stops_at_both_ends() {
        assert_eq!(turn_page(0, 47, PAGE, true), 1);
        assert_eq!(turn_page(2, 47, PAGE, true), 2);
        assert_eq!(turn_page(1, 47, PAGE, false), 0);
        assert_eq!(turn_page(0, 47, PAGE, false), 0);
        assert_eq!(turn_page(9, 47, PAGE, false), 1);
    }

    #[test]
    fn the_summary_counts_applications_and_pages() {
        assert_eq!(summary(47, 0, PAGE), "47 · de A à Z · page 1 sur 3");
        assert_eq!(summary(47, 2, PAGE), "47 · de A à Z · page 3 sur 3");
        assert_eq!(summary(0, 0, PAGE), "0 · de A à Z · page 1 sur 1");
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
    fn an_application_is_found_by_its_kind_and_its_keywords_after_the_names() {
        // A name, what kind of application it is, its keywords.
        let apps = [
            ("Firefox", "Navigateur web", "Internet;WWW"),
            ("Kitty", "Émulateur de terminal", "shell;console"),
            ("Navi", "Aide-mémoire", ""),
        ];
        fn also<'b>(app: &'b (&str, &str, &str)) -> Vec<&'b str> {
            let mut terms = vec![app.1];
            terms.extend(app.2.split(';'));
            terms
        }
        let find = |query| {
            let hits = search_with(&apps, |app| app.0, also, query);
            hits.into_iter().map(|app| app.0).collect::<Vec<_>>()
        };

        assert_eq!(find("navig"), ["Firefox"]);
        assert_eq!(find("terminal"), ["Kitty"]);
        assert_eq!(find("emul"), ["Kitty"]);
        assert_eq!(find("www"), ["Firefox"]);
        // A name that matches comes before a kind that does.
        assert_eq!(find("nav"), ["Navi", "Firefox"]);
        // Inside a word of a kind is not enough: « Navigateur » holds « gat ».
        assert!(find("gat").is_empty());
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
        assert_eq!(place.label(false), "Droite");
        assert_eq!(place.label(true), "Rangée");
    }

    #[test]
    fn an_application_never_tidied_opens_floating() {
        let place = place(&identity("gimp", false), &[]);

        assert_eq!(place, Place::Floating);
        assert_eq!(place.label(true), "Flottant");
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
        assert_eq!(place.label(false), "Centre");
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
