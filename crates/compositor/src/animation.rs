//! Animations: the shell eases from one state to the next instead of jumping.
//!
//! Pure and tested: curves, tweens over instants, and how an animated thing
//! looks at the instant a frame is drawn. The model decides where things
//! go; this only says where they show on the way. Nothing here knows
//! Wayland: the scene asks each window, panel or hint for its [`Look`].

use std::time::{Duration, Instant};

use coquille_model::{Point, Rect};

/// How long each animation lasts, at normal speed.
pub mod durations {
    use std::time::Duration;

    /// A window appears.
    pub const OPEN: Duration = Duration::from_millis(260);
    /// A window closes.
    pub const CLOSE: Duration = Duration::from_millis(200);
    /// A window goes from one place to another: a slot, maximized, back.
    pub const MOVE: Duration = Duration::from_millis(320);
    /// A window goes into the dock.
    pub const MINIMIZE: Duration = Duration::from_millis(300);
    /// A window comes back from the dock.
    pub const RESTORE: Duration = Duration::from_millis(320);
    /// The accent ring of the focused window lights up or goes out.
    pub const RING: Duration = Duration::from_millis(160);
    /// The window switcher or the drop zones show.
    pub const FADE: Duration = Duration::from_millis(160);
    /// A panel of the shell (drawer, notifications) comes in.
    pub const PANEL: Duration = Duration::from_millis(260);
    /// A panel of the shell goes.
    pub const PANEL_CLOSE: Duration = Duration::from_millis(180);
}

/// How a tween goes from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    /// Fast, then slowing down: what comes to rest.
    EaseOut,
    /// Slow, then speeding up: what leaves.
    EaseIn,
    /// A critically damped spring: quick to start, long to settle, never
    /// past the end. What moves from one place to another.
    Settle,
}

/// The stiffness of [`Curve::Settle`], for the spring to be still at the end.
const SETTLE: f64 = 9.23;

impl Curve {
    /// Where the curve is at `t`, from 0 to 1.
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Linear => t,
            Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
            Curve::EaseIn => t.powi(3),
            Curve::Settle => {
                let spring = |t: f64| 1.0 - (1.0 + SETTLE * t) * (-SETTLE * t).exp();
                spring(t) / spring(1.0)
            }
        }
    }
}

/// How fast animations run, and whether they run at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    enabled: bool,
    /// How many times longer every animation lasts: to look at one closely.
    slowdown: f64,
}

impl Default for Pace {
    fn default() -> Self {
        Self::new(true, 1.0)
    }
}

