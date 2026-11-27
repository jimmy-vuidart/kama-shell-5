use super::*;
use crate::geometry::{Insets, Point};
use crate::memory::RememberedPlace;

const LEFT: OutputId = OutputId(1);
const CENTER: OutputId = OutputId(2);
const RIGHT: OutputId = OutputId(3);

const SIDE_SIZE: Size = Size::new(1920, 1080);
const CENTER_SIZE: Size = Size::new(3440, 1440);

fn outputs() -> Vec<OutputInfo> {
    vec![
        OutputInfo::new(LEFT, OutputRole::Left, SIDE_SIZE),
        OutputInfo::new(CENTER, OutputRole::Center, CENTER_SIZE),
        OutputInfo::new(RIGHT, OutputRole::Right, SIDE_SIZE),
    ]
}

/// Left 16:9 in two columns, center 21:9 in one, right 16:9 in two.
fn desk() -> Desk {
    Desk::new(outputs(), Metrics::default())
}

fn slot(output: OutputId, index: usize) -> SlotId {
    SlotId::new(output, index)
}

fn open(desk: &mut Desk, id: u64, app: &str, cursor: OutputId) -> Placement {
    desk.open_window(
        WindowId(id),
        Some(AppKey::new(app)),
        OpenRequest::at(cursor),
    )
    .unwrap()
}

/// Opens a window and tidies it into a slot, as the user would.
fn open_in(desk: &mut Desk, id: u64, app: &str, output: OutputId, index: usize) {
    open(desk, id, app, output);
    desk.move_to_slot(WindowId(id), slot(output, index))
        .unwrap();
}

fn place(desk: &Desk, id: u64) -> Placement {
    desk.placement(WindowId(id)).unwrap()
}

fn geo(desk: &Desk, id: u64) -> WindowGeometry {
    desk.arrangement()
        .into_iter()
        .find(|geometry| geometry.id == WindowId(id))
        .unwrap()
}

fn tabs(desk: &Desk, output: OutputId, index: usize) -> Vec<u64> {
    desk.screen(output).unwrap().slots()[index]
        .windows()
        .iter()
        .map(|id| id.0)
        .collect()
}

fn active_tab(desk: &Desk, output: OutputId, index: usize) -> Option<u64> {
    desk.screen(output).unwrap().slots()[index]
        .active()
        .map(|id| id.0)
}

fn floating_output(placement: Placement) -> OutputId {
    match placement {
        Placement::Floating { output, .. } => output,
        other => panic!("expected a floating window, got {other:?}"),
    }
}

/// A game anchored on the center screen.
fn anchor_game(desk: &mut Desk, id: u64) {
    open_in(desk, id, "game", CENTER, 0);
    desk.set_anchored(WindowId(id), true).unwrap();
}

// Rule 1: an application never tidied opens floating, where the cursor is.

#[test]
fn an_unknown_app_opens_floating_on_the_cursor_screen() {
    let mut desk = desk();

    let placement = open(&mut desk, 1, "gimp", RIGHT);

    assert_eq!(floating_output(placement), RIGHT);
    assert_eq!(place(&desk, 1), placement);
    assert!(geo(&desk, 1).floating);
}

#[test]
fn a_floating_window_opens_centered_at_its_preferred_size() {
    let mut desk = desk();

    let placement = desk
        .open_window(
            WindowId(1),
            None,
            OpenRequest::at(RIGHT).with_size(Size::new(800, 600)),
        )
        .unwrap();

    assert_eq!(
        placement,
        Placement::Floating {
            output: RIGHT,
            rect: Rect::new(560, 240, 800, 600),
        }
    );
}

#[test]
fn a_floating_window_without_preferred_size_fits_in_the_screen() {
    let mut desk = desk();

    let Placement::Floating { rect, .. } = open(&mut desk, 1, "gimp", LEFT) else {
        panic!("expected a floating window");
    };

    assert!(!rect.is_empty());
    assert_eq!(rect, rect.clamped_to(Rect::new(8, 8, 1904, 1064)));
}

/// Opens a window of `size` floating, with `request` saying the rest.
fn open_sized(desk: &mut Desk, id: u64, app: Option<&str>, request: OpenRequest) -> Rect {
    let app = app.map(AppKey::new);
    match desk.open_window(WindowId(id), app, request).unwrap() {
        Placement::Floating { rect, .. } => rect,
        other => panic!("expected a floating window, got {other:?}"),
    }
}

#[test]
fn a_window_opening_where_another_floats_moves_down_and_right() {
    let mut desk = desk();
    let request = OpenRequest::at(RIGHT).with_size(Size::new(800, 600));

    let first = open_sized(&mut desk, 1, Some("kitty"), request);
    let second = open_sized(&mut desk, 2, Some("kitty"), request);
    let third = open_sized(&mut desk, 3, Some("kitty"), request);

    assert_eq!(first, Rect::new(560, 240, 800, 600));
    assert_eq!(second, Rect::new(592, 272, 800, 600));
    assert_eq!(third, Rect::new(624, 304, 800, 600));
}

#[test]
fn a_window_opens_centered_again_once_the_place_is_free() {
    let mut desk = desk();
    let request = OpenRequest::at(RIGHT).with_size(Size::new(800, 600));
    open_sized(&mut desk, 1, None, request);

    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(
        open_sized(&mut desk, 2, None, request),
        Rect::new(560, 240, 800, 600)
    );
}

#[test]
fn a_window_that_chose_its_size_still_counts_where_it_is_centered() {
    let mut desk = desk();
    open_sized(&mut desk, 1, None, OpenRequest::at(RIGHT));
    // Its client chose a smaller size: centered where it opened.
    let rect = Rect::centered(Size::new(700, 500), geo(&desk, 1).frame);
    desk.set_floating_rect(WindowId(1), rect).unwrap();

    let second = open_sized(&mut desk, 2, None, OpenRequest::at(RIGHT));

    let center = |rect: Rect| (rect.x + rect.w / 2, rect.y + rect.h / 2);
    let ((x1, y1), (x2, y2)) = (center(rect), center(second));
    assert_eq!((x2 - x1, y2 - y1), (32, 32));
}

#[test]
fn dialogs_of_the_same_window_do_not_hide_each_other() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", RIGHT, 0);
    let request = OpenRequest::at(RIGHT)
        .with_size(Size::new(400, 300))
        .with_parent(WindowId(1));

    let first = open_sized(&mut desk, 2, Some("nautilus"), request);
    let second = open_sized(&mut desk, 3, Some("nautilus"), request);

    assert_eq!(second, first.translated(Point::new(32, 32)));
}

#[test]
fn the_cascade_stops_at_the_edge_of_the_screen() {
    let mut desk = desk();
    let request = OpenRequest::at(LEFT).with_size(Size::new(1900, 1060));

    let first = open_sized(&mut desk, 1, None, request);
    let second = open_sized(&mut desk, 2, None, request);

    assert_eq!(second, first);
}

#[test]
fn a_dialog_opens_over_its_window_and_ignores_the_habit_of_its_application() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", RIGHT, 0);
    open_in(&mut desk, 2, "firefox", RIGHT, 1);
    let parent = geo(&desk, 1).frame;
    let habits = desk.memory().clone();

    let request = OpenRequest::at(RIGHT)
        .with_size(Size::new(400, 300))
        .with_parent(WindowId(1));
    let dialog = open_sized(&mut desk, 3, Some("nautilus"), request);

    assert_eq!(dialog, Rect::centered(Size::new(400, 300), parent));
    assert_eq!(tabs(&desk, RIGHT, 0), [1]);
    assert_eq!(desk.memory(), &habits);
    assert_eq!(desk.focused(), Some(WindowId(3)));
}

#[test]
fn a_dialog_whose_window_is_elsewhere_opens_centered_on_its_screen() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", LEFT, 0);

    let request = OpenRequest::at(RIGHT)
        .with_size(Size::new(800, 600))
        .with_parent(WindowId(1));

    assert_eq!(
        open_sized(&mut desk, 2, Some("nautilus"), request),
        Rect::new(560, 240, 800, 600)
    );
}

#[test]
fn a_dialog_over_a_window_near_the_edge_stays_on_the_screen() {
    let mut desk = desk();
    let parent = OpenRequest::at(RIGHT).with_size(Size::new(300, 200));
    open_sized(&mut desk, 1, None, parent);
    desk.float_window(WindowId(1), RIGHT, Rect::new(8, 8, 300, 200))
        .unwrap();

    let request = OpenRequest::at(RIGHT)
        .with_size(Size::new(800, 600))
        .with_parent(WindowId(1));

    assert_eq!(
        open_sized(&mut desk, 2, None, request),
        Rect::new(8, 8, 800, 600)
    );
}

#[test]
fn a_preferred_size_larger_than_the_screen_is_shrunk() {
    let mut desk = desk();

    let placement = desk
        .open_window(
            WindowId(1),
            None,
            OpenRequest::at(LEFT).with_size(Size::new(5000, 5000)),
        )
        .unwrap();

    assert_eq!(
        placement,
        Placement::Floating {
            output: LEFT,
            rect: Rect::new(8, 8, 1904, 1064),
        }
    );
}

#[test]
fn opening_a_window_does_not_move_the_others() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    let before = geo(&desk, 1);

    open(&mut desk, 2, "gimp", LEFT);

    let after = geo(&desk, 1);
    assert_eq!(after.frame, before.frame);
    assert!(after.visible);
    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn opening_a_window_focuses_it() {
    let mut desk = desk();
    open(&mut desk, 1, "discord", LEFT);

    open(&mut desk, 2, "gimp", RIGHT);

    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn opening_the_same_window_twice_is_refused() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);

    let result = desk.open_window(WindowId(1), None, OpenRequest::at(LEFT));

    assert_eq!(result, Err(ModelError::DuplicateWindow(WindowId(1))));
}

#[test]
fn opening_on_an_unknown_output_is_refused() {
    let mut desk = desk();

    let result = desk.open_window(WindowId(1), None, OpenRequest::at(OutputId(99)));

    assert_eq!(result, Err(ModelError::UnknownOutput(OutputId(99))));
    assert_eq!(desk.placement(WindowId(1)), None);
}

// Rule 3: a screen is never full.

#[test]
fn dropping_on_an_empty_slot_places_the_window_there() {
    let mut desk = desk();
    open_in(&mut desk, 2, "notes", RIGHT, 0);
    open(&mut desk, 1, "guide", CENTER);

    desk.move_to_slot(WindowId(1), slot(RIGHT, 1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(RIGHT, 1)));
    assert_eq!(tabs(&desk, RIGHT, 0), [2]);
    assert!(desk.screen(CENTER).unwrap().floating().is_empty());
}

#[test]
fn dropping_on_an_occupied_slot_stacks_the_window_as_a_tab() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open(&mut desk, 2, "browser", RIGHT);

    desk.move_to_slot(WindowId(2), slot(RIGHT, 0)).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [1, 2]);
    assert_eq!(active_tab(&desk, RIGHT, 0), Some(2));
}

