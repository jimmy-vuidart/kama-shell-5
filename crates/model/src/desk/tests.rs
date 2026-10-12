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

/// Left 16:9 in `Columns`, center 21:9 in `Full`, right 16:9 in `Columns`.
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
        Placement::Slot(slot) => panic!("expected a floating window, got {slot:?}"),
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
    open(&mut desk, 1, "guide", CENTER);

    desk.move_to_slot(WindowId(1), slot(RIGHT, 1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(RIGHT, 1)));
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
        open_in(&mut desk, id, "browser", RIGHT, 1);
    }

    assert_eq!(tabs(&desk, RIGHT, 1), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn moving_to_another_slot_leaves_the_previous_one() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    desk.move_to_slot(WindowId(1), slot(LEFT, 1)).unwrap();

    assert!(tabs(&desk, RIGHT, 0).is_empty());
    assert_eq!(tabs(&desk, LEFT, 1), [1]);
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
    open(&mut desk, 1, "discord", RIGHT);
    desk.move_to_slot(WindowId(1), slot(LEFT, 1)).unwrap();
    desk.close_window(WindowId(1)).unwrap();

    let placement = open(&mut desk, 2, "discord", CENTER);

    assert_eq!(placement, Placement::Slot(slot(LEFT, 1)));
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
    open_in(&mut desk, 1, "notes", RIGHT, 1);
    desk.close_window(WindowId(1)).unwrap();
    open_in(&mut desk, 2, "guide", RIGHT, 0);
    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();

    let placement = open(&mut desk, 3, "notes", LEFT);

    assert_eq!(placement, Placement::Slot(slot(RIGHT, 0)));
    assert_eq!(tabs(&desk, RIGHT, 0), [2, 3]);
}

#[test]
fn an_app_remembered_on_a_free_screen_floats_on_that_screen() {
    let mut desk = desk();
    open_in(&mut desk, 1, "notes", LEFT, 0);
    desk.close_window(WindowId(1)).unwrap();
    desk.set_layout(LEFT, LayoutKind::Free).unwrap();

    let placement = open(&mut desk, 2, "notes", RIGHT);

    assert_eq!(floating_output(placement), LEFT);
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
        RememberedPlace {
            role: OutputRole::Right,
            slot: 0,
        },
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
        RememberedPlace {
            role: OutputRole::Right,
            slot: 0,
        },
    );
    let only_center = vec![OutputInfo::new(CENTER, OutputRole::Center, CENTER_SIZE)];
    let mut desk = Desk::new(only_center, Metrics::default()).with_memory(memory);

    let placement = open(&mut desk, 1, "code", CENTER);

    assert_eq!(floating_output(placement), CENTER);
    assert_eq!(desk.memory().len(), 1);
}

// Rule 5: swap with the center.

#[test]
fn swapping_exchanges_the_side_window_and_the_center_window() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "editor", RIGHT, 0);

    desk.swap_with_center(WindowId(2)).unwrap();

    assert_eq!(place(&desk, 2), Placement::Slot(slot(CENTER, 0)));
    assert_eq!(place(&desk, 1), Placement::Slot(slot(RIGHT, 0)));
    assert_eq!(desk.focused(), Some(WindowId(2)));
}

#[test]
fn swapping_with_an_empty_center_moves_the_window() {
    let mut desk = desk();
    open_in(&mut desk, 1, "editor", RIGHT, 1);

    desk.swap_with_center(WindowId(1)).unwrap();

    assert_eq!(place(&desk, 1), Placement::Slot(slot(CENTER, 0)));
    assert!(tabs(&desk, RIGHT, 1).is_empty());
}

#[test]
fn swapping_a_tab_leaves_its_neighbours_in_place() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "discord", RIGHT, 0);
    open_in(&mut desk, 3, "editor", RIGHT, 0);

    desk.swap_with_center(WindowId(3)).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [2, 1]);
    assert_eq!(active_tab(&desk, RIGHT, 0), Some(1));
    assert_eq!(tabs(&desk, CENTER, 0), [3]);
}