impl Pace {
    pub fn new(enabled: bool, slowdown: f64) -> Self {
        Self {
            enabled,
            slowdown: if slowdown.is_finite() {
                slowdown.clamp(0.1, 50.0)
            } else {
                1.0
            },
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// A tween that starts at `now`, or none when animations are off.
    pub fn tween(&self, now: Instant, duration: Duration, curve: Curve) -> Option<Tween> {
        self.enabled.then(|| Tween {
            start: now,
            duration: duration.mul_f64(self.slowdown),
            curve,
        })
    }
}

/// A run from 0 to 1 over some time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    start: Instant,
    duration: Duration,
    curve: Curve,
}

impl Tween {
    /// How much of its time has passed, from 0 to 1.
    pub fn linear(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start);
        (elapsed.as_secs_f64() / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// Where it is along its curve.
    pub fn at(&self, now: Instant) -> f64 {
        self.curve.at(self.linear(now))
    }

    pub fn done(&self, now: Instant) -> bool {
        self.linear(now) >= 1.0
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// A rectangle that may sit between pixels, on its way somewhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<Rect> for Area {
    fn from(rect: Rect) -> Self {
        Self {
            x: f64::from(rect.x),
            y: f64::from(rect.y),
            w: f64::from(rect.w),
            h: f64::from(rect.h),
        }
    }
}

impl Area {
    pub fn lerp(self, to: Self, t: f64) -> Self {
        Self {
            x: lerp(self.x, to.x, t),
            y: lerp(self.y, to.y, t),
            w: lerp(self.w, to.w, t),
            h: lerp(self.h, to.h, t),
        }
    }

    pub fn center(self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// `factor` times as large, around the same center.
    pub fn scaled(self, factor: f64) -> Self {
        let (cx, cy) = self.center();
        let (w, h) = (self.w * factor, self.h * factor);
        Self {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            w,
            h,
        }
    }

    pub fn moved(self, dx: f64, dy: f64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    /// A small area of the proportions of this one, centered on `(x, y)`.
    pub fn shrunk_to(self, x: f64, y: f64, factor: f64) -> Self {
        let (w, h) = (self.w * factor, self.h * factor);
        Self {
            x: x - w / 2.0,
            y: y - h / 2.0,
            w,
            h,
        }
    }
}

/// Where something drawn for its place `target` shows, and how opaque.
/// Everything inside the target follows: its parts are moved and stretched
/// with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub target: Rect,
    pub shown: Area,
    pub alpha: f32,
}

impl Look {
    /// Exactly at its place.
    pub fn still(target: Rect) -> Self {
        Self {
            target,
            shown: target.into(),
            alpha: 1.0,
        }
    }

    pub fn is_still(&self) -> bool {
        self.alpha >= 1.0 && self.shown == Area::from(self.target)
    }

    /// How much wider and taller it shows than its place.
    pub fn scale(&self) -> (f64, f64) {
        let ratio = |shown: f64, target: i32| {
            if target > 0 {
                shown / f64::from(target)
            } else {
                1.0
            }
        };
        (
            ratio(self.shown.w, self.target.w),
            ratio(self.shown.h, self.target.h),
        )
    }

    /// Where a point of the place shows.
    pub fn point(&self, x: f64, y: f64) -> (f64, f64) {
        let (sx, sy) = self.scale();
        (
            self.shown.x + (x - f64::from(self.target.x)) * sx,
            self.shown.y + (y - f64::from(self.target.y)) * sy,
        )
    }

    /// Where a rectangle of the place shows, to the pixel. Its edges are
    /// rounded each on their own, so that two rectangles side by side stay
    /// side by side.
    pub fn rect(&self, rect: Rect) -> Rect {
        if self.shown == Area::from(self.target) {
            return rect;
        }
        let (x0, y0) = self.point(f64::from(rect.x), f64::from(rect.y));
        let (x1, y1) = self.point(f64::from(rect.right()), f64::from(rect.bottom()));
        let (x0, y0, x1, y1) = (x0.round(), y0.round(), x1.round(), y1.round());
        Rect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
    }

    /// The same look, moved by `offset`.
    pub fn translated(&self, offset: Point) -> Look {
        Look {
            target: self.target.translated(offset),
            shown: self.shown.moved(f64::from(offset.x), f64::from(offset.y)),
            alpha: self.alpha,
        }
    }
}

/// How a window comes to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Entrance {
    /// It opens: it grows a little, from transparent.
    Open,
    /// It comes back from the dock, at `from`.
    Restore { from: Area },
}

/// What a window does besides standing at its place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Motion {
    /// From where it showed, to its place.
    moving: Option<(Area, Tween)>,
    entering: Option<(Entrance, Tween)>,
    /// It goes into the dock, at this area: drawn although hidden.
    minimizing: Option<(Area, Tween)>,
    /// Hidden, it is still drawn as it was until the tween ends: under a
    /// window that grows over it.
    lingering: Option<Tween>,
    /// The ring lights up (`true`) or goes out.
    ring: Option<(bool, Tween)>,
}

/// How small a window opening starts.
const OPEN_SCALE: f64 = 0.9;
/// How small a closing window ends.
pub const CLOSE_SCALE: f64 = 0.92;
/// How small a window going into the dock ends.
pub const DOCK_SCALE: f64 = 0.08;

impl Motion {
    /// Its place changed from `before`: it goes there from where it shows
    /// now, even halfway through another move.
    pub fn move_from(&mut self, before: Rect, now: Instant, tween: Option<Tween>) {
        let Some(tween) = tween else {
            self.moving = None;
            return;
        };
        let from = self.placed(before, now);
        self.moving = Some((from, tween));
    }

    pub fn enter(&mut self, entrance: Entrance, tween: Option<Tween>) {
        self.minimizing = None;
        self.entering = tween.map(|tween| (entrance, tween));
    }

    pub fn minimize(&mut self, to: Area, tween: Option<Tween>) {
        self.entering = None;
        self.minimizing = tween.map(|tween| (to, tween));
    }