#[test]
fn a_slot_accepts_any_number_of_windows() {
    let mut desk = desk();

    for id in 1..=6 {
        open_in(&mut desk, id, "browser", RIGHT, 0);
    }

    assert_eq!(tabs(&desk, RIGHT, 0), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn moving_to_another_slot_closes_the_column_it_leaves() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.move_to_slot(WindowId(1), slot(LEFT, 0)).unwrap();

    assert_eq!(tabs(&desk, LEFT, 0), [1]);
    assert_eq!(desk.screen(RIGHT).unwrap().slots().len(), 1);
    assert_eq!(geo(&desk, 2).frame, Rect::new(8, 48, 1904, 1024));
}

#[test]
fn a_window_moved_right_on_its_own_screen_lands_in_the_new_column() {
    let mut desk = desk();
    desk.set_layout(RIGHT, LayoutKind::ThreeColumns).unwrap();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.move_to_slot(WindowId(1), slot(RIGHT, 2)).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [2]);
    assert_eq!(tabs(&desk, RIGHT, 1), [1]);
    assert_eq!(
        desk.memory().get(&AppKey::new("guide")),
        Some(RememberedPlace::in_slot(OutputRole::Right, 1))
    );
}

#[test]
fn dropping_on_a_slot_the_layout_does_not_have_is_refused() {
    let mut desk = desk();
    open(&mut desk, 1, "guide", LEFT);

    let result = desk.move_to_slot(WindowId(1), slot(LEFT, 2));

    assert_eq!(result, Err(ModelError::UnknownSlot(slot(LEFT, 2))));
    assert!(matches!(place(&desk, 1), Placement::Floating { .. }));
}

#[test]
fn moving_an_unknown_window_is_refused() {
    let mut desk = desk();

    let result = desk.move_to_slot(WindowId(7), slot(LEFT, 0));

    assert_eq!(result, Err(ModelError::UnknownWindow(WindowId(7))));
}

// Rule 4: per-application memory.

#[test]
fn a_tidied_app_reopens_in_the_same_slot() {
    let mut desk = desk();
    open_in(&mut desk, 9, "guide", LEFT, 0);
    open(&mut desk, 1, "discord", RIGHT);
    desk.move_to_slot(WindowId(1), slot(LEFT, 1)).unwrap();
    desk.close_window(WindowId(1)).unwrap();

    let placement = open(&mut desk, 2, "discord", CENTER);

    assert_eq!(placement, Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn a_second_window_of_an_app_opens_beside_the_first_while_there_is_room() {
    let mut desk = desk();
    open_in(&mut desk, 1, "kitty", LEFT, 0);

    let second = open(&mut desk, 2, "kitty", CENTER);
    let third = open(&mut desk, 3, "kitty", CENTER);

    assert_eq!(second, Placement::Slot(slot(LEFT, 1)));
    assert_eq!(third, Placement::Slot(slot(LEFT, 0)));
    assert_eq!(tabs(&desk, LEFT, 0), [1, 3]);
    assert_eq!(tabs(&desk, LEFT, 1), [2]);
}

#[test]
fn an_app_left_floating_creates_no_habit() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", RIGHT);
    desk.float_window(WindowId(1), RIGHT, Rect::new(100, 100, 640, 480))
        .unwrap();
    desk.close_window(WindowId(1)).unwrap();

    let placement = open(&mut desk, 2, "gimp", LEFT);

    assert!(desk.memory().is_empty());
    assert_eq!(floating_output(placement), LEFT);
}

#[test]
fn a_window_without_app_key_is_never_remembered() {
    let mut desk = desk();
    desk.open_window(WindowId(1), None, OpenRequest::at(LEFT))
        .unwrap();

    desk.move_to_slot(WindowId(1), slot(LEFT, 0)).unwrap();

    assert!(desk.memory().is_empty());
}

#[test]
fn a_remembered_slot_missing_from_the_layout_falls_back_to_the_last_slot() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.close_window(WindowId(2)).unwrap();
    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();

    let placement = open(&mut desk, 3, "notes", LEFT);

    assert_eq!(placement, Placement::Slot(slot(RIGHT, 0)));
    assert_eq!(tabs(&desk, RIGHT, 0), [1, 3]);
}

#[test]
fn a_remembered_column_that_closed_opens_again_at_its_place() {
    let mut desk = desk();
    open_in(&mut desk, 1, "agent", LEFT, 0);
    open_in(&mut desk, 2, "product", LEFT, 1);
    desk.close_window(WindowId(1)).unwrap();
    desk.close_window(WindowId(2)).unwrap();

    open(&mut desk, 3, "product", CENTER);
    let placement = open(&mut desk, 4, "agent", CENTER);

    assert_eq!(placement, Placement::Slot(slot(LEFT, 0)));
    assert_eq!(tabs(&desk, LEFT, 0), [4]);
    assert_eq!(tabs(&desk, LEFT, 1), [3]);
}

#[test]
fn a_remembered_column_that_is_there_takes_the_app_as_a_tab_when_the_screen_is_full() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    open_in(&mut desk, 3, "chat", RIGHT, 1);
    desk.close_window(WindowId(3)).unwrap();

    let placement = open(&mut desk, 4, "chat", LEFT);

    assert_eq!(placement, Placement::Slot(slot(RIGHT, 1)));
    assert_eq!(tabs(&desk, RIGHT, 1), [2, 4]);
}

#[test]
fn floating_a_tidied_app_forgets_its_habit() {
    let mut desk = desk();
    open_in(&mut desk, 1, "notes", LEFT, 0);

    desk.float_window(WindowId(1), LEFT, Rect::new(100, 100, 640, 480))
        .unwrap();

    assert!(desk.memory().is_empty());
}

#[test]
fn memory_loaded_from_disk_places_the_app() {
    let mut memory = AppMemory::default();
    memory.remember(
        AppKey::new("code"),
        RememberedPlace::in_slot(OutputRole::Right, 0),
    );
    let mut desk = desk().with_memory(memory);

    let placement = open(&mut desk, 1, "code", LEFT);

    assert_eq!(placement, Placement::Slot(slot(RIGHT, 0)));
}

#[test]
fn a_habit_on_a_screen_that_is_not_plugged_opens_floating() {
    let mut memory = AppMemory::default();
    memory.remember(
        AppKey::new("code"),
        RememberedPlace::in_slot(OutputRole::Right, 0),
    );
    let only_center = vec![OutputInfo::new(CENTER, OutputRole::Center, CENTER_SIZE)];
    let mut desk = Desk::new(only_center, Metrics::default()).with_memory(memory);

    let placement = open(&mut desk, 1, "code", CENTER);

    assert_eq!(floating_output(placement), CENTER);
    assert_eq!(desk.memory().len(), 1);
}

// Rule 7: never in front of an anchored game.

#[test]
fn with_a_game_anchored_an_app_opens_floating_on_the_last_used_side_screen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    desk.note_output_used(RIGHT);

    let placement = open(&mut desk, 2, "gimp", CENTER);

    assert_eq!(floating_output(placement), RIGHT);
}

#[test]
fn the_last_used_side_screen_follows_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "discord", LEFT, 0);
    anchor_game(&mut desk, 3);
    desk.focus(WindowId(1)).unwrap();
    desk.focus(WindowId(3)).unwrap();

    let placement = open(&mut desk, 4, "gimp", CENTER);

    assert_eq!(floating_output(placement), RIGHT);
}

#[test]
fn with_no_side_screen_used_yet_the_first_one_is_chosen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    let placement = open(&mut desk, 2, "gimp", CENTER);

    assert_eq!(floating_output(placement), LEFT);
}

#[test]
fn a_panel_opens_where_the_pointer_is_without_a_game() {
    let desk = desk();

    assert_eq!(desk.opening_screen(CENTER), Ok(CENTER));
    assert_eq!(desk.opening_screen(RIGHT), Ok(RIGHT));
}

#[test]
fn with_a_game_anchored_a_panel_opens_on_the_last_used_side_screen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    desk.note_output_used(RIGHT);

    assert_eq!(desk.opening_screen(CENTER), Ok(RIGHT));
}

#[test]
fn when_games_hold_every_screen_a_panel_opens_over_the_one_of_the_pointer() {
    let only_center = vec![OutputInfo::new(CENTER, OutputRole::Center, CENTER_SIZE)];
    let mut desk = Desk::new(only_center, Metrics::default());
    anchor_game(&mut desk, 1);

    assert_eq!(desk.opening_screen(CENTER), Ok(CENTER));
}

#[test]
fn an_app_remembered_at_the_center_is_deflected_and_keeps_its_habit() {
    let mut desk = desk();
    open_in(&mut desk, 1, "browser", CENTER, 0);
    desk.close_window(WindowId(1)).unwrap();
    anchor_game(&mut desk, 2);
    desk.note_output_used(RIGHT);

    let placement = open(&mut desk, 3, "browser", CENTER);

    assert_eq!(floating_output(placement), RIGHT);
    assert_eq!(
        desk.memory().get(&AppKey::new("browser")),
        Some(RememberedPlace::in_slot(OutputRole::Center, 0))
    );
}

#[test]
fn an_app_remembered_on_a_side_screen_still_goes_to_its_slot_during_a_game() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();
    anchor_game(&mut desk, 2);
    desk.note_output_used(RIGHT);

    let placement = open(&mut desk, 3, "discord", CENTER);

    assert_eq!(placement, Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn a_window_opening_while_a_game_is_played_leaves_the_focus_to_the_game() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    let placement = open(&mut desk, 2, "chat", CENTER);

    assert_eq!(floating_output(placement), LEFT);
    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 2).focused);
}

#[test]
fn a_tidied_app_opening_while_a_game_is_played_shows_without_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "notes", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();
    anchor_game(&mut desk, 3);

    let placement = open(&mut desk, 4, "discord", CENTER);

    assert_eq!(placement, Placement::Slot(slot(LEFT, 0)));
    assert_eq!(active_tab(&desk, LEFT, 0), Some(4));
    assert_eq!(desk.focused(), Some(WindowId(3)));
}

#[test]
fn the_window_opened_during_a_game_is_next_in_line_for_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", RIGHT, 0);
    anchor_game(&mut desk, 2);
    open(&mut desk, 3, "chat", CENTER);

    desk.close_window(WindowId(2)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(3)));
}

#[test]
fn a_window_opening_while_the_game_is_left_aside_takes_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", RIGHT, 0);
    anchor_game(&mut desk, 2);
    desk.focus(WindowId(1)).unwrap();

    open(&mut desk, 3, "chat", RIGHT);

    assert_eq!(desk.focused(), Some(WindowId(3)));
}

// Anchored screen (game mode).

#[test]
fn an_anchored_window_covers_the_whole_output_without_decoration() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    let game = geo(&desk, 1);

    assert_eq!(game.frame, Rect::from_size(CENTER_SIZE));
    assert_eq!(game.client, game.frame);
    assert_eq!(game.titlebar, None);
    assert!(game.anchored);
    assert!(game.visible);
}