#[test]
fn swapping_only_takes_the_active_center_tab() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "admin", CENTER, 0);
    open_in(&mut desk, 3, "editor", RIGHT, 0);

    desk.swap_with_center(WindowId(3)).unwrap();

    assert_eq!(tabs(&desk, CENTER, 0), [1, 3]);
    assert_eq!(active_tab(&desk, CENTER, 0), Some(3));
    assert_eq!(tabs(&desk, RIGHT, 0), [2]);
}

#[test]
fn swapping_a_floating_window_gives_its_rect_to_the_center_window() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open(&mut desk, 2, "editor", RIGHT);
    let rect = Rect::new(100, 120, 900, 700);
    desk.float_window(WindowId(2), RIGHT, rect).unwrap();

    desk.swap_with_center(WindowId(2)).unwrap();

    assert_eq!(place(&desk, 2), Placement::Slot(slot(CENTER, 0)));
    assert_eq!(
        place(&desk, 1),
        Placement::Floating {
            output: RIGHT,
            rect,
        }
    );
}

#[test]
fn swapping_is_refused_while_a_game_is_anchored() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open_in(&mut desk, 2, "discord", LEFT, 0);

    let result = desk.swap_with_center(WindowId(2));

    assert_eq!(result, Err(ModelError::Anchored(CENTER)));
    assert_eq!(place(&desk, 2), Placement::Slot(slot(LEFT, 0)));
    assert_eq!(place(&desk, 1), Placement::Slot(slot(CENTER, 0)));
}

#[test]
fn swapping_a_window_already_at_the_center_is_refused() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);

    let result = desk.swap_with_center(WindowId(1));

    assert_eq!(result, Err(ModelError::NotOnSideScreen(WindowId(1))));
}

#[test]
fn swapping_without_a_center_screen_is_refused() {
    let only_left = vec![OutputInfo::new(LEFT, OutputRole::Left, SIDE_SIZE)];
    let mut desk = Desk::new(only_left, Metrics::default());
    open_in(&mut desk, 1, "editor", LEFT, 0);

    assert_eq!(
        desk.swap_with_center(WindowId(1)),
        Err(ModelError::NoCenter)
    );
}

#[test]
fn swapping_does_not_change_habits() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "editor", RIGHT, 0);
    let before = desk.memory().clone();

    desk.swap_with_center(WindowId(2)).unwrap();

    assert_eq!(desk.memory(), &before);
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
        Some(RememberedPlace {
            role: OutputRole::Center,
            slot: 0,
        })
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
    assert!(zones.iter().all(|zone| match zone.target {
        DropTarget::Slot(slot) => slot.output != CENTER,
        DropTarget::Float(output) => output != CENTER,
    }));
}

#[test]
fn shell_gestures_are_refused_on_an_anchored_screen() {
    let mut desk = desk();
    anchor_game(&mut desk, 1);
    open(&mut desk, 2, "notes", LEFT);

    assert_eq!(
        desk.set_layout(CENTER, LayoutKind::Columns),
        Err(ModelError::Anchored(CENTER))
    );
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
    assert_eq!(game.frame, Rect::new(8, 8, 3424, 1424));
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
    open_in(&mut desk, 1, "video", LEFT, 1);

    desk.set_fullscreen(WindowId(1), None).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::from_size(SIDE_SIZE));
    assert_eq!(place(&desk, 1), Placement::Slot(slot(LEFT, 1)));
}

#[test]
fn going_fullscreen_on_its_own_screen_keeps_the_place_of_the_window() {
    let mut desk = desk();
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
fn the_swap_button_is_offered_on_side_screens_only_without_a_game() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "browser", CENTER, 0);
    assert!(desk.can_swap_with_center(WindowId(1)));
    assert!(!desk.can_swap_with_center(WindowId(2)));
    assert!(!desk.can_swap_with_center(WindowId(99)));

    desk.set_anchored(WindowId(2), true).unwrap();

    assert!(!desk.can_swap_with_center(WindowId(1)));
}

#[test]
fn a_window_under_a_side_game_cannot_be_swapped() {
    let mut desk = desk();
    open_in(&mut desk, 1, "discord", LEFT, 0);
    open_in(&mut desk, 2, "video", LEFT, 1);

    desk.set_anchored(WindowId(2), true).unwrap();

    assert!(!desk.can_swap_with_center(WindowId(1)));
    assert!(!desk.can_swap_with_center(WindowId(2)));
}

