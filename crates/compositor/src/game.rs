//! Game mode: pure decisions about the pointer and the shell modifier, shared
//! by the nested and the hardware backends.

use coquille_model::Rect;

/// Tells a tap of the shell modifier (pressed and released with nothing in
/// between) from its use in a shortcut or a drag. A tap frees the pointer
/// from the game that holds it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ModTap {
    armed: bool,
}

impl ModTap {
    /// A key went down. `is_mod` says it is the shell modifier, and `alone`
    /// that no other modifier was held at that moment.
    pub fn key_pressed(&mut self, is_mod: bool, alone: bool) {
        self.armed = is_mod && alone;
    }

    /// A key went up. Returns whether this ends a tap of the shell modifier.
    /// Letting go of another key (one held to walk, in a game) spoils
    /// nothing.
    pub fn key_released(&mut self, is_mod: bool) -> bool {
        is_mod && std::mem::take(&mut self.armed)
    }

    /// Something else happened while the modifier was held (a mouse button):
    /// this is no longer a tap.
    pub fn disturb(&mut self) {
        self.armed = false;
    }
}

/// Where a confined pointer goes when it is at `from` and wants to be at
/// `wanted`: as far as it can on each axis while staying `inside`, so that it
/// reaches the edge it is pushed against and slides along it. Games that
/// scroll when the pointer touches the edge of their window rely on this.
pub fn confine(
    from: (f64, f64),
    wanted: (f64, f64),
    inside: impl Fn((f64, f64)) -> bool,
) -> (f64, f64) {
    if inside(wanted) {
        return wanted;
    }
    let x = furthest(from.0, wanted.0, |x| inside((x, from.1)));
    let y = furthest(from.1, wanted.1, |y| inside((x, y)));
    (x, y)
}

/// The value closest to `to` that is still `ok`, coming from `from`.
fn furthest(from: f64, to: f64, ok: impl Fn(f64) -> bool) -> f64 {
    if ok(to) {
        return to;
    }
    // Halving twelve times is finer than a pixel for any move a mouse makes.
    let (mut good, mut bad) = (from, to);
    for _ in 0..12 {
        let middle = (good + bad) / 2.0;
        if ok(middle) {
            good = middle;
        } else {
            bad = middle;
        }
    }
    good
}

/// How long a push against the edge may pause without starting over. Longer
/// than the gap between two events of the slowest mouse.
const PUSH_PAUSE_MS: u64 = 150;

/// The pointer being pushed against an edge of a game's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Push {
    /// The way out being pushed: left or right, up or down.
    side: (i8, i8),
    since_ms: u64,
    last_ms: u64,
}

/// Sticky edges: the pointer leaves the screen of a game only after it has
/// been pushed against the edge for a while, so that a quick move in the
/// game never ends up on the screen next to it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StickyEdges {
    push: Option<Push>,
}

impl StickyEdges {
    /// The pointer is being held at an edge.
    pub fn is_pushing(&self) -> bool {
        self.push.is_some()
    }

    /// Where the pointer goes when it is at `from` and the mouse takes it to
    /// `wanted`. `games` are the screens whose edges are sticky, `now_ms` the
    /// time of the move, and `delay_ms` how long the edge must be pushed.
    pub fn filter(
        &mut self,
        from: (f64, f64),
        wanted: (f64, f64),
        games: &[Rect],
        now_ms: u64,
        delay_ms: u64,
    ) -> (f64, f64) {
        let holds = |rect: &Rect, point: (f64, f64)| {
            (f64::from(rect.x)..f64::from(rect.right())).contains(&point.0)
                && (f64::from(rect.y)..f64::from(rect.bottom())).contains(&point.1)
        };
        let Some(screen) = games.iter().find(|rect| holds(rect, from)) else {
            self.push = None;
            return wanted;
        };
        if holds(screen, wanted) {
            // A push lapses by itself: a small move back does not spoil it.
            return wanted;
        }

        let side_of = |value: f64, low: i32, high: i32| {
            i8::from(value >= f64::from(high)) - i8::from(value < f64::from(low))
        };
        let side = (
            side_of(wanted.0, screen.x, screen.right()),
            side_of(wanted.1, screen.y, screen.bottom()),
        );
        let since_ms = match self.push {
            Some(push)
                if push.side == side && now_ms.saturating_sub(push.last_ms) <= PUSH_PAUSE_MS =>
            {
                push.since_ms
            }
            _ => now_ms,
        };
        if now_ms.saturating_sub(since_ms) >= delay_ms {
            self.push = None;
            return wanted;
        }
        self.push = Some(Push {
            side,
            since_ms,
            last_ms: now_ms,
        });
        // The last position of a screen is its last pixel.
        (
            wanted
                .0
                .clamp(f64::from(screen.x), f64::from(screen.right() - 1)),
            wanted
                .1
                .clamp(f64::from(screen.y), f64::from(screen.bottom() - 1)),
        )
    }
}