#[test]
fn anchoring_focuses_the_game() {
    let mut desk = desk();
    open_in(&mut desk, 1, "game", CENTER, 0);
    open_in(&mut desk, 2, "discord", LEFT, 0);

    desk.set_anchored(WindowId(1), true).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn other_windows_of_an_anchored_screen_are_hidden() {
    let mut desk = desk();
    open(&mut desk, 1, "notes", CENTER);
    anchor_game(&mut desk, 2);

    assert!(!geo(&desk, 1).visible);
}

#[test]
fn an_anchored_screen_offers_no_drop_zone() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open(&mut desk, 2, "notes", LEFT);

    let zones = desk.drop_zones(WindowId(2));

    assert!(!zones.is_empty());
    assert!(zones.iter().all(|zone| zone.slot.output != CENTER));
}

#[test]
fn shell_gestures_are_refused_on_an_anchored_screen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open(&mut desk, 2, "notes", LEFT);

    assert_eq!(
        desk.move_to_slot(WindowId(2), slot(CENTER, 0)),
        Err(ModelError::Anchored(CENTER))
    );
    assert_eq!(
        desk.float_window(WindowId(2), CENTER, Rect::new(0, 0, 400, 300)),
        Err(ModelError::Anchored(CENTER))
    );
    assert_eq!(
        desk.move_to_slot(WindowId(1), slot(LEFT, 0)),
        Err(ModelError::Anchored(CENTER))
    );
}

#[test]
fn releasing_the_anchor_puts_the_window_back_in_its_slot() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    desk.set_anchored(WindowId(1), false).unwrap();

    let game = geo(&desk, 1);
    assert!(!game.anchored);
    assert_eq!(game.frame, Rect::new(8, 48, 3424, 1384));
    assert!(game.titlebar.is_some());
    assert_eq!(desk.screen(CENTER).unwrap().anchored(), None);
}

#[test]
fn closing_the_anchored_window_releases_the_screen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(desk.screen(CENTER).unwrap().anchored(), None);
}

#[test]
fn going_fullscreen_on_another_screen_moves_the_window_there() {
    let mut desk = desk();
    open_in(&mut desk, 1, "video", LEFT, 0);

    desk.set_fullscreen(WindowId(1), Some(CENTER)).unwrap();

    let video = geo(&desk, 1);
    assert_eq!(video.output, CENTER);
    assert!(video.anchored);
    assert_eq!(video.frame, Rect::from_size(CENTER_SIZE));
    assert_eq!(desk.screen(CENTER).unwrap().anchored(), Some(WindowId(1)));
    assert_eq!(desk.screen(LEFT).unwrap().anchored(), None);
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn going_fullscreen_without_naming_a_screen_anchors_the_window_where_it_is() {
    let mut desk = desk();
    open_in(&mut desk, 2, "notes", LEFT, 0);
    open_in(&mut desk, 1, "video", LEFT, 1);

    desk.set_fullscreen(WindowId(1), None).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::from_size(SIDE_SIZE));
    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn going_fullscreen_on_its_own_screen_keeps_the_place_of_the_window() {
    let mut desk = desk();
    open_in(&mut desk, 2, "notes", LEFT, 0);
    open_in(&mut desk, 1, "video", LEFT, 1);

    desk.set_fullscreen(WindowId(1), Some(LEFT)).unwrap();
    desk.set_anchored(WindowId(1), false).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn a_window_sent_fullscreen_to_another_screen_floats_there_afterwards() {
    let mut desk = desk();
    open_in(&mut desk, 1, "video", LEFT, 0);
    let habits = desk.memory().clone();

    desk.set_fullscreen(WindowId(1), Some(RIGHT)).unwrap();
    desk.set_anchored(WindowId(1), false).unwrap();

    assert_eq!(floating_output(place(&desk, 1)), RIGHT);
    assert!(geo(&desk, 1).visible);
    // The application asked for it, not the user: habits are left alone.
    assert_eq!(desk.memory(), &habits);
}

#[test]
fn going_fullscreen_on_an_unknown_screen_is_refused() {
    let mut desk = desk();
    open(&mut desk, 1, "video", LEFT);

    assert_eq!(
        desk.set_fullscreen(WindowId(1), Some(OutputId(99))),
        Err(ModelError::UnknownOutput(OutputId(99)))
    );
    assert!(!geo(&desk, 1).anchored);
}

#[test]
fn a_second_fullscreen_window_takes_the_screen_from_the_first() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open(&mut desk, 2, "video", LEFT);

    desk.set_fullscreen(WindowId(2), Some(CENTER)).unwrap();

    assert!(geo(&desk, 2).anchored);
    assert!(!geo(&desk, 1).anchored);
    assert!(!geo(&desk, 1).visible);
}

#[test]
fn the_anchored_window_leaving_for_another_screen_releases_the_first() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open(&mut desk, 2, "notes", CENTER);

    desk.set_fullscreen(WindowId(1), Some(LEFT)).unwrap();

    assert_eq!(desk.screen(CENTER).unwrap().anchored(), None);
    assert_eq!(desk.screen(LEFT).unwrap().anchored(), Some(WindowId(1)));
}

#[test]
fn the_focus_never_goes_to_a_window_hidden_under_a_game() {
    let mut desk = desk();
    open_in(&mut desk, 1, "notes", CENTER, 0);
    desk.minimize(WindowId(1)).unwrap();
    anchor_game(&mut desk, 2);

    // Restoring a window of the anchored screen cannot show it.
    desk.restore_last();

    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert!(geo(&desk, 2).focused);
    assert!(!geo(&desk, 1).focused);
}

#[test]
fn a_window_hidden_under_a_game_gets_the_focus_back_when_the_game_leaves() {
    let mut desk = desk();
    open_in(&mut desk, 1, "notes", CENTER, 0);
    anchor_game(&mut desk, 2);
    desk.focus(WindowId(1)).unwrap();
    assert_eq!(desk.focused(), Some(WindowId(2)));

    desk.close_window(WindowId(2)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn an_anchored_screen_has_no_separation_to_grab() {
    let mut desk = desk();
    open_in(&mut desk, 1, "video", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    assert_eq!(desk.separators(RIGHT).len(), 1);

    desk.set_anchored(WindowId(1), true).unwrap();

    assert!(desk.separators(RIGHT).is_empty());
    assert_eq!(
        desk.move_separator(RIGHT, 0, Point::new(900, 500)),
        Err(ModelError::Anchored(RIGHT))
    );
}

#[test]
fn a_game_is_in_play_while_it_has_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    assert_eq!(desk.game_in_play(), None);

    anchor_game(&mut desk, 2);
    assert_eq!(desk.game_in_play(), Some(CENTER));

    desk.focus(WindowId(1)).unwrap();
    assert_eq!(desk.game_in_play(), None);

    desk.focus(WindowId(2)).unwrap();
    assert_eq!(desk.game_in_play(), Some(CENTER));
}

// Mod+Escape: which game goes back into a window.

#[test]
fn the_focused_game_is_the_one_put_back_in_a_window() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open_in(&mut desk, 2, "video", LEFT, 0);
    desk.set_anchored(WindowId(2), true).unwrap();

    assert_eq!(desk.game_to_release(CENTER), Some(WindowId(2)));
}

#[test]
fn without_a_focused_game_the_one_under_the_pointer_is_put_back_in_a_window() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open_in(&mut desk, 2, "video", LEFT, 0);
    desk.set_anchored(WindowId(2), true).unwrap();
    open_in(&mut desk, 3, "discord", RIGHT, 0);

    assert_eq!(desk.game_to_release(CENTER), Some(WindowId(1)));
    assert_eq!(desk.game_to_release(LEFT), Some(WindowId(2)));
}

#[test]
fn from_a_side_screen_the_only_game_is_put_back_in_a_window() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open_in(&mut desk, 2, "discord", RIGHT, 0);

    assert_eq!(desk.game_to_release(RIGHT), Some(WindowId(1)));
}

#[test]
fn without_any_game_nothing_is_put_back_in_a_window() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", RIGHT, 0);

    assert_eq!(desk.game_to_release(RIGHT), None);
}

// Layout changes.

#[test]
fn going_from_columns_to_full_merges_the_slots_into_tabs() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.focus(WindowId(1)).unwrap();

    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();

    assert_eq!(desk.screen(RIGHT).unwrap().layout(), LayoutKind::Full);
    assert_eq!(tabs(&desk, RIGHT, 0), [1, 2]);
    assert_eq!(active_tab(&desk, RIGHT, 0), Some(1));
}

#[test]
fn merging_slots_keeps_the_focused_window_visible() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 1).visible);
}

#[test]
fn going_from_full_to_columns_moves_nothing() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "admin", CENTER, 0);

    desk.set_layout(CENTER, LayoutKind::TwoColumns).unwrap();

    assert_eq!(desk.screen(CENTER).unwrap().slots().len(), 1);
    assert_eq!(tabs(&desk, CENTER, 0), [1, 2]);
    assert_eq!(geo(&desk, 2).frame, Rect::new(8, 48, 3424, 1384));
}

#[test]
fn going_from_three_columns_to_two_merges_the_third_into_the_second() {
    let mut desk = desk();
    desk.set_layout(RIGHT, LayoutKind::ThreeColumns).unwrap();
    for (id, app) in [(1, "guide"), (2, "notes"), (3, "chat")] {
        open_in(&mut desk, id, app, RIGHT, id as usize - 1);
    }

    desk.set_layout(RIGHT, LayoutKind::TwoColumns).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [1]);
    assert_eq!(tabs(&desk, RIGHT, 1), [2, 3]);
    assert_eq!(active_tab(&desk, RIGHT, 1), Some(3));
}

#[test]
fn changing_the_layout_of_an_anchored_screen_is_allowed() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    desk.set_layout(CENTER, LayoutKind::TwoColumns).unwrap();

    assert_eq!(
        desk.screen(CENTER).unwrap().layout(),
        LayoutKind::TwoColumns
    );
    assert!(geo(&desk, 1).anchored);
}

#[test]
fn main_side_gives_two_thirds_to_the_left_column() {
    let mut desk = desk();
    desk.set_layout(CENTER, LayoutKind::MainSide).unwrap();
    open_in(&mut desk, 1, "product", CENTER, 0);
    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 3424, 1384));

    open_in(&mut desk, 2, "admin", CENTER, 1);

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 2278, 1384));
    assert_eq!(geo(&desk, 2).frame, Rect::new(2294, 48, 1138, 1384));
}

#[test]
fn a_separation_moves_and_its_widths_survive_a_layout_change() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    assert_eq!(
        desk.move_separator(RIGHT, 0, Point::new(1112, 500)),
        Ok(true)
    );
    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 1100, 1024));
    assert_eq!(geo(&desk, 2).frame, Rect::new(1116, 48, 796, 1024));
    assert_eq!(
        desk.move_separator(RIGHT, 0, Point::new(1112, 500)),
        Ok(false)
    );

    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();
    open_in(&mut desk, 3, "chat", LEFT, 0);
    desk.set_layout(RIGHT, LayoutKind::TwoColumns).unwrap();
    desk.move_to_slot(WindowId(2), slot(RIGHT, 1)).unwrap();
    assert_eq!(geo(&desk, 2).frame, Rect::new(1116, 48, 796, 1024));
}