#[test]
fn an_anchored_screen_has_no_separation_to_grab() {
    let mut desk = desk();
    open_in(&mut desk, 1, "video", RIGHT, 0);
    assert!(desk.separator(RIGHT).is_some());

    desk.set_anchored(WindowId(1), true).unwrap();

    assert_eq!(desk.separator(RIGHT), None);
    assert_eq!(desk.ratio_at(RIGHT, Point::new(900, 500)), None);
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

    desk.set_layout(RIGHT, LayoutKind::Tabs).unwrap();

    assert_eq!(desk.focused(), Some(WindowId(2)));
    assert!(geo(&desk, 2).visible);
    assert!(!geo(&desk, 1).visible);
}

#[test]
fn going_from_full_to_columns_leaves_the_second_slot_empty() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);
    open_in(&mut desk, 2, "admin", CENTER, 0);

    desk.set_layout(CENTER, LayoutKind::Columns).unwrap();

    assert_eq!(tabs(&desk, CENTER, 0), [1, 2]);
    assert!(tabs(&desk, CENTER, 1).is_empty());
}

#[test]
fn going_from_columns_to_rows_keeps_each_slot() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.set_layout(RIGHT, LayoutKind::Rows).unwrap();

    assert_eq!(tabs(&desk, RIGHT, 0), [1]);
    assert_eq!(tabs(&desk, RIGHT, 1), [2]);
    assert_eq!(geo(&desk, 2).frame, Rect::new(8, 544, 1904, 528));
}

#[test]
fn going_to_free_floats_every_window_where_it_was() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    let guide = geo(&desk, 1).frame;
    let notes = geo(&desk, 2).frame;

    desk.set_layout(RIGHT, LayoutKind::Free).unwrap();

    assert!(desk.screen(RIGHT).unwrap().slots().is_empty());
    assert_eq!(
        place(&desk, 1),
        Placement::Floating {
            output: RIGHT,
            rect: guide,
        }
    );
    assert_eq!(
        place(&desk, 2),
        Placement::Floating {
            output: RIGHT,
            rect: notes,
        }
    );
}

#[test]
fn leaving_free_does_not_tidy_anything() {
    let mut desk = desk();
    open(&mut desk, 1, "blender", LEFT);
    desk.set_layout(LEFT, LayoutKind::Free).unwrap();

    desk.set_layout(LEFT, LayoutKind::Columns).unwrap();

    assert!(matches!(place(&desk, 1), Placement::Floating { .. }));
    assert_eq!(desk.screen(LEFT).unwrap().slots().len(), 2);
    assert!(tabs(&desk, LEFT, 0).is_empty());
}

#[test]
fn two_thirds_one_third_is_refused_on_a_side_screen() {
    let mut desk = desk();

    let result = desk.set_layout(LEFT, LayoutKind::TwoThirdsOneThird);

    assert_eq!(
        result,
        Err(ModelError::LayoutNotAllowed {
            kind: LayoutKind::TwoThirdsOneThird,
            role: OutputRole::Left,
        })
    );
    assert_eq!(desk.screen(LEFT).unwrap().layout(), LayoutKind::Columns);
}

#[test]
fn the_center_accepts_two_thirds_one_third() {
    let mut desk = desk();
    open_in(&mut desk, 1, "product", CENTER, 0);

    desk.set_layout(CENTER, LayoutKind::TwoThirdsOneThird)
        .unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 8, 2277, 1424));
}

#[test]
fn the_ratio_moves_the_separation_and_survives_a_layout_change() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);

    desk.set_ratio(RIGHT, Ratio::from_percent(58)).unwrap();
    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 8, 1100, 1064));
    assert_eq!(geo(&desk, 2).frame, Rect::new(1116, 8, 796, 1064));

    desk.set_layout(RIGHT, LayoutKind::Full).unwrap();
    desk.set_layout(RIGHT, LayoutKind::Columns).unwrap();
    assert_eq!(desk.screen(RIGHT).unwrap().ratio(), Ratio::from_percent(58));
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
            slot: slot(RIGHT, 0),
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
    assert_eq!(window.frame, Rect::new(8, 8, 948, 1064));
    assert_eq!(window.titlebar, Some(Rect::new(8, 8, 948, 38)));
    assert_eq!(window.client, Rect::new(8, 46, 948, 1026));
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

    assert_eq!(geo(&desk, 1).frame, Rect::new(60, 8, 922, 1064));
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

    assert_eq!(geo(&desk, 1).frame, Rect::new(68, 8, 918, 1064));
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

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 8, 948, 1064));
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

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 8, 628, 704));
    assert_eq!(geo(&desk, 2).frame, Rect::new(672, 312, 600, 400));
}