/// What to say in the log about how a game's frames reach its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameNote {
    /// The game's own buffer is shown as it is.
    Direct,
    /// The frame was drawn by the compositor although direct display was
    /// allowed.
    Composited,
}

/// The shortcuts the hardware may take on the screen of a game, direct
/// scanout and variable refresh rate, and what is left of them once one has
/// failed. A failure turns its shortcut off for good: the screen goes back
/// to what always works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boost {
    scanout: bool,
    vrr: bool,
    /// What the log was last told in this stay of a game on the screen.
    told: Option<FrameNote>,
}

impl Boost {
    /// `scanout` and `vrr` say what the configuration allows and the screen
    /// can do.
    pub fn new(scanout: bool, vrr: bool) -> Self {
        Self {
            scanout,
            vrr,
            told: None,
        }
    }

    /// The configuration changed what is allowed: a shortcut given up after
    /// a failure gets another chance.
    pub fn allow(&mut self, scanout: bool, vrr: bool) {
        *self = Self::new(scanout, vrr);
    }

    /// Whether the next frame may show the buffer of the game directly. Only
    /// a screen held by a fullscreen window may.
    pub fn scanout(&self, anchored: bool) -> bool {
        anchored && self.scanout
    }

    /// Whether the refresh rate should follow the game.
    pub fn vrr(&self, anchored: bool) -> bool {
        anchored && self.vrr
    }

    /// Direct scanout failed. Returns whether this is news, to be logged.
    pub fn scanout_failed(&mut self) -> bool {
        std::mem::take(&mut self.scanout)
    }

    /// Turning the variable refresh rate on failed. Returns whether this is
    /// news, to be logged.
    pub fn vrr_failed(&mut self) -> bool {
        std::mem::take(&mut self.vrr)
    }

    /// A frame went out, `direct`ly or not. Returns what is worth a line in
    /// the log: each way of showing a game, once per stay of the game.
    pub fn frame_shown(&mut self, anchored: bool, direct: bool) -> Option<FrameNote> {
        if !self.scanout(anchored) {
            self.told = None;
            return None;
        }
        let note = if direct {
            FrameNote::Direct
        } else {
            FrameNote::Composited
        };
        // Direct display is the news: once seen, a composited frame (the
        // pointer came back over the game) is nothing to report.
        let news = match self.told {
            None => true,
            Some(FrameNote::Composited) => note == FrameNote::Direct,
            Some(FrameNote::Direct) => false,
        };
        news.then(|| {
            self.told = Some(note);
            note
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_screen_of_a_game_takes_shortcuts() {
        let boost = Boost::new(true, true);

        assert!(boost.scanout(true));
        assert!(boost.vrr(true));
        assert!(!boost.scanout(false));
        assert!(!boost.vrr(false));
    }

    #[test]
    fn shortcuts_turned_off_are_never_taken() {
        let boost = Boost::new(false, false);

        assert!(!boost.scanout(true));
        assert!(!boost.vrr(true));
    }

    #[test]
    fn the_configuration_can_take_a_shortcut_back_and_give_it_again() {
        let mut boost = Boost::new(true, true);

        boost.allow(false, true);
        assert!(!boost.scanout(true));
        assert!(boost.vrr(true));

        // Given up after a failure, then allowed again by the user.
        boost.vrr_failed();
        boost.allow(true, true);
        assert!(boost.scanout(true));
        assert!(boost.vrr(true));
    }

    #[test]
    fn each_shortcut_is_switched_separately() {
        assert!(Boost::new(true, false).scanout(true));
        assert!(!Boost::new(true, false).vrr(true));
        assert!(Boost::new(false, true).vrr(true));
        assert!(!Boost::new(false, true).scanout(true));
    }

    #[test]
    fn a_failure_turns_its_shortcut_off_for_good_and_is_news_once() {
        let mut boost = Boost::new(true, true);

        assert!(boost.scanout_failed());
        assert!(!boost.scanout(true));
        assert!(boost.vrr(true));
        assert!(!boost.scanout_failed());

        assert!(boost.vrr_failed());
        assert!(!boost.vrr(true));
        assert!(!boost.vrr_failed());
    }

    #[test]
    fn the_first_frame_of_a_game_says_how_it_was_shown() {
        let mut direct = Boost::new(true, false);
        let mut composited = Boost::new(true, false);

        assert_eq!(direct.frame_shown(true, true), Some(FrameNote::Direct));
        assert_eq!(
            composited.frame_shown(true, false),
            Some(FrameNote::Composited)
        );
    }

    #[test]
    fn the_same_way_of_showing_frames_is_said_once() {
        let mut boost = Boost::new(true, false);

        boost.frame_shown(true, false);
        assert_eq!(boost.frame_shown(true, false), None);
        assert_eq!(boost.frame_shown(true, true), Some(FrameNote::Direct));
        assert_eq!(boost.frame_shown(true, true), None);
        // The pointer passes over the game: composited again, not news.
        assert_eq!(boost.frame_shown(true, false), None);
        assert_eq!(boost.frame_shown(true, true), None);
    }

    #[test]
    fn the_next_game_is_reported_anew() {
        let mut boost = Boost::new(true, false);

        boost.frame_shown(true, true);
        assert_eq!(boost.frame_shown(false, false), None);

        assert_eq!(boost.frame_shown(true, true), Some(FrameNote::Direct));
    }

    #[test]
    fn nothing_is_reported_when_direct_display_is_off() {
        let mut boost = Boost::new(false, true);

        assert_eq!(boost.frame_shown(true, false), None);
    }

    #[test]
    fn pressing_and_releasing_the_modifier_alone_is_a_tap() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, true);

        assert!(tap.key_released(true));
    }

    #[test]
    fn a_shortcut_is_not_a_tap() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, true);
        tap.key_pressed(false, false);