#[test]
fn each_number_of_columns_keeps_its_own_widths() {
    let mut desk = desk();
    desk.set_layout(RIGHT, LayoutKind::ThreeColumns).unwrap();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.move_separator(RIGHT, 0, Point::new(1112, 500))
        .unwrap();
    open_in(&mut desk, 3, "chat", RIGHT, 2);

    assert_eq!(desk.separators(RIGHT).len(), 2);
    assert_eq!(geo(&desk, 1).frame.w, 629);

    desk.close_window(WindowId(3)).unwrap();
    assert_eq!(geo(&desk, 1).frame.w, 1100);
}

#[test]
fn the_widths_of_the_columns_are_kept_for_the_next_session() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.move_separator(RIGHT, 0, Point::new(1112, 500))
        .unwrap();

    let mut next = self::desk().with_proportions(&desk.proportions());
    open_in(&mut next, 1, "guide", RIGHT, 0);
    open_in(&mut next, 2, "notes", RIGHT, 1);

    assert_eq!(geo(&next, 1).frame, Rect::new(8, 48, 1100, 1024));
}

// Focus.

#[test]
fn exactly_one_window_is_focused() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "product", CENTER, 0);
    open(&mut desk, 3, "gimp", RIGHT);

    desk.focus(WindowId(2)).unwrap();

    let focused: Vec<_> = desk
        .arrangement()
        .into_iter()
        .filter(|geometry| geometry.focused)
        .map(|geometry| geometry.id)
        .collect();
    assert_eq!(focused, [WindowId(2)]);
}

#[test]
fn focusing_a_hidden_tab_shows_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "browser", RIGHT, 0);

    desk.focus(WindowId(1)).unwrap();

    assert!(geo(&desk, 1).visible);
    assert!(!geo(&desk, 2).visible);
}

#[test]
fn focusing_a_floating_window_raises_it() {
    let mut desk = desk();
    open(&mut desk, 1, "blender", LEFT);
    open(&mut desk, 2, "materials", LEFT);

    desk.focus(WindowId(1)).unwrap();

    let order: Vec<_> = desk.arrangement().iter().map(|g| g.id.0).collect();
    assert_eq!(order, [2, 1]);
}

#[test]
fn focusing_an_unknown_window_is_refused() {
    let mut desk = desk();

    assert_eq!(
        desk.focus(WindowId(9)),
        Err(ModelError::UnknownWindow(WindowId(9)))
    );
}

#[test]
fn closing_the_focused_tab_focuses_its_neighbour() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "guide", RIGHT, 0);
    open_in(&mut desk, 3, "browser", RIGHT, 0);
    desk.focus(WindowId(1)).unwrap();
    desk.focus(WindowId(3)).unwrap();

    desk.close_window(WindowId(3)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn closing_a_focused_window_alone_in_its_slot_focuses_the_previous_one() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "product", CENTER, 0);
    open_in(&mut desk, 3, "guide", RIGHT, 0);
    desk.focus(WindowId(1)).unwrap();
    desk.focus(WindowId(3)).unwrap();

    desk.close_window(WindowId(3)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn closing_the_last_window_leaves_nothing_focused() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);

    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(desk.focused(), None);
    assert!(desk.arrangement().is_empty());
    assert_eq!(desk.window(WindowId(1)), None);
}

#[test]
fn closing_an_unfocused_window_keeps_the_focus() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    open(&mut desk, 2, "notes", RIGHT);

    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn activating_a_tab_focuses_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "browser", RIGHT, 0);
    open_in(&mut desk, 3, "product", CENTER, 0);

    desk.activate_tab(slot(RIGHT, 0), 0).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert_eq!(active_tab(&desk, RIGHT, 0), Some(1));
}

#[test]
fn activating_a_tab_that_does_not_exist_is_refused() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    assert_eq!(
        desk.activate_tab(slot(RIGHT, 0), 3),
        Err(ModelError::UnknownTab {
            stack: StackId::Slot(slot(RIGHT, 0)),
            index: 3,
        })
    );
}

#[test]
fn cycling_tabs_focuses_the_next_one() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "browser", RIGHT, 0);

    desk.cycle_tab(slot(RIGHT, 0), true).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn a_title_can_be_set_and_read_back() {
    let mut desk = desk();
    open(&mut desk, 1, "code", LEFT);

    desk.set_title(WindowId(1), "Newsletter.tsx").unwrap();

    let record = desk.window(WindowId(1)).unwrap();
    assert_eq!(record.title, "Newsletter.tsx");
    assert_eq!(record.app, Some(AppKey::new("code")));
}

// Arrangement.

#[test]
fn a_tiled_window_has_a_title_bar_above_its_client_area() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);

    let window = geo(&desk, 1);

    assert_eq!(window.output, LEFT);
    assert_eq!(window.frame, Rect::new(8, 48, 1904, 1024));
    assert_eq!(window.titlebar, Some(Rect::new(8, 48, 1904, 38)));
    assert_eq!(window.client, Rect::new(8, 86, 1904, 986));
    assert!(!window.floating);
}

#[test]
fn only_the_active_tab_of_a_slot_is_visible() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "browser", RIGHT, 0);

    assert!(!geo(&desk, 1).visible);
    assert!(geo(&desk, 2).visible);
    assert_eq!(geo(&desk, 1).frame, geo(&desk, 2).frame);
}

#[test]
fn floating_windows_come_above_tiled_ones() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    open_in(&mut desk, 2, "discord", LEFT, 0);

    let order: Vec<_> = desk.arrangement().iter().map(|g| g.id.0).collect();

    assert_eq!(order, [2, 1]);
}

#[test]
fn slots_stay_clear_of_the_reserved_zone() {
    let mut outputs = outputs();
    outputs[0].reserved = Insets {
        left: 52,
        ..Insets::ZERO
    };
    let mut desk = Desk::new(outputs, Metrics::default());
    open_in(&mut desk, 1, "discord", LEFT, 0);

    assert_eq!(geo(&desk, 1).frame, Rect::new(60, 48, 1852, 1024));
}

#[test]
fn reserving_a_zone_moves_slots_and_floating_windows_out_of_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open(&mut desk, 2, "gimp", LEFT);
    desk.float_window(WindowId(2), LEFT, Rect::new(8, 100, 600, 400))
        .unwrap();

    let dock = Insets {
        left: 60,
        ..Insets::ZERO
    };
    desk.set_reserved(LEFT, dock).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(68, 48, 1844, 1024));
    assert_eq!(geo(&desk, 2).frame, Rect::new(68, 100, 600, 400));
}

#[test]
fn a_reserved_zone_given_back_returns_to_the_slots() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    let dock = Insets {
        left: 60,
        ..Insets::ZERO
    };
    desk.set_reserved(LEFT, dock).unwrap();

    desk.set_reserved(LEFT, Insets::ZERO).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 1904, 1024));
}

#[test]
fn a_zone_cannot_be_reserved_on_an_unknown_output() {
    let mut desk = desk();

    let result = desk.set_reserved(OutputId(9), Insets::uniform(10));

    assert_eq!(result, Err(ModelError::UnknownOutput(OutputId(9))));
}

#[test]
fn resizing_an_output_resizes_slots_and_keeps_floating_windows_inside() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open(&mut desk, 2, "gimp", LEFT);
    desk.float_window(WindowId(2), LEFT, Rect::new(1200, 600, 600, 400))
        .unwrap();

    desk.set_output_size(LEFT, Size::new(1280, 720)).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 1264, 664));
    assert_eq!(geo(&desk, 2).frame, Rect::new(672, 312, 600, 400));
}

// Asked by the dock: a screen for the next window, a habit to forget.

#[test]
fn an_application_asked_on_a_screen_opens_there_once() {
    let mut desk = desk();
    desk.open_next_on(vec![AppKey::new("firefox")], RIGHT)
        .unwrap();

    let first = open(&mut desk, 1, "firefox", LEFT);
    let second = open(&mut desk, 2, "firefox", LEFT);

    assert!(matches!(first, Placement::Floating { output: RIGHT, .. }));
    assert!(matches!(second, Placement::Floating { output: LEFT, .. }));
}

#[test]
fn a_screen_asked_once_wins_over_the_habit_and_leaves_it_alone() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();
    desk.open_next_on(vec![AppKey::new("discord")], RIGHT)
        .unwrap();

    let asked = open(&mut desk, 2, "discord", LEFT);
    let later = open(&mut desk, 3, "discord", RIGHT);

    assert!(matches!(asked, Placement::Floating { output: RIGHT, .. }));
    assert_eq!(later, Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn a_screen_asked_for_an_application_does_not_move_the_others() {
    let mut desk = desk();
    desk.open_next_on(vec![AppKey::new("firefox")], RIGHT)
        .unwrap();

    let other = open(&mut desk, 1, "kitty", LEFT);

    assert!(matches!(other, Placement::Floating { output: LEFT, .. }));
    let asked = open(&mut desk, 2, "firefox", LEFT);
    assert!(matches!(asked, Placement::Floating { output: RIGHT, .. }));
}

#[test]
fn a_screen_held_by_a_game_cannot_be_asked_for() {
    let mut desk = desk();
    open(&mut desk, 1, "steam_app_7", CENTER);
    desk.set_fullscreen(WindowId(1), None).unwrap();
    desk.note_output_used(RIGHT);
    desk.open_next_on(vec![AppKey::new("firefox")], CENTER)
        .unwrap();

    let asked = open(&mut desk, 2, "firefox", LEFT);

    assert!(matches!(asked, Placement::Floating { output: RIGHT, .. }));
}

#[test]
fn an_application_asked_for_under_several_names_opens_there_once() {
    let mut desk = desk();
    let names = vec![AppKey::new("code-oss"), AppKey::new("Code")];
    desk.open_next_on(names, RIGHT).unwrap();

    let first = open(&mut desk, 1, "code", LEFT);
    let second = open(&mut desk, 2, "code-oss", LEFT);

    assert!(matches!(first, Placement::Floating { output: RIGHT, .. }));
    assert!(matches!(second, Placement::Floating { output: LEFT, .. }));
}

#[test]
fn asking_again_for_an_application_replaces_the_screen_asked_before() {
    let mut desk = desk();
    desk.open_next_on(vec![AppKey::new("firefox")], RIGHT)
        .unwrap();
    desk.open_next_on(vec![AppKey::new("Firefox")], CENTER)
        .unwrap();

    let first = open(&mut desk, 1, "firefox", LEFT);
    let second = open(&mut desk, 2, "firefox", LEFT);

    assert!(matches!(first, Placement::Floating { output: CENTER, .. }));
    assert!(matches!(second, Placement::Floating { output: LEFT, .. }));
}

#[test]
fn asking_for_an_unknown_screen_is_refused() {
    let mut desk = desk();

    let result = desk.open_next_on(vec![AppKey::new("firefox")], OutputId(9));

    assert_eq!(result, Err(ModelError::UnknownOutput(OutputId(9))));
}

#[test]
fn a_forgotten_application_opens_floating_again() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();

    desk.forget_app(&AppKey::new("discord"));

    let reopened = open(&mut desk, 2, "discord", RIGHT);
    assert!(matches!(
        reopened,
        Placement::Floating { output: RIGHT, .. }
    ));
}