    pub fn linger(&mut self, tween: Option<Tween>) {
        self.lingering = tween;
    }

    pub fn ring(&mut self, lit: bool, tween: Option<Tween>) {
        self.ring = tween.map(|tween| (lit, tween));
    }

    /// Where its frame shows, moving aside, for its place `target`.
    fn placed(&self, target: Rect, now: Instant) -> Area {
        match &self.moving {
            Some((from, tween)) => from.lerp(target.into(), tween.at(now)),
            None => target.into(),
        }
    }

    /// How its frame shows, for its place `target`.
    pub fn look(&self, target: Rect, now: Instant) -> Look {
        let mut shown = self.placed(target, now);
        let mut alpha = 1.0;
        match &self.entering {
            Some((Entrance::Open, tween)) => {
                shown = shown.scaled(lerp(OPEN_SCALE, 1.0, tween.at(now)));
                alpha = Curve::EaseOut.at(tween.linear(now) * 1.6);
            }
            Some((Entrance::Restore { from }, tween)) => {
                shown = from.lerp(shown, tween.at(now));
                alpha = Curve::EaseOut.at(tween.linear(now) * 2.5);
            }
            None => {}
        }
        if let Some((to, tween)) = &self.minimizing {
            shown = shown.lerp(*to, tween.at(now));
            alpha = 1.0 - Curve::EaseIn.at(tween.linear(now));
        }
        Look {
            target,
            shown,
            alpha: alpha as f32,
        }
    }

    /// Drawn although the model hides it: on its way to the dock, or still
    /// covered by what grows over it.
    pub fn drawn_hidden(&self, now: Instant) -> bool {
        let busy = |tween: &Tween| !tween.done(now);
        self.minimizing
            .as_ref()
            .is_some_and(|(_, tween)| busy(tween))
            || self.lingering.as_ref().is_some_and(busy)
    }

    /// How opaque its ring is, being focused or not.
    pub fn ring_alpha(&self, focused: bool, now: Instant) -> f32 {
        let lit = match &self.ring {
            Some((true, tween)) => tween.at(now),
            Some((false, tween)) => 1.0 - tween.at(now),
            None => f64::from(u8::from(focused)),
        };
        lit as f32
    }

    /// Forgets what is over, and says whether anything is still going on.
    pub fn settle(&mut self, now: Instant) -> bool {
        let over = |tween: &Tween| tween.done(now);
        if self.moving.as_ref().is_some_and(|(_, tween)| over(tween)) {
            self.moving = None;
        }
        if self.entering.as_ref().is_some_and(|(_, tween)| over(tween)) {
            self.entering = None;
        }
        if self
            .minimizing
            .as_ref()
            .is_some_and(|(_, tween)| over(tween))
        {
            self.minimizing = None;
        }
        if self.lingering.as_ref().is_some_and(over) {
            self.lingering = None;
        }
        if self.ring.as_ref().is_some_and(|(_, tween)| over(tween)) {
            self.ring = None;
        }
        self.moving.is_some()
            || self.entering.is_some()
            || self.minimizing.is_some()
            || self.lingering.is_some()
            || self.ring.is_some()
    }
}

/// Something that leaves: a window closed, a panel gone. It shrinks a
/// little, or slides away, and fades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exit {
    pub tween: Tween,
    /// Where it slides to, from its place; none to shrink instead.
    pub slide: Option<(f64, f64)>,
}

impl Exit {
    pub fn look(&self, target: Rect, now: Instant) -> Look {
        let t = self.tween.at(now);
        let shown = match self.slide {
            Some((dx, dy)) => Area::from(target).moved(dx * t, dy * t),
            None => Area::from(target).scaled(lerp(1.0, CLOSE_SCALE, t)),
        };
        Look {
            target,
            shown,
            alpha: (1.0 - t) as f32,
        }
    }

    pub fn done(&self, now: Instant) -> bool {
        self.tween.done(now)
    }
}

/// A panel coming in: it slides from where it comes from, or grows a
/// little, and gets opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub tween: Tween,
    /// Where it starts, from its place; none to grow instead.
    pub slide: Option<(f64, f64)>,
}