        assert!(!tap.key_released(false));
        assert!(!tap.key_released(true));
    }

    #[test]
    fn the_modifier_pressed_with_another_modifier_held_is_not_a_tap() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, false);

        assert!(!tap.key_released(true));
    }

    #[test]
    fn a_drag_with_the_modifier_is_not_a_tap() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, true);
        tap.disturb();

        assert!(!tap.key_released(true));
    }

    #[test]
    fn letting_go_of_another_key_during_the_tap_spoils_nothing() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, true);

        assert!(!tap.key_released(false));
        assert!(tap.key_released(true));
    }

    #[test]
    fn a_tap_counts_once() {
        let mut tap = ModTap::default();

        tap.key_pressed(true, true);
        tap.key_released(true);

        assert!(!tap.key_released(true));
    }

    const GAME: Rect = Rect::new(1920, 0, 3440, 1440);
    const DELAY: u64 = 300;

    /// Pushes right against the right edge of the game screen at `now`.
    fn push_right(edges: &mut StickyEdges, now: u64) -> (f64, f64) {
        edges.filter((5359.0, 700.0), (5370.0, 702.0), &[GAME], now, DELAY)
    }

    #[test]
    fn the_pointer_moves_freely_inside_the_screen_of_a_game() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((2000.0, 700.0), (2500.0, 650.0), &[GAME], 0, DELAY);

        assert_eq!(moved, (2500.0, 650.0));
    }

    #[test]
    fn the_pointer_stops_at_the_edge_of_the_screen_of_a_game() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((5350.0, 700.0), (5370.0, 702.0), &[GAME], 0, DELAY);

        assert_eq!(moved, (5359.0, 702.0));
    }

    #[test]
    fn pushing_against_the_edge_long_enough_lets_the_pointer_out() {
        let mut edges = StickyEdges::default();

        assert_eq!(push_right(&mut edges, 1000), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, 1100), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, 1200), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, 1299), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, 1300), (5370.0, 702.0));
    }

    #[test]
    fn a_pause_in_the_push_starts_the_delay_again() {
        let mut edges = StickyEdges::default();

        push_right(&mut edges, 1000);
        push_right(&mut edges, 1100);
        // Nothing for longer than a push may pause: the count starts over.
        let again = 1100 + PUSH_PAUSE_MS + 1;
        assert_eq!(push_right(&mut edges, again), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, again + 150), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, again + 299), (5359.0, 702.0));
        assert_eq!(push_right(&mut edges, again + 300), (5370.0, 702.0));
    }

    #[test]
    fn a_small_move_back_does_not_spoil_the_push() {
        let mut edges = StickyEdges::default();

        push_right(&mut edges, 1000);
        push_right(&mut edges, 1100);
        let back = edges.filter((5359.0, 700.0), (5357.0, 700.0), &[GAME], 1110, DELAY);
        assert_eq!(back, (5357.0, 700.0));
        push_right(&mut edges, 1200);

        assert_eq!(push_right(&mut edges, 1300), (5370.0, 702.0));
    }

    #[test]
    fn pushing_against_another_edge_starts_the_delay_again() {
        let mut edges = StickyEdges::default();

        push_right(&mut edges, 1000);
        push_right(&mut edges, 1100);
        let left = edges.filter((1920.0, 700.0), (1900.0, 700.0), &[GAME], 1200, DELAY);
        assert_eq!(left, (1920.0, 700.0));

        let left = edges.filter((1920.0, 700.0), (1900.0, 700.0), &[GAME], 1350, DELAY);
        assert_eq!(left, (1920.0, 700.0));
        let left = edges.filter((1920.0, 700.0), (1900.0, 700.0), &[GAME], 1500, DELAY);
        assert_eq!(left, (1900.0, 700.0));
    }

    #[test]
    fn once_out_the_pointer_must_push_again_to_leave_the_next_time() {
        let mut edges = StickyEdges::default();

        push_right(&mut edges, 1000);
        push_right(&mut edges, 1150);
        push_right(&mut edges, 1300);

        assert_eq!(push_right(&mut edges, 1310), (5359.0, 702.0));
    }

    #[test]
    fn entering_the_screen_of_a_game_is_never_held_back() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((1900.0, 500.0), (1950.0, 500.0), &[GAME], 0, DELAY);

        assert_eq!(moved, (1950.0, 500.0));
    }

    #[test]
    fn screens_without_a_game_have_no_sticky_edge() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((1900.0, 500.0), (1950.0, 500.0), &[], 0, DELAY);
        assert_eq!(moved, (1950.0, 500.0));
        let moved = edges.filter((100.0, 500.0), (-50.0, 500.0), &[GAME], 0, DELAY);
        assert_eq!(moved, (-50.0, 500.0));
    }

    #[test]
    fn a_delay_of_zero_turns_sticky_edges_off() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((5350.0, 700.0), (5370.0, 702.0), &[GAME], 0, 0);

        assert_eq!(moved, (5370.0, 702.0));
    }

    #[test]
    fn a_corner_holds_the_pointer_on_both_axes() {
        let mut edges = StickyEdges::default();

        let moved = edges.filter((1925.0, 5.0), (1900.0, -20.0), &[GAME], 0, DELAY);

        assert_eq!(moved, (1920.0, 0.0));
    }

    /// A region 100 wide and 50 high.
    fn in_box(point: (f64, f64)) -> bool {
        (0.0..100.0).contains(&point.0) && (0.0..50.0).contains(&point.1)
    }

    #[test]
    fn a_confined_pointer_moves_freely_inside_its_region() {
        assert_eq!(confine((10.0, 10.0), (60.0, 40.0), in_box), (60.0, 40.0));
    }

    #[test]
    fn a_confined_pointer_reaches_the_edge_and_slides_along_it() {
        let (x, y) = confine((90.0, 10.0), (120.0, 30.0), in_box);
        assert!((99.0..100.0).contains(&x), "{x}");
        assert_eq!(y, 30.0);

        let (x, y) = confine((50.0, 45.0), (70.0, 80.0), in_box);
        assert_eq!(x, 70.0);
        assert!((49.0..50.0).contains(&y), "{y}");
    }

    #[test]
    fn a_confined_pointer_pushed_into_a_corner_stops_in_the_corner() {
        let (x, y) = confine((95.0, 45.0), (130.0, 70.0), in_box);

        assert!((99.0..100.0).contains(&x), "{x}");
        assert!((49.0..50.0).contains(&y), "{y}");
    }

    #[test]
    fn a_confined_pointer_does_not_cut_through_a_notch() {
        // An L-shaped region: the box without its top-right quarter.
        let in_l = |point: (f64, f64)| in_box(point) && !(point.0 >= 50.0 && point.1 < 25.0);

        let (x, y) = confine((40.0, 30.0), (60.0, 10.0), in_l);

        assert_eq!(x, 60.0);
        assert!((25.0..26.0).contains(&y), "{y}");
    }

    #[test]
    fn a_pointer_already_outside_its_region_does_not_move_further_out() {
        assert_eq!(confine((150.0, 10.0), (160.0, 10.0), in_box), (150.0, 10.0));
    }
}