#[test]
fn a_game_opens_at_the_center_wherever_the_pointer_is() {
    let mut desk = desk();

    let game = open(&mut desk, 1, "steam_app_1245620", LEFT);

    assert!(matches!(game, Placement::Floating { output: CENTER, .. }));
}

#[test]
fn a_game_ignores_the_habit_of_its_application() {
    let mut desk = desk();
    open_in(&mut desk, 1, "steam_app_7", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();

    let game = open(&mut desk, 2, "steam_app_7", RIGHT);

    assert!(matches!(game, Placement::Floating { output: CENTER, .. }));
}

#[test]
fn a_second_game_opens_beside_the_one_being_played() {
    let mut desk = desk();
    open(&mut desk, 1, "steam_app_7", CENTER);
    desk.set_fullscreen(WindowId(1), None).unwrap();
    desk.note_output_used(LEFT);

    let second = open(&mut desk, 2, "steam_app_8", RIGHT);

    assert!(matches!(second, Placement::Floating { output: LEFT, .. }));
}

#[test]
fn windows_are_listed_from_the_one_focused_last() {
    let mut desk = desk();
    open(&mut desk, 1, "kitty", LEFT);
    open(&mut desk, 2, "firefox", LEFT);
    open(&mut desk, 3, "discord", RIGHT);
    desk.focus(WindowId(1)).unwrap();

    let recent: Vec<u64> = desk.recent_windows().map(|id| id.0).collect();

    assert_eq!(recent, [1, 3, 2]);
}

// Drop zones (rule 2).

#[test]
fn an_empty_screen_is_a_single_drop_zone() {
    let mut desk = desk();
    open(&mut desk, 1, "notes", LEFT);

    let zones = desk.drop_zones(WindowId(1));

    let slots: Vec<_> = zones.iter().map(|zone| zone.slot).collect();
    assert_eq!(slots, [slot(LEFT, 0), slot(CENTER, 0), slot(RIGHT, 0)]);
    assert_eq!(zones[0].rect, Rect::new(8, 8, 1904, 1064));
    assert!(zones.iter().all(|zone| !zone.occupied));
}

#[test]
fn a_screen_with_room_shows_its_columns_and_a_new_one_on_the_right() {
    let mut desk = desk();
    desk.set_layout(RIGHT, LayoutKind::ThreeColumns).unwrap();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open(&mut desk, 2, "notes", LEFT);

    let zones: Vec<_> = desk
        .drop_zones(WindowId(2))
        .into_iter()
        .filter(|zone| zone.slot.output == RIGHT)
        .collect();

    assert_eq!(zones.len(), 2);
    assert_eq!(zones[0].rect, Rect::new(8, 8, 948, 1064));
    assert!(zones[0].occupied);
    assert_eq!(zones[1].slot, slot(RIGHT, 1));
    assert!(!zones[1].occupied);
}

#[test]
fn a_full_screen_shows_only_its_columns() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    open(&mut desk, 3, "chat", LEFT);

    let zones = desk.drop_zones(WindowId(3));

    let right: Vec<_> = zones
        .iter()
        .filter(|zone| zone.slot.output == RIGHT)
        .collect();
    assert_eq!(right.len(), 2);
    assert!(right.iter().all(|zone| zone.occupied));
}

#[test]
fn a_zone_is_occupied_when_another_window_is_there() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    let zones = desk.drop_zones(WindowId(2));

    let occupied: Vec<_> = zones
        .iter()
        .filter(|zone| zone.occupied)
        .map(|zone| zone.slot)
        .collect();
    assert_eq!(occupied, [slot(RIGHT, 0)]);
}

// Floating windows take the size their client chooses.

#[test]
fn a_floating_window_follows_its_client_without_taking_the_focus() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    open(&mut desk, 2, "notes", RIGHT);
    let rect = Rect::new(300, 200, 640, 480);

    desk.set_floating_rect(WindowId(1), rect).unwrap();

    assert_eq!(place(&desk, 1), Placement::Floating { output: LEFT, rect });
    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn a_floating_window_is_kept_inside_its_screen_when_its_client_resizes() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);

    desk.set_floating_rect(WindowId(1), Rect::new(1500, 900, 4000, 400))
        .unwrap();

    assert_eq!(
        place(&desk, 1),
        Placement::Floating {
            output: LEFT,
            rect: Rect::new(8, 672, 1904, 400),
        }
    );
}

#[test]
fn only_floating_windows_follow_their_client() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    assert_eq!(
        desk.set_floating_rect(WindowId(1), Rect::new(0, 0, 100, 100)),
        Err(ModelError::NotFloating(WindowId(1)))
    );
    assert_eq!(
        desk.set_floating_rect(WindowId(7), Rect::new(0, 0, 100, 100)),
        Err(ModelError::UnknownWindow(WindowId(7)))
    );
}

// Minimized windows.

#[test]
fn a_minimized_window_is_hidden_and_the_previous_one_gets_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "guide", RIGHT, 0);

    desk.minimize(WindowId(2)).unwrap();

    assert!(!geo(&desk, 2).visible);
    assert!(geo(&desk, 2).minimized);
    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert_eq!(place(&desk, 2), Placement::Slot(slot(RIGHT, 0)));
}

#[test]
fn minimizing_the_shown_tab_shows_the_next_one() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 0);
    open_in(&mut desk, 3, "browser", RIGHT, 0);
    desk.focus(WindowId(2)).unwrap();

    desk.minimize(WindowId(2)).unwrap();

    assert_eq!(active_tab(&desk, RIGHT, 0), Some(3));
    assert_eq!(desk.focused(), Some(WindowId(3)));
    assert!(geo(&desk, 3).visible);
}

#[test]
fn minimizing_the_only_window_leaves_nothing_focused() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);

    desk.minimize(WindowId(1)).unwrap();

    assert_eq!(desk.focused(), None);
    assert!(!geo(&desk, 1).visible);
}

#[test]
fn the_window_minimized_last_is_restored_first() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "guide", RIGHT, 0);
    open_in(&mut desk, 3, "product", CENTER, 0);
    desk.minimize(WindowId(1)).unwrap();
    desk.minimize(WindowId(2)).unwrap();

    assert_eq!(desk.restore_last(), Some(WindowId(2)));

    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 2).minimized);
    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert_eq!(desk.restore_last(), Some(WindowId(1)));
    assert_eq!(desk.restore_last(), None);
}

// An application asks to come forward (xdg-activation).

#[test]
fn an_activated_floating_window_comes_back_from_minimized_with_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", RIGHT, 0);
    open(&mut desk, 2, "claude", LEFT);
    desk.minimize(WindowId(2)).unwrap();

    desk.activate(WindowId(2)).unwrap();

    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 2).minimized);
    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert_eq!(floating_output(place(&desk, 2)), LEFT);
}

#[test]
fn an_activated_tab_is_shown_in_its_slot() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 0);

    desk.activate(WindowId(1)).unwrap();

    assert_eq!(active_tab(&desk, RIGHT, 0), Some(1));
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn an_activated_window_shows_but_the_game_being_played_keeps_the_focus() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", RIGHT, 0);
    desk.minimize(WindowId(1)).unwrap();
    anchor_game(&mut desk, 2);

    desk.activate(WindowId(1)).unwrap();

    assert!(geo(&desk, 1).visible);
    assert_eq!(desk.focused(), Some(WindowId(2)));
    // Next in line: it has the focus once the game is gone.
    desk.close_window(WindowId(2)).unwrap();
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn activating_an_unknown_window_is_refused() {
    let mut desk = desk();

    assert!(desk.activate(WindowId(9)).is_err());
}

#[test]
fn showing_a_minimized_tab_restores_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 0);
    desk.minimize(WindowId(1)).unwrap();

    desk.activate_tab(slot(RIGHT, 0), 0).unwrap();

    assert!(geo(&desk, 1).visible);
    assert_eq!(desk.restore_last(), None);
}

#[test]
fn closing_a_minimized_window_forgets_it() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    open(&mut desk, 2, "notes", RIGHT);
    desk.minimize(WindowId(1)).unwrap();

    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(desk.restore_last(), None);
    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn a_fullscreen_window_cannot_be_minimized_by_the_shell() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);

    assert_eq!(
        desk.minimize(WindowId(1)),
        Err(ModelError::Anchored(CENTER))
    );
    assert_eq!(
        desk.minimize(WindowId(9)),
        Err(ModelError::UnknownWindow(WindowId(9)))
    );
}

#[test]
fn a_stack_never_shows_a_minimized_tab_while_another_one_could_show() {
    let mut desk = desk();
    for id in 1..=3 {
        open_in(&mut desk, id, &format!("app{id}"), RIGHT, 0);
    }
    desk.minimize(WindowId(2)).unwrap();
    desk.focus(WindowId(1)).unwrap();

    // The shown tab leaves: the minimized one is next in line, but the third
    // is the one to show.
    desk.move_to_slot(WindowId(1), slot(LEFT, 0)).unwrap();

    assert_eq!(active_tab(&desk, RIGHT, 0), Some(3));
    assert!(geo(&desk, 3).visible);
    assert!(geo(&desk, 2).minimized);
}

// Moving the separations between columns.

#[test]
fn the_separation_follows_the_pointer() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.move_separator(RIGHT, 0, Point::new(1112, 500))
        .unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 48, 1100, 1024));
    assert_eq!(desk.separators(RIGHT), [Rect::new(1108, 8, 8, 1064)]);
}

#[test]
fn a_screen_in_one_piece_has_no_separation() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    assert!(desk.separators(RIGHT).is_empty());
    assert!(desk.separators(CENTER).is_empty());
    assert!(desk.separators(OutputId(99)).is_empty());
    assert_eq!(
        desk.move_separator(RIGHT, 0, Point::new(900, 500)),
        Ok(false)
    );
}

// Windows that draw their own title bar.

#[test]
fn a_window_with_its_own_title_bar_gets_none_from_the_shell() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", LEFT, 0);

    desk.set_own_titlebar(WindowId(1), true).unwrap();

    let window = geo(&desk, 1);
    assert_eq!(window.titlebar, None);
    assert_eq!(window.client, window.frame);
    assert_eq!(window.frame, Rect::new(8, 48, 1904, 1024));
}

#[test]
fn a_floating_window_with_its_own_title_bar_is_all_client() {
    let mut desk = desk();
    open(&mut desk, 1, "steam", RIGHT);
    desk.set_own_titlebar(WindowId(1), true).unwrap();

    desk.float_window(WindowId(1), RIGHT, Rect::new(100, 100, 800, 600))
        .unwrap();

    let window = geo(&desk, 1);
    assert_eq!(window.titlebar, None);
    assert_eq!(window.client, Rect::new(100, 100, 800, 600));
}