// Asked by the dock: a screen for the next window, a habit to forget.

#[test]
fn an_application_asked_on_a_screen_opens_there_once() {
    let mut desk = desk();
    desk.open_next_on(AppKey::new("firefox"), RIGHT).unwrap();

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
    desk.open_next_on(AppKey::new("discord"), RIGHT).unwrap();

    let asked = open(&mut desk, 2, "discord", LEFT);
    let later = open(&mut desk, 3, "discord", RIGHT);

    assert!(matches!(asked, Placement::Floating { output: RIGHT, .. }));
    assert_eq!(later, Placement::Slot(slot(LEFT, 0)));
}

#[test]
fn a_screen_asked_for_an_application_does_not_move_the_others() {
    let mut desk = desk();
    desk.open_next_on(AppKey::new("firefox"), RIGHT).unwrap();

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
    desk.open_next_on(AppKey::new("firefox"), CENTER).unwrap();

    let asked = open(&mut desk, 2, "firefox", LEFT);

    assert!(matches!(asked, Placement::Floating { output: RIGHT, .. }));
}

#[test]
fn asking_for_an_unknown_screen_is_refused() {
    let mut desk = desk();

    let result = desk.open_next_on(AppKey::new("firefox"), OutputId(9));

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
fn every_slot_of_every_screen_is_a_drop_zone() {
    let mut desk = desk();
    open(&mut desk, 1, "notes", LEFT);

    let zones = desk.drop_zones(WindowId(1));

    let targets: Vec<_> = zones.iter().map(|zone| zone.target).collect();
    assert_eq!(
        targets,
        [
            DropTarget::Slot(slot(LEFT, 0)),
            DropTarget::Slot(slot(LEFT, 1)),
            DropTarget::Slot(slot(CENTER, 0)),
            DropTarget::Slot(slot(RIGHT, 0)),
            DropTarget::Slot(slot(RIGHT, 1)),
        ]
    );
    assert_eq!(zones[0].rect, Rect::new(8, 8, 948, 1064));
    assert!(zones.iter().all(|zone| !zone.occupied));
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
        .map(|zone| zone.target)
        .collect();
    assert_eq!(occupied, [DropTarget::Slot(slot(RIGHT, 0))]);
}

#[test]
fn a_free_screen_is_a_single_floating_drop_zone() {
    let mut desk = desk();
    desk.set_layout(LEFT, LayoutKind::Free).unwrap();
    open(&mut desk, 1, "notes", RIGHT);

    let zones = desk.drop_zones(WindowId(1));

    assert_eq!(
        zones[0],
        DropZone {
            target: DropTarget::Float(LEFT),
            rect: Rect::from_size(SIDE_SIZE),
            occupied: false,
        }
    );
}

// Suggestions for the placement bar.

#[test]
fn the_first_free_slot_of_the_screen_is_suggested() {
    let mut desk = desk();
    open(&mut desk, 1, "term", LEFT);
    assert_eq!(desk.suggested_slot(WindowId(1)), Some(slot(LEFT, 0)));

    desk.move_to_slot(WindowId(1), slot(LEFT, 0)).unwrap();
    // Another application: the same one would reopen in its slot.
    open(&mut desk, 2, "notes", LEFT);

    assert_eq!(desk.suggested_slot(WindowId(2)), Some(slot(LEFT, 1)));
}

#[test]
fn with_every_slot_taken_the_one_focused_last_is_suggested() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open_in(&mut desk, 2, "notes", RIGHT, 1);
    desk.focus(WindowId(1)).unwrap();
    open(&mut desk, 3, "browser", RIGHT);

    assert_eq!(desk.suggested_slot(WindowId(3)), Some(slot(RIGHT, 0)));
}