impl Entry {
    pub fn look(&self, target: Rect, now: Instant) -> Look {
        let t = self.tween.at(now);
        let shown = match self.slide {
            Some((dx, dy)) => Area::from(target).moved(dx * (1.0 - t), dy * (1.0 - t)),
            None => Area::from(target).scaled(lerp(0.94, 1.0, t)),
        };
        Look {
            target,
            shown,
            alpha: Curve::EaseOut.at(self.tween.linear(now) * 1.5) as f32,
        }
    }

    pub fn done(&self, now: Instant) -> bool {
        self.tween.done(now)
    }
}

/// The side of a screen a panel comes from, by the edges it holds on to
/// (left, top, right, bottom): one edge, or one edge and both of its
/// neighbours. `None` when it holds two opposite edges or none.
pub fn edge_of(left: bool, top: bool, right: bool, bottom: bool) -> Option<Side> {
    match (left, top, right, bottom) {
        (true, _, false, _) if top == bottom => Some(Side::Left),
        (false, _, true, _) if top == bottom => Some(Side::Right),
        (_, true, _, false) if left == right => Some(Side::Top),
        (_, false, _, true) if left == right => Some(Side::Bottom),
        _ => None,
    }
}

/// A side of a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    /// The side of `screen` that `bar` stands against.
    pub fn of_bar(bar: Rect, screen: Rect) -> Self {
        if bar.h >= bar.w {
            if bar.x + bar.w / 2 < screen.x + screen.w / 2 {
                Side::Left
            } else {
                Side::Right
            }
        } else if bar.y + bar.h / 2 < screen.y + screen.h / 2 {
            Side::Top
        } else {
            Side::Bottom
        }
    }

    /// Moving `distance` toward this side.
    pub fn toward(self, distance: f64) -> (f64, f64) {
        match self {
            Side::Left => (-distance, 0.0),
            Side::Top => (0.0, -distance),
            Side::Right => (distance, 0.0),
            Side::Bottom => (0.0, distance),
        }
    }

    /// Where a panel at `place` starts from, coming in from this side: a
    /// quarter of its depth away, no more than [`PANEL_SLIDE`].
    pub fn slide(self, place: Rect) -> (f64, f64) {
        let depth = match self {
            Side::Left | Side::Right => place.w,
            Side::Top | Side::Bottom => place.h,
        };
        self.toward((f64::from(depth) / 4.0).min(PANEL_SLIDE))
    }

    /// Where something at `point` goes into a bar against this side: the
    /// bar's line, level with the point.
    pub fn into_bar(self, bar: Rect, point: Point) -> (f64, f64) {
        let clamp = |value: i32, low: i32, high: i32| f64::from(value.clamp(low, high.max(low)));
        let middle_x = f64::from(bar.x) + f64::from(bar.w) / 2.0;
        let middle_y = f64::from(bar.y) + f64::from(bar.h) / 2.0;
        match self {
            Side::Left | Side::Right => (middle_x, clamp(point.y, bar.y, bar.bottom())),
            Side::Top | Side::Bottom => (clamp(point.x, bar.x, bar.right()), middle_y),
        }
    }
}

/// How a panel comes in, and goes the way it came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// From a side of its screen.
    Slide(Side),
    /// Growing a little, in place.
    Grow,
    /// Only getting opaque, in place: what covers the whole screen, and
    /// draws a copy of the bar of the dock where the bar is.
    Fade,
    /// As it is: the panel moves by itself (the launcher of the dock).
    Own,
}

impl Arrival {
    /// Where a panel at `place` starts from; none to grow instead (or, for
    /// one that moves by itself, to stay as it is).
    pub fn offset(self, place: Rect) -> Option<(f64, f64)> {
        match self {
            Arrival::Slide(side) => Some(side.slide(place)),
            Arrival::Grow => None,
            Arrival::Fade => Some((0.0, 0.0)),
            Arrival::Own => None,
        }
    }
}