#[test]
fn sharing_a_slot_does_not_bring_the_shell_bar_back() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", LEFT, 0);
    desk.set_own_titlebar(WindowId(1), true).unwrap();
    open_in(&mut desk, 2, "notes", LEFT, 0);
    desk.focus(WindowId(1)).unwrap();

    let window = geo(&desk, 1);

    assert_eq!(window.titlebar, None);
    assert_eq!(window.client, window.frame);
}

#[test]
fn giving_the_bar_back_to_the_shell_restores_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", LEFT, 0);
    desk.set_own_titlebar(WindowId(1), true).unwrap();

    desk.set_own_titlebar(WindowId(1), false).unwrap();

    assert!(geo(&desk, 1).titlebar.is_some());
    assert_eq!(
        desk.set_own_titlebar(WindowId(9), true),
        Err(ModelError::UnknownWindow(WindowId(9)))
    );
}

// Invariants under arbitrary sequences of operations.

/// Small deterministic generator: enough to explore, and failures reproduce.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
}

#[test]
fn any_sequence_of_operations_keeps_the_desk_consistent() {
    const OUTPUTS: [OutputId; 3] = [LEFT, CENTER, RIGHT];
    const APPS: [&str; 4] = ["discord", "browser", "code", "game"];

    for seed in 1..=8 {
        let mut rng = Lcg(seed);
        let mut desk = desk();
        let mut next_id = 0;

        for step in 0..1500 {
            let output = OUTPUTS[rng.next(3) as usize];
            let window = WindowId(rng.next(next_id + 1));
            let index = rng.next(3) as usize;

            // Refusals are part of the contract; only consistency matters here.
            let _outcome: Result<(), ModelError> = match rng.next(22) {
                0 | 1 => {
                    next_id += 1;
                    let app = AppKey::new(APPS[rng.next(4) as usize]);
                    desk.open_window(WindowId(next_id), Some(app), OpenRequest::at(output))
                        .map(drop)
                }
                2 => desk.close_window(window),
                3 => desk.move_to_slot(window, slot(output, index)),
                4 => {
                    let x = rng.next(4000) as i32 - 500;
                    let rect = Rect::new(x, x / 2, 300 + x.abs(), 200 + x.abs() / 2);
                    desk.float_window(window, output, rect)
                }
                5 => match desk.lift(window) {
                    Some(lifted) => {
                        let _ = desk.float_window(window, output, Rect::new(40, 40, 500, 400));
                        desk.put_down(lifted)
                    }
                    None => Ok(()),
                },
                6 => desk.set_layout(output, LayoutKind::ALL[rng.next(4) as usize]),
                7 => {
                    let point = Point::new(rng.next(4000) as i32 - 300, 500);
                    desk.move_separator(output, index, point).map(drop)
                }
                8 => desk.focus(window),
                9 => desk.set_anchored(window, rng.next(2) == 0),
                10 => {
                    next_id += 1;
                    let app = AppKey::new(APPS[rng.next(4) as usize]);
                    let request = OpenRequest::at(output).with_parent(window);
                    desk.open_window(WindowId(next_id), Some(app), request)
                        .map(drop)
                }
                11 => {
                    let x = rng.next(3000) as i32 - 400;
                    desk.set_floating_rect(window, Rect::new(x, x / 3, 200 + x.abs(), 150))
                }
                12 => desk.minimize(window),
                13 => {
                    desk.restore_last();
                    Ok(())
                }
                14 => desk.set_fullscreen(window, (index < 2).then_some(output)),
                15 => {
                    if let Some(game) = desk.game_to_release(output) {
                        assert!(desk.set_anchored(game, false).is_ok());
                    }
                    Ok(())
                }
                16 => desk.set_own_titlebar(window, rng.next(2) == 0),
                17 => desk.maximize(window),
                18 => desk.unmaximize(window),
                19 => desk.toggle_maximized(window),
                20 => {
                    let stack = StackId::Maximized(output);
                    desk.activate_tab(stack, index)
                        .or_else(|_| desk.cycle_tab(stack, rng.next(2) == 0))
                }
                _ => desk.cycle_tab(slot(output, index), rng.next(2) == 0),
            };

            if let Err(problem) = desk.check_invariants() {
                panic!("seed {seed}, step {step}: {problem}");
            }
        }
    }
}

// Capsules: the windows of a slot, shown above it.

fn group(desk: &Desk, output: OutputId, index: usize) -> Option<CapsuleGroup> {
    desk.capsule_groups()
        .into_iter()
        .find(|group| group.stack == StackId::Slot(slot(output, index)))
}

#[test]
fn a_floating_window_is_not_moved_by_the_capsules() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    let rect = Rect::new(100, 100, 800, 600);

    desk.float_window(WindowId(1), LEFT, rect).unwrap();

    assert_eq!(geo(&desk, 1).frame, rect);
    assert!(desk.capsule_groups().is_empty());
}

#[test]
fn every_occupied_slot_has_its_group_of_capsules() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open_in(&mut desk, 2, "wiki", LEFT, 0);
    open_in(&mut desk, 3, "notes", LEFT, 1);

    assert_eq!(
        desk.capsule_groups(),
        [
            CapsuleGroup {
                stack: StackId::Slot(slot(LEFT, 0)),
                band: Rect::new(8, 8, 948, 32),
                windows: vec![WindowId(1), WindowId(2)],
                shown: Some(WindowId(2)),
                maximized: false,
            },
            CapsuleGroup {
                stack: StackId::Slot(slot(LEFT, 1)),
                band: Rect::new(964, 8, 948, 32),
                windows: vec![WindowId(3)],
                shown: Some(WindowId(3)),
                maximized: false,
            },
        ]
    );
}

#[test]
fn a_screen_without_tidied_window_has_no_capsules() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open(&mut desk, 2, "gimp", RIGHT);

    let slots: Vec<_> = desk
        .capsule_groups()
        .iter()
        .map(|group| group.stack)
        .collect();

    assert_eq!(slots, [StackId::Slot(slot(LEFT, 0))]);
}

#[test]
fn an_anchored_screen_has_no_capsules() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "discord", LEFT, 0);
    anchor_game(&mut desk, 3);

    assert_eq!(group(&desk, CENTER, 0), None);
    assert!(group(&desk, LEFT, 0).is_some());
}

#[test]
fn a_slot_whose_windows_are_all_minimized_keeps_its_capsules() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);

    desk.minimize(WindowId(1)).unwrap();

    let group = group(&desk, LEFT, 0).unwrap();
    assert_eq!(group.windows, [WindowId(1)]);
    assert_eq!(group.shown, None);
}

#[test]
fn the_capsules_show_which_tab_took_over_from_a_minimized_one() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open_in(&mut desk, 2, "wiki", LEFT, 0);

    desk.minimize(WindowId(2)).unwrap();

    assert_eq!(group(&desk, LEFT, 0).unwrap().shown, Some(WindowId(1)));
}

#[test]
fn a_drop_zone_is_still_the_whole_slot() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open_in(&mut desk, 3, "notes", LEFT, 1);
    open(&mut desk, 2, "gimp", RIGHT);

    let zones = desk.drop_zones(WindowId(2));

    assert_eq!(zones[0].rect, Rect::new(8, 8, 948, 1064));
    assert!(zones[0].occupied);
}

// Maximized windows: over the slots of their screen, with capsules of their
// own at the top of the middle.

fn maximized_group(desk: &Desk, output: OutputId) -> Option<CapsuleGroup> {
    desk.capsule_groups()
        .into_iter()
        .find(|group| group.stack == StackId::Maximized(output))
}

fn covered(desk: &Desk, output: OutputId) -> bool {
    desk.screen(output).unwrap().covered()
}

/// Left in columns: a guide on the left, notes on the right.
fn split_left() -> Desk {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open_in(&mut desk, 2, "notes", LEFT, 1);
    desk
}

#[test]
fn a_maximized_window_leaves_its_slot_and_covers_the_screen() {
    let mut desk = split_left();

    desk.maximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    // Its column closed behind it.
    assert_eq!(desk.screen(LEFT).unwrap().slots().len(), 1);
    assert_eq!(tabs(&desk, LEFT, 0), [2]);
    assert!(covered(&desk, LEFT));
    let maximized = geo(&desk, 1);
    assert!(maximized.maximized && maximized.visible && maximized.focused);
    assert!(!maximized.floating);
    assert_eq!(maximized.frame, Rect::new(8, 48, 1904, 1024));
    assert!(!geo(&desk, 2).visible);
}

#[test]
fn the_capsules_of_maximized_windows_sit_at_the_top_of_the_middle() {
    let mut desk = split_left();

    desk.maximize(WindowId(1)).unwrap();

    let groups = desk.capsule_groups();
    assert_eq!(
        groups[0],
        CapsuleGroup {
            stack: StackId::Maximized(LEFT),
            band: Rect::new(484, 8, 952, 32),
            windows: vec![WindowId(1)],
            shown: Some(WindowId(1)),
            maximized: true,
        }
    );
    // The slots keep their capsules, nothing shown in them.
    let notes = group(&desk, LEFT, 0).unwrap();
    assert_eq!(notes.band, Rect::new(8, 8, 1904, 32));
    assert_eq!(notes.shown, None);
}

#[test]
fn a_tab_of_a_slot_brings_the_split_back_and_keeps_the_maximized_window() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.activate_tab(slot(LEFT, 0), 0).unwrap();

    assert!(!covered(&desk, LEFT));
    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    assert!(!geo(&desk, 1).visible);
    assert!(geo(&desk, 2).visible && geo(&desk, 2).focused);
    let group = maximized_group(&desk, LEFT).unwrap();
    assert_eq!(group.windows, [WindowId(1)]);
    assert_eq!(group.shown, None);
}

#[test]
fn the_capsule_of_a_maximized_window_covers_the_screen_again() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    desk.activate_tab(slot(LEFT, 0), 0).unwrap();

    desk.activate_tab(StackId::Maximized(LEFT), 0).unwrap();

    assert!(covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert!(geo(&desk, 1).visible);
}

#[test]
fn minimizing_the_only_maximized_window_brings_the_split_back() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.minimize(WindowId(1)).unwrap();

    assert!(!covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert!(geo(&desk, 2).visible);
    let group = maximized_group(&desk, LEFT).unwrap();
    assert_eq!(group.windows, [WindowId(1)]);
    assert_eq!(group.shown, None);
}

#[test]
fn minimizing_one_of_two_maximized_windows_shows_the_other() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    desk.maximize(WindowId(2)).unwrap();

    desk.minimize(WindowId(2)).unwrap();

    assert!(covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert_eq!(
        maximized_group(&desk, LEFT).unwrap().shown,
        Some(WindowId(1))
    );
}

#[test]
fn restoring_a_minimized_maximized_window_covers_the_screen_again() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    desk.minimize(WindowId(1)).unwrap();

    assert_eq!(desk.restore_last(), Some(WindowId(1)));

    assert!(covered(&desk, LEFT));
    assert!(geo(&desk, 1).visible);
}