#[test]
fn nothing_is_suggested_on_a_screen_without_usable_slot() {
    let mut desk = desk();
    desk.set_layout(LEFT, LayoutKind::Free).unwrap();
    open(&mut desk, 1, "blender", LEFT);
    open(&mut desk, 2, "notes", CENTER);
    anchor_game(&mut desk, 3);

    assert_eq!(desk.suggested_slot(WindowId(1)), None);
    assert_eq!(desk.suggested_slot(WindowId(2)), None);
    assert_eq!(desk.suggested_slot(WindowId(9)), None);
}

#[test]
fn nothing_is_suggested_for_a_window_already_in_a_slot() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    assert_eq!(desk.suggested_slot(WindowId(1)), None);
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

// Moving the separation between two slots.

#[test]
fn the_separation_follows_the_pointer() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);

    let ratio = desk.ratio_at(RIGHT, Point::new(1112, 500)).unwrap();
    desk.set_ratio(RIGHT, ratio).unwrap();

    assert_eq!(geo(&desk, 1).frame, Rect::new(8, 8, 1100, 1064));
    assert_eq!(desk.separator(RIGHT), Some(Rect::new(1108, 8, 8, 1064)));
}

#[test]
fn a_screen_in_one_piece_has_no_separation() {
    let desk = desk();

    assert_eq!(desk.separator(CENTER), None);
    assert_eq!(desk.ratio_at(CENTER, Point::new(1000, 500)), None);
    assert_eq!(desk.separator(OutputId(99)), None);
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
    assert_eq!(window.frame, Rect::new(8, 8, 948, 1064));
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
fn sharing_a_slot_brings_the_shell_bar_back_for_its_tabs() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", LEFT, 0);
    desk.set_own_titlebar(WindowId(1), true).unwrap();
    open_in(&mut desk, 2, "notes", LEFT, 0);
    desk.focus(WindowId(1)).unwrap();

    let window = geo(&desk, 1);

    assert_eq!(window.titlebar, Some(Rect::new(8, 8, 948, 38)));
    assert_eq!(window.client, Rect::new(8, 46, 948, 1026));
    assert_eq!(desk.tab_strip(WindowId(1)), [WindowId(1), WindowId(2)]);
}

#[test]
fn the_tabs_layout_always_shows_the_strip() {
    let mut desk = desk();
    open_in(&mut desk, 1, "nautilus", CENTER, 0);
    desk.set_own_titlebar(WindowId(1), true).unwrap();

    desk.set_layout(CENTER, LayoutKind::Tabs).unwrap();

    assert!(geo(&desk, 1).titlebar.is_some());
    assert_eq!(desk.tab_strip(WindowId(1)), [WindowId(1)]);
}

#[test]
fn a_window_alone_in_its_slot_or_floating_has_no_tab_strip() {
    let mut desk = desk();
    open_in(&mut desk, 1, "guide", RIGHT, 0);
    open(&mut desk, 2, "gimp", RIGHT);

    assert!(desk.tab_strip(WindowId(1)).is_empty());
    assert!(desk.tab_strip(WindowId(2)).is_empty());
    assert!(desk.tab_strip(WindowId(9)).is_empty());
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
    const LAYOUTS: [LayoutKind; 6] = [
        LayoutKind::Full,
        LayoutKind::Columns,
        LayoutKind::Rows,
        LayoutKind::Tabs,
        LayoutKind::Free,
        LayoutKind::TwoThirdsOneThird,
    ];
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
            let _outcome: Result<(), ModelError> = match rng.next(18) {
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
                5 => desk.swap_with_center(window),
                6 => desk.set_layout(output, LAYOUTS[rng.next(6) as usize]),
                7 => desk.set_ratio(output, Ratio::from_permille(rng.next(1000) as u16)),
                8 => desk.focus(window),
                9 => desk.set_anchored(window, rng.next(2) == 0),
                10 => match desk.suggested_slot(window) {
                    Some(suggested) => desk.move_to_slot(window, suggested),
                    None => Ok(()),
                },
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
                _ => desk.cycle_tab(slot(output, index), rng.next(2) == 0),
            };

            if let Err(problem) = desk.check_invariants() {
                panic!("seed {seed}, step {step}: {problem}");
            }
        }
    }
}