/// How far a panel slides in: a quarter of its depth, no more than this.
pub const PANEL_SLIDE: f64 = 96.0;

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn tween(start: Instant, duration: Duration, curve: Curve) -> Option<Tween> {
        Pace::default().tween(start, duration, curve)
    }

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for curve in [Curve::Linear, Curve::EaseOut, Curve::EaseIn, Curve::Settle] {
            assert!(curve.at(0.0).abs() < 1e-9, "{curve:?}");
            assert!((curve.at(1.0) - 1.0).abs() < 1e-9, "{curve:?}");
            assert!(
                (curve.at(2.0) - 1.0).abs() < 1e-9,
                "{curve:?} stops at the end"
            );
        }
    }

    #[test]
    fn a_settling_move_never_goes_past_its_end() {
        let mut last = 0.0;
        for step in 0..=100 {
            let at = Curve::Settle.at(f64::from(step) / 100.0);
            assert!(at >= last && at <= 1.0 + 1e-9);
            last = at;
        }
        assert!(Curve::Settle.at(0.3) > 0.75, "most of the way early on");
    }

    #[test]
    fn what_comes_to_rest_is_fast_first_and_what_leaves_slow_first() {
        assert!(Curve::EaseOut.at(0.25) > 0.25);
        assert!(Curve::EaseIn.at(0.25) < 0.25);
    }

    #[test]
    fn animations_turned_off_give_no_tween() {
        let pace = Pace::new(false, 1.0);
        assert_eq!(pace.tween(Instant::now(), 100 * MS, Curve::Linear), None);
    }

    #[test]
    fn a_slowdown_stretches_every_animation() {
        let start = Instant::now();
        let slow = Pace::new(true, 10.0)
            .tween(start, 100 * MS, Curve::Linear)
            .unwrap();
        assert!((slow.linear(start + 500 * MS) - 0.5).abs() < 1e-9);
        assert!(!slow.done(start + 999 * MS));
        assert!(slow.done(start + 1000 * MS));
    }

    #[test]
    fn a_window_at_rest_shows_at_its_place() {
        let place = Rect::new(10, 20, 300, 200);
        let look = Motion::default().look(place, Instant::now());

        assert!(look.is_still());
        assert_eq!(
            look.rect(Rect::new(10, 20, 300, 38)),
            Rect::new(10, 20, 300, 38)
        );
    }

    #[test]
    fn a_moved_window_goes_from_where_it_was_to_its_place() {
        let start = Instant::now();
        let (before, after) = (Rect::new(0, 0, 100, 100), Rect::new(200, 0, 300, 100));
        let mut motion = Motion::default();
        motion.move_from(before, start, tween(start, 100 * MS, Curve::Linear));

        assert_eq!(motion.look(after, start).shown, Area::from(before));
        let halfway = motion.look(after, start + 50 * MS);
        assert_eq!(halfway.shown, Area::from(Rect::new(100, 0, 200, 100)));
        assert!(motion.look(after, start + 100 * MS).is_still());
    }

    #[test]
    fn a_move_halfway_through_another_starts_from_where_the_window_shows() {
        let start = Instant::now();
        let mut motion = Motion::default();
        let (a, b, c) = (
            Rect::new(0, 0, 100, 100),
            Rect::new(200, 0, 100, 100),
            Rect::new(200, 200, 100, 100),
        );
        motion.move_from(a, start, tween(start, 100 * MS, Curve::Linear));
        let now = start + 50 * MS;
        motion.move_from(b, now, tween(now, 100 * MS, Curve::Linear));

        let shown = motion.look(c, now).shown;
        assert_eq!(shown, Area::from(Rect::new(100, 0, 100, 100)));
    }

    #[test]
    fn the_parts_of_a_window_stretch_with_it() {
        let place = Rect::new(100, 100, 200, 100);
        let look = Look {
            target: place,
            shown: Area::from(Rect::new(0, 0, 400, 50)),
            alpha: 1.0,
        };

        assert_eq!(look.scale(), (2.0, 0.5));
        assert_eq!(
            look.rect(Rect::new(100, 100, 200, 20)),
            Rect::new(0, 0, 400, 10)
        );
        assert_eq!(
            look.rect(Rect::new(200, 120, 100, 80)),
            Rect::new(200, 10, 200, 40)
        );
    }

    #[test]
    fn an_opening_window_grows_from_its_middle_and_fades_in() {
        let start = Instant::now();
        let place = Rect::new(0, 0, 1000, 500);
        let mut motion = Motion::default();
        motion.enter(Entrance::Open, tween(start, 100 * MS, Curve::Linear));

        let first = motion.look(place, start);
        assert_eq!(first.alpha, 0.0);
        assert_eq!(first.shown.center(), (500.0, 250.0));
        assert!((first.shown.w - 900.0).abs() < 1e-9);
        assert!(motion.look(place, start + 100 * MS).is_still());
    }

    #[test]
    fn a_minimized_window_is_drawn_until_it_reaches_the_dock() {
        let start = Instant::now();
        let place = Rect::new(500, 100, 800, 600);
        let dock = Area::from(place).shrunk_to(20.0, 400.0, DOCK_SCALE);
        let mut motion = Motion::default();
        motion.minimize(dock, tween(start, 100 * MS, Curve::Linear));

        assert!(motion.drawn_hidden(start + 50 * MS));
        let end = motion.look(place, start + 100 * MS);
        assert_eq!(end.shown.center(), (20.0, 400.0));
        assert_eq!(end.alpha, 0.0);
        assert!(!motion.drawn_hidden(start + 100 * MS));
        assert!(!motion.settle(start + 100 * MS), "nothing left to do");
    }

    #[test]
    fn a_restored_window_leaves_the_dock_for_its_place() {
        let start = Instant::now();
        let place = Rect::new(500, 100, 800, 600);
        let from = Area::from(place).shrunk_to(20.0, 400.0, DOCK_SCALE);
        let mut motion = Motion::default();
        motion.enter(
            Entrance::Restore { from },
            tween(start, 100 * MS, Curve::Linear),
        );

        assert_eq!(motion.look(place, start).shown, from);
        assert!(motion.look(place, start + 100 * MS).is_still());
    }

    #[test]
    fn the_ring_fades_in_and_out() {
        let start = Instant::now();
        let mut motion = Motion::default();
        assert_eq!(motion.ring_alpha(true, start), 1.0);
        assert_eq!(motion.ring_alpha(false, start), 0.0);

        motion.ring(false, tween(start, 100 * MS, Curve::Linear));
        assert_eq!(motion.ring_alpha(false, start + 25 * MS), 0.75);
    }

    #[test]
    fn a_closing_window_shrinks_and_fades_out() {
        let start = Instant::now();
        let exit = Exit {
            tween: tween(start, 100 * MS, Curve::Linear).unwrap(),
            slide: None,
        };
        let place = Rect::new(0, 0, 100, 100);

        let end = exit.look(place, start + 100 * MS);
        assert_eq!(end.alpha, 0.0);
        assert!((end.shown.w - 92.0).abs() < 1e-9);
        assert!(exit.done(start + 100 * MS));
    }

    #[test]
    fn a_panel_slides_in_from_its_side() {
        let start = Instant::now();
        let entry = Entry {
            tween: tween(start, 100 * MS, Curve::Linear).unwrap(),
            slide: Some(Side::Left.toward(40.0)),
        };
        let place = Rect::new(60, 0, 400, 1000);

        assert_eq!(entry.look(place, start).shown.x, 20.0);
        assert!(entry.look(place, start + 100 * MS).is_still());
    }

    #[test]
    fn a_panel_comes_from_the_one_edge_it_holds() {
        assert_eq!(edge_of(true, true, false, true), Some(Side::Left));
        assert_eq!(edge_of(true, false, false, false), Some(Side::Left));
        assert_eq!(edge_of(false, true, true, true), Some(Side::Right));
        assert_eq!(edge_of(true, false, true, true), Some(Side::Bottom));
        assert_eq!(edge_of(true, true, true, true), None, "the whole screen");
        assert_eq!(edge_of(false, false, false, false), None, "centered");
        assert_eq!(edge_of(true, false, false, true), None, "a corner");
    }

    #[test]
    fn a_bar_stands_against_the_side_it_is_nearest() {
        let screen = Rect::new(0, 0, 1920, 1080);
        assert_eq!(Side::of_bar(Rect::new(8, 8, 44, 1064), screen), Side::Left);
        assert_eq!(
            Side::of_bar(Rect::new(1868, 8, 44, 1064), screen),
            Side::Right
        );
        assert_eq!(
            Side::of_bar(Rect::new(600, 1036, 720, 36), screen),
            Side::Bottom
        );
    }

    #[test]
    fn a_window_goes_into_the_bar_level_with_itself() {
        let bar = Rect::new(8, 100, 44, 800);
        assert_eq!(
            Side::Left.into_bar(bar, Point::new(900, 500)),
            (30.0, 500.0)
        );
        assert_eq!(
            Side::Left.into_bar(bar, Point::new(900, 2000)),
            (30.0, 900.0)
        );
    }
}