#[test]
fn unmaximizing_puts_a_window_back_in_its_slot() {
    let mut desk = split_left();
    open_in(&mut desk, 3, "wiki", LEFT, 0);
    desk.maximize(WindowId(1)).unwrap();

    desk.unmaximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
    assert_eq!(tabs(&desk, LEFT, 0), [3, 1]);
    assert_eq!(active_tab(&desk, LEFT, 0), Some(1));
    assert!(!covered(&desk, LEFT));
    assert!(maximized_group(&desk, LEFT).is_none());
    assert!(!geo(&desk, 1).maximized);
}

#[test]
fn unmaximizing_a_window_that_floated_puts_it_back_where_it_floated() {
    let mut desk = split_left();
    open(&mut desk, 3, "gimp", LEFT);
    let rect = Rect::new(200, 150, 700, 500);
    desk.float_window(WindowId(3), LEFT, rect).unwrap();

    desk.maximize(WindowId(3)).unwrap();
    desk.unmaximize(WindowId(3)).unwrap();

    assert_eq!(place(&desk, 3), Placement::Floating { output: LEFT, rect });
    assert_eq!(desk.memory().get(&AppKey::new("gimp")), None);
}

#[test]
fn a_maximized_window_goes_back_to_the_last_slot_when_its_own_is_gone() {
    let mut desk = split_left();
    desk.maximize(WindowId(2)).unwrap();
    desk.set_layout(LEFT, LayoutKind::Full).unwrap();

    desk.unmaximize(WindowId(2)).unwrap();

    assert_eq!(place(&desk, 2), Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn a_maximized_window_reopens_its_column_at_its_place() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    assert_eq!(desk.screen(LEFT).unwrap().slots().len(), 1);

    desk.unmaximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
    assert_eq!(tabs(&desk, LEFT, 1), [2]);
}

#[test]
fn a_maximized_tab_goes_back_to_its_column_as_a_tab() {
    let mut desk = desk();
    desk.set_layout(LEFT, LayoutKind::ThreeColumns).unwrap();
    open_in(&mut desk, 1, "guide", LEFT, 0);
    open_in(&mut desk, 2, "wiki", LEFT, 0);
    open_in(&mut desk, 3, "notes", LEFT, 1);
    desk.maximize(WindowId(2)).unwrap();

    desk.unmaximize(WindowId(2)).unwrap();

    assert_eq!(tabs(&desk, LEFT, 0), [1, 2]);
    assert_eq!(desk.screen(LEFT).unwrap().slots().len(), 2);
}

#[test]
fn a_maximized_window_restored_on_an_empty_screen_opens_a_column() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);
    desk.move_to_slot(WindowId(1), slot(LEFT, 0)).unwrap();
    open_in(&mut desk, 2, "notes", LEFT, 1);
    desk.maximize(WindowId(2)).unwrap();
    desk.close_window(WindowId(1)).unwrap();

    desk.unmaximize(WindowId(2)).unwrap();

    assert_eq!(place(&desk, 2), Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn a_screen_without_column_takes_maximized_windows_too() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", LEFT);

    desk.maximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    let groups = desk.capsule_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].stack, StackId::Maximized(LEFT));
}

#[test]
fn a_window_alone_in_the_slot_of_a_full_screen_already_has_it_all() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);

    desk.maximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(CENTER, 0)));
    assert!(!covered(&desk, CENTER));
    assert!(desk.is_maximized(WindowId(1)));
    assert!(geo(&desk, 1).maximized);
}

#[test]
fn a_window_alone_on_a_screen_of_columns_counts_as_maximized() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);

    let alone = geo(&desk, 1);
    assert!(alone.maximized && !alone.floating);
    assert!(desk.is_maximized(WindowId(1)));
    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
    // Its capsules are where those of maximized windows go.
    let group = group(&desk, LEFT, 0).unwrap();
    assert!(group.maximized);
    assert_eq!(group.stack, StackId::Slot(slot(LEFT, 0)));
    let screen = desk.screen(LEFT).unwrap();
    assert_eq!(group.band, screen.maximized_parts(desk.metrics()).0);
}

#[test]
fn every_tab_of_the_only_column_counts_as_maximized() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", CENTER, 0);
    open_in(&mut desk, 2, "wiki", CENTER, 0);

    assert!(geo(&desk, 1).maximized && geo(&desk, 2).maximized);
    assert!(group(&desk, CENTER, 0).unwrap().maximized);
}

#[test]
fn sharing_the_screen_is_not_being_maximized() {
    let mut desk = split_left();

    for id in [1, 2] {
        assert!(!geo(&desk, id).maximized);
        assert!(!desk.is_maximized(WindowId(id)));
    }
    assert!(!group(&desk, LEFT, 0).unwrap().maximized);
    assert_eq!(
        group(&desk, LEFT, 0).unwrap().band,
        Rect::new(8, 8, 948, 32)
    );

    // The other column closes: the one left has the whole screen.
    desk.close_window(WindowId(2)).unwrap();
    assert!(geo(&desk, 1).maximized);
}

#[test]
fn the_column_left_under_a_maximized_window_is_not_maximized() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    assert!(!geo(&desk, 2).maximized);
    assert!(!desk.is_maximized(WindowId(2)));
    assert!(!group(&desk, LEFT, 0).unwrap().maximized);
}

#[test]
fn restoring_the_window_of_the_only_column_floats_it() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", LEFT, 0);

    desk.toggle_maximized(WindowId(1)).unwrap();

    assert_eq!(floating_output(place(&desk, 1)), LEFT);
    assert!(desk.screen(LEFT).unwrap().slots().is_empty());
    assert!(!desk.is_maximized(WindowId(1)));
    let floating = geo(&desk, 1);
    assert!(floating.floating && floating.focused && !floating.maximized);
    assert!(desk.memory().is_empty());
}

#[test]
fn unmaximizing_a_window_that_is_not_maximized_changes_nothing() {
    let mut desk = split_left();

    desk.unmaximize(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn toggling_maximizes_then_restores() {
    let mut desk = split_left();

    desk.toggle_maximized(WindowId(2)).unwrap();
    assert!(desk.is_maximized(WindowId(2)));

    desk.toggle_maximized(WindowId(2)).unwrap();
    assert_eq!(place(&desk, 2), Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn dragging_a_maximized_window_to_a_slot_takes_it_out() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.move_to_slot(WindowId(1), slot(RIGHT, 0)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(RIGHT, 0)));
    assert!(!covered(&desk, LEFT));
    assert_eq!(
        desk.memory().get(&AppKey::new("guide")),
        Some(RememberedPlace::in_slot(OutputRole::Right, 0))
    );
}

#[test]
fn a_maximized_window_dragged_by_its_title_bar_floats_without_habit() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.float_window(WindowId(1), LEFT, Rect::new(100, 100, 600, 400))
        .unwrap();

    assert_eq!(floating_output(place(&desk, 1)), LEFT);
    assert!(!covered(&desk, LEFT));
    assert_eq!(desk.memory().get(&AppKey::new("guide")), None);
}

#[test]
fn closing_the_maximized_window_brings_the_split_back() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.close_window(WindowId(1)).unwrap();

    assert!(!covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn closing_one_of_two_maximized_windows_shows_the_other() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    desk.maximize(WindowId(2)).unwrap();

    desk.close_window(WindowId(2)).unwrap();

    assert!(covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn a_maximized_window_can_go_fullscreen_and_come_back() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();

    desk.set_anchored(WindowId(1), true).unwrap();
    assert!(geo(&desk, 1).anchored && !geo(&desk, 1).maximized);
    desk.set_anchored(WindowId(1), false).unwrap();

    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    assert!(geo(&desk, 1).maximized);
}

#[test]
fn maximizing_on_an_anchored_screen_is_refused() {
    let mut desk = desk();
    open_in(&mut desk, 1, "game", LEFT, 0);
    open_in(&mut desk, 2, "notes", LEFT, 1);
    desk.set_anchored(WindowId(1), true).unwrap();

    assert_eq!(desk.maximize(WindowId(2)), Err(ModelError::Anchored(LEFT)));
}

#[test]
fn floating_windows_stay_above_a_maximized_one() {
    let mut desk = split_left();
    open(&mut desk, 3, "gimp", LEFT);
    desk.maximize(WindowId(1)).unwrap();

    let order: Vec<_> = desk.arrangement().iter().map(|g| g.id.0).collect();
    let rank = |id| order.iter().position(|window| *window == id).unwrap();

    assert!(rank(1) < rank(3));
    assert!(geo(&desk, 3).visible);
}

#[test]
fn maximizing_is_remembered_with_the_slot_to_go_back_to() {
    let mut desk = split_left();

    desk.maximize(WindowId(2)).unwrap();

    assert_eq!(
        desk.memory().get(&AppKey::new("notes")),
        Some(RememberedPlace::maximized(OutputRole::Left, Some(1)))
    );

    desk.unmaximize(WindowId(2)).unwrap();

    assert_eq!(
        desk.memory().get(&AppKey::new("notes")),
        Some(RememberedPlace::in_slot(OutputRole::Left, 1))
    );
}

#[test]
fn an_app_closed_maximized_reopens_maximized_and_restores_to_its_slot() {
    let mut desk = split_left();
    desk.maximize(WindowId(2)).unwrap();
    desk.close_window(WindowId(2)).unwrap();

    let placement = open(&mut desk, 3, "notes", RIGHT);

    assert_eq!(placement, Placement::Maximized(LEFT));
    assert!(covered(&desk, LEFT));
    assert_eq!(desk.focused(), Some(WindowId(3)));

    desk.unmaximize(WindowId(3)).unwrap();
    assert_eq!(place(&desk, 3), Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn an_app_that_floated_before_maximizing_reopens_maximized_then_floats() {
    let mut desk = desk();
    open(&mut desk, 1, "gimp", RIGHT);
    desk.maximize(WindowId(1)).unwrap();
    desk.close_window(WindowId(1)).unwrap();

    assert_eq!(
        open(&mut desk, 2, "gimp", LEFT),
        Placement::Maximized(RIGHT)
    );
    desk.unmaximize(WindowId(2)).unwrap();

    assert_eq!(floating_output(place(&desk, 2)), RIGHT);
    assert_eq!(desk.memory().get(&AppKey::new("gimp")), None);
}

#[test]
fn an_app_reopening_maximized_during_a_game_shows_without_the_focus() {
    let mut desk = split_left();
    desk.maximize(WindowId(2)).unwrap();
    desk.close_window(WindowId(2)).unwrap();
    anchor_game(&mut desk, 3);

    assert_eq!(
        open(&mut desk, 4, "notes", CENTER),
        Placement::Maximized(LEFT)
    );

    assert_eq!(desk.focused(), Some(WindowId(3)));
    assert!(geo(&desk, 4).visible);
}

#[test]
fn a_maximized_habit_on_an_anchored_screen_opens_floating_elsewhere() {
    let mut desk = desk();
    open_in(&mut desk, 1, "notes", CENTER, 0);
    open(&mut desk, 2, "gimp", CENTER);
    desk.maximize(WindowId(2)).unwrap();
    desk.close_window(WindowId(2)).unwrap();
    desk.set_anchored(WindowId(1), true).unwrap();
    desk.note_output_used(RIGHT);

    let placement = open(&mut desk, 3, "gimp", CENTER);

    assert_eq!(floating_output(placement), RIGHT);
}

#[test]
fn cycling_maximized_tabs_shows_each_in_turn() {
    let mut desk = split_left();
    desk.maximize(WindowId(1)).unwrap();
    desk.maximize(WindowId(2)).unwrap();

    desk.cycle_tab(StackId::Maximized(LEFT), true).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert!(geo(&desk, 1).visible && !geo(&desk, 2).visible);
}

// A laptop: one screen, a game on it.

fn laptop() -> Desk {
    let screen = OutputInfo::new(CENTER, OutputRole::Center, Size::new(1920, 1080));
    Desk::new(vec![screen], Metrics::default())
}

/// A game holds the lone screen, and Discord opens while it is played.
fn playing_with_discord() -> Desk {
    let mut desk = laptop();
    open(&mut desk, 1, "steam_app_1", CENTER);
    desk.set_anchored(WindowId(1), true).unwrap();
    open(&mut desk, 2, "discord", CENTER);
    desk
}

#[test]
fn a_window_opened_during_a_game_on_a_lone_screen_stays_behind_it() {
    let desk = playing_with_discord();

    assert!(!geo(&desk, 2).visible);
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn a_window_brought_from_the_dock_comes_over_the_game() {
    let mut desk = playing_with_discord();

    desk.bring(WindowId(2)).unwrap();

    assert!(geo(&desk, 2).visible);
    assert_eq!(desk.focused(), Some(WindowId(2)));
    // Still fullscreen, under the window brought over it.
    assert!(geo(&desk, 1).anchored && geo(&desk, 1).visible);
    let order: Vec<WindowId> = desk
        .arrangement()
        .iter()
        .map(|geometry| geometry.id)
        .collect();
    assert_eq!(order, [WindowId(1), WindowId(2)]);
}

#[test]
fn clicking_the_game_puts_it_back_in_front() {
    let mut desk = playing_with_discord();
    desk.bring(WindowId(2)).unwrap();

    desk.focus(WindowId(1)).unwrap();

    assert!(!geo(&desk, 2).visible);
    assert!(!desk.is_over_game(WindowId(2)));
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn once_the_game_is_over_nothing_is_over_it() {
    let mut desk = playing_with_discord();
    desk.bring(WindowId(2)).unwrap();

    desk.set_anchored(WindowId(1), false).unwrap();

    assert!(!desk.is_over_game(WindowId(2)));
    assert!(geo(&desk, 2).visible);
    // Played again: Discord does not come back over it by itself.
    desk.set_anchored(WindowId(1), true).unwrap();
    assert!(!geo(&desk, 2).visible);
}

#[test]
fn bringing_a_window_no_game_hides_only_focuses_it() {
    let mut desk = desk();
    open(&mut desk, 1, "kitty", LEFT);
    open(&mut desk, 2, "discord", LEFT);

    desk.bring(WindowId(1)).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert!(!desk.is_over_game(WindowId(1)));
    assert_eq!(
        desk.bring(WindowId(9)),
        Err(ModelError::UnknownWindow(WindowId(9)))
    );
}

// Changing one's mind while dragging a window: it goes back where it was.

/// The user drags window `id` by its title bar onto `output`: it floats.
fn drag_out(desk: &mut Desk, id: u64, output: OutputId) -> Lifted {
    let lifted = desk.lift(WindowId(id)).unwrap();
    desk.float_window(WindowId(id), output, Rect::new(100, 100, 800, 600))
        .unwrap();
    lifted
}

#[test]
fn a_window_put_down_goes_back_to_its_column_and_keeps_its_habit() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "browser", LEFT, 1);
    let habit = desk.memory().get(&AppKey::new("discord"));

    let lifted = drag_out(&mut desk, 1, CENTER);
    assert_eq!(desk.memory().get(&AppKey::new("discord")), None);
    assert_eq!(tabs(&desk, LEFT, 0), [2]);
    desk.put_down(lifted).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 0)));
    assert_eq!(tabs(&desk, LEFT, 1), [2]);
    assert_eq!(desk.memory().get(&AppKey::new("discord")), habit);
    assert_eq!(desk.focused(), Some(WindowId(1)));
}

#[test]
fn a_tab_put_down_goes_back_to_its_stack() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 0);

    let lifted = drag_out(&mut desk, 2, RIGHT);
    desk.put_down(lifted).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [1, 2]);
    assert_eq!(active_tab(&desk, RIGHT, 0), Some(2));
}

#[test]
fn a_maximized_window_put_down_covers_its_screen_again() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.maximize(WindowId(1)).unwrap();

    let lifted = drag_out(&mut desk, 1, RIGHT);
    desk.put_down(lifted).unwrap();

    assert_eq!(place(&desk, 1), Placement::Maximized(RIGHT));
    assert!(geo(&desk, 1).visible);
    assert!(!geo(&desk, 2).visible);
    // Restored, it still goes back to its column.
    desk.unmaximize(WindowId(1)).unwrap();
    assert_eq!(place(&desk, 1), Placement::Slot(slot(RIGHT, 0)));
}

#[test]
fn a_floating_window_put_down_goes_back_to_where_it_floated() {
    let mut desk = desk();
    let request = OpenRequest::at(RIGHT).with_size(Size::new(400, 300));
    let rect = open_sized(&mut desk, 1, Some("gimp"), request);

    let lifted = drag_out(&mut desk, 1, LEFT);
    desk.put_down(lifted).unwrap();

    assert_eq!(
        place(&desk, 1),
        Placement::Floating {
            output: RIGHT,
            rect
        }
    );
    assert_eq!(desk.memory().get(&AppKey::new("gimp")), None);
}

// Spatial mode

#[test]
fn spatial_normal_windows_open_maximized_edge_to_edge() {
    let mut memory = AppMemory::default();
    memory.remember(
        AppKey::new("kitty"),
        RememberedPlace::in_slot(OutputRole::Right, 1),
    );
    let mut desk = desk().with_memory(memory).with_spatial(true);
    assert!(desk.is_spatial());

    let placement = desk
        .open_window(
            WindowId(1),
            Some(AppKey::new("kitty")),
            OpenRequest::at(LEFT).with_size(Size::new(600, 400)),
        )
        .unwrap();

    assert_eq!(placement, Placement::Maximized(LEFT));
    let g = geo(&desk, 1);
    assert_eq!(g.frame, desk.screen(LEFT).unwrap().info().usable());
    assert!(g.maximized);
    assert!(!g.floating);
    assert!(g.visible);

    // Classic unmaximize / float / move_to_slot are no-ops for maximized windows.
    desk.unmaximize(WindowId(1)).unwrap();
    desk.toggle_maximized(WindowId(1)).unwrap();
    desk.float_window(WindowId(1), LEFT, Rect::new(10, 10, 400, 300))
        .unwrap();
    desk.move_to_slot(WindowId(1), slot(RIGHT, 0)).unwrap();
    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    desk.check_invariants().unwrap();
}

#[test]
fn spatial_dialogs_stay_floating_centered_over_parent() {
    let mut desk = desk().with_spatial(true);
    open(&mut desk, 1, "gimp", CENTER);

    let placement = desk
        .open_window(
            WindowId(2),
            Some(AppKey::new("gimp")),
            OpenRequest::at(CENTER)
                .with_parent(WindowId(1))
                .with_size(Size::new(400, 300)),
        )
        .unwrap();

    assert_eq!(floating_output(placement), CENTER);
    let g = geo(&desk, 2);
    assert!(g.floating);
    assert!(!g.maximized);

    // Floating dialogs can still be moved.
    let moved = Rect::new(100, 100, 400, 300);
    desk.float_window(WindowId(2), CENTER, moved).unwrap();
    assert_eq!(
        place(&desk, 2),
        Placement::Floating {
            output: CENTER,
            rect: moved,
        }
    );
    desk.check_invariants().unwrap();
}

#[test]
fn spatial_has_no_capsules_drop_zones_or_separators() {
    let mut desk = desk().with_spatial(true);
    open(&mut desk, 1, "kitty", LEFT);
    open(&mut desk, 2, "firefox", CENTER);

    assert!(desk.capsule_groups().is_empty());
    assert!(desk.drop_zones(WindowId(1)).is_empty());
    assert!(desk.separators(LEFT).is_empty());
}

#[test]
fn move_to_screen_teleports_and_reveals_fallback() {
    let mut desk = desk().with_spatial(true);
    open(&mut desk, 1, "kitty", LEFT);
    open(&mut desk, 2, "firefox", LEFT);
    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 1).visible);

    // Teleport window 2 to CENTER: window 1 is revealed on LEFT.
    desk.move_to_screen(WindowId(2), CENTER).unwrap();
    assert_eq!(place(&desk, 2), Placement::Maximized(CENTER));
    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert!(geo(&desk, 1).visible);
    assert!(geo(&desk, 2).visible);

    // Minimize window 2, then move_to_screen on its own screen: unminimizes and focuses.
    desk.minimize(WindowId(2)).unwrap();
    assert!(geo(&desk, 2).minimized);
    desk.move_to_screen(WindowId(2), CENTER).unwrap();
    assert!(!geo(&desk, 2).minimized);
    assert_eq!(desk.focused(), Some(WindowId(2)));

    // Teleport the last window off LEFT: LEFT becomes empty (revealing spatial home).
    desk.move_to_screen(WindowId(1), CENTER).unwrap();
    let left = desk.screen(LEFT).unwrap();
    assert!(left.maximized().is_empty());
    assert!(!left.covered());
    desk.check_invariants().unwrap();
}

#[test]
fn move_to_screen_rejects_anchored_target() {
    let mut desk = desk().with_spatial(true);
    open(&mut desk, 1, "kitty", LEFT);
    open(&mut desk, 2, "game", CENTER);
    desk.set_anchored(WindowId(2), true).unwrap();

    assert_eq!(
        desk.move_to_screen(WindowId(1), CENTER),
        Err(ModelError::Anchored(CENTER))
    );
    assert_eq!(place(&desk, 1), Placement::Maximized(LEFT));
    desk.check_invariants().unwrap();
}

#[test]
fn spatial_game_mode_and_rule_7() {
    let mut desk = desk().with_spatial(true);
    // Steam game opens on CENTER even when cursor is on LEFT.
    let game_place = open(&mut desk, 1, "steam_app_123", LEFT);
    assert_eq!(game_place, Placement::Maximized(CENTER));
    desk.set_anchored(WindowId(1), true).unwrap();

    // New window opened with cursor on CENTER goes to side screen, game keeps focus.
    let side_place = open(&mut desk, 2, "discord", CENTER);
    assert_eq!(side_place, Placement::Maximized(LEFT));
    assert_eq!(desk.focused(), Some(WindowId(1)));
    assert!(geo(&desk, 1).anchored);
    assert!(geo(&desk, 2).visible);
    desk.check_invariants().unwrap();
}
