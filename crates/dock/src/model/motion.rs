//! How the dock moves: what goes from hidden to shown over a while, the
//! icon that hops while its application starts, the tile that grows in.
//! Pure: the time is always given.

use std::time::{Duration, Instant};

/// How long each motion takes, before any slowdown.
pub mod durations {
    use std::time::Duration;

    /// The launcher coming out from behind its bar.
    pub const DRAWER: Duration = Duration::from_millis(280);
    /// The launcher going back behind it.
    pub const DRAWER_CLOSE: Duration = Duration::from_millis(200);
    /// The mixer opening in the drawer.
    pub const MIXER: Duration = Duration::from_millis(180);
    /// A new tile of the bar growing in.
    pub const ARRIVAL: Duration = Duration::from_millis(320);
    /// One hop of an icon whose application starts.
    pub const HOP: Duration = Duration::from_millis(640);
    /// How long an icon hops at most, when no window comes.
    pub const LAUNCH: Duration = Duration::from_secs(5);
}

/// Whether the dock moves, and how much slower than it should (to watch
/// a motion, or capture it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    pub enabled: bool,
    slowdown: f32,
}

impl Pace {
    pub fn new(enabled: bool, slowdown: f32) -> Self {
        let slowdown = if slowdown.is_finite() {
            slowdown.clamp(0.1, 50.0)
        } else {
            1.0
        };
        Self { enabled, slowdown }
    }

    /// How long `duration` lasts at this pace; nothing when nothing moves.
    pub fn of(self, duration: Duration) -> Duration {
        if self.enabled {
            duration.mul_f32(self.slowdown)
        } else {
            Duration::ZERO
        }
    }
}

impl Default for Pace {
    fn default() -> Self {
        Self::new(true, 1.0)
    }
}

/// Something that shows and hides: how far it is shown, from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reveal {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Reveal {
    /// At rest at `value`.
    pub fn at_rest(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            start: now,
            duration: Duration::ZERO,
        }
    }

    /// Hidden, and coming in at once.
    pub fn coming(now: Instant, duration: Duration) -> Self {
        let mut reveal = Self::at_rest(0.0, now);
        reveal.go(1.0, now, duration);
        reveal
    }

    /// Heads for `to` from where it is now: turned back halfway, it does
    /// not jump.
    pub fn go(&mut self, to: f32, now: Instant, duration: Duration) {
        let from = self.at(now);
        // What is left of the way takes its share of the time.
        let share = (to - from).abs().min(1.0);
        *self = Self {
            from,
            to,
            start: now,
            duration: duration.mul_f32(share),
        };
    }

    /// How far it is shown at `now`: quick to come, quick to go at the end.
    pub fn at(&self, now: Instant) -> f32 {
        let progress = self.progress(now);
        let eased = if self.to >= self.from {
            ease_out(progress)
        } else {
            ease_in(progress)
        };
        self.from + (self.to - self.from) * eased
    }

    pub fn moving(&self, now: Instant) -> bool {
        self.progress(now) < 1.0
    }

    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0)
    }
}

/// An application started from the dock, until its first window comes.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    /// The key of its entry in the bar.
    pub key: String,
    since: Instant,
    /// How many windows it had when it was started.
    windows: usize,
    /// One hop, at this pace.
    hop: Duration,
    /// When it lands for good: at the end of the hop it is in.
    lands: Option<Instant>,
}

impl Launch {
    pub fn new(key: String, windows: usize, now: Instant, pace: Pace) -> Self {
        Self {
            key,
            since: now,
            windows,
            hop: pace.of(durations::HOP),
            lands: None,
        }
    }

    /// It now has `windows`: one more than at the start ends it, as does
    /// waiting too long. It finishes the hop it is in.
    pub fn watch(&mut self, windows: usize, now: Instant) {
        if self.lands.is_some() || self.hop.is_zero() {
            return;
        }
        let elapsed = now.saturating_duration_since(self.since);
        let limit =
            durations::LAUNCH.mul_f32(self.hop.as_secs_f32() / durations::HOP.as_secs_f32());
        if windows > self.windows || elapsed >= limit {
            let hops = (elapsed.as_secs_f32() / self.hop.as_secs_f32())
                .ceil()
                .max(1.0);
            self.lands = Some(self.since + self.hop.mul_f32(hops));
        }
    }

    /// It has landed: it can go.
    pub fn over(&self, now: Instant) -> bool {
        self.hop.is_zero() || self.lands.is_some_and(|lands| now >= lands)
    }

    /// How high it is at `now`, from 0 (on the bar) to 1 (at the top of a
    /// hop): it falls back as a thrown thing does.
    pub fn height(&self, now: Instant) -> f32 {
        if self.over(now) {
            return 0.0;
        }
        let x = self.phase(now);
        4.0 * x * (1.0 - x)
    }

    /// How much it shrinks at `now`, from 0 (its size) to 1 (the most):
    /// where hopping would leave the bar, it beats instead.
    pub fn beat(&self, now: Instant) -> f32 {
        if self.over(now) {
            return 0.0;
        }
        let x = self.phase(now);
        (1.0 - (std::f32::consts::TAU * x).cos()) / 2.0
    }

    /// Where it is in its hop, from 0 to 1.
    fn phase(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.since).as_secs_f32();
        (elapsed / self.hop.as_secs_f32()).fract()
    }
}

/// How big a tile that came in at `since` is at `now`: it grows past its
/// size a little and settles back.
pub fn arrival_scale(since: Instant, now: Instant, pace: Pace) -> f32 {
    let duration = pace.of(durations::ARRIVAL);
    if duration.is_zero() {
        return 1.0;
    }
    let elapsed = now.saturating_duration_since(since).as_secs_f32();
    let progress = (elapsed / duration.as_secs_f32()).min(1.0);
    0.4 + 0.6 * ease_out_back(progress)
}

/// Whether a tile that came in at `since` still grows.
pub fn arriving(since: Instant, now: Instant, pace: Pace) -> bool {
    now.saturating_duration_since(since) < pace.of(durations::ARRIVAL)
}

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in(t: f32) -> f32 {
    t.powi(3)
}

/// Out, past the end a little, and back.
fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn a_reveal_comes_in_and_stays() {
        let now = Instant::now();
        let reveal = Reveal::coming(now, 200 * MS);

        assert_eq!(reveal.at(now), 0.0);
        assert!(reveal.at(now + 100 * MS) > 0.5, "quick at first");
        assert_eq!(reveal.at(now + 200 * MS), 1.0);
        assert!(!reveal.moving(now + 300 * MS));
    }

    #[test]
    fn a_reveal_turned_back_halfway_does_not_jump() {
        let now = Instant::now();
        let mut reveal = Reveal::coming(now, 200 * MS);
        let halfway = now + 50 * MS;
        let shown = reveal.at(halfway);
        reveal.go(0.0, halfway, 200 * MS);

        assert_eq!(reveal.at(halfway), shown);
        // Less of the way, less of the time.
        assert!(!reveal.moving(halfway + 200 * MS));
        assert_eq!(reveal.at(halfway + 200 * MS), 0.0);
    }

    #[test]
    fn without_animations_nothing_takes_time() {
        let pace = Pace::new(false, 1.0);
        let now = Instant::now();

        assert_eq!(pace.of(durations::DRAWER), Duration::ZERO);
        assert_eq!(Reveal::coming(now, pace.of(durations::DRAWER)).at(now), 1.0);
        assert_eq!(arrival_scale(now, now, pace), 1.0);
        assert!(Launch::new("kitty".into(), 0, now, pace).over(now));
    }

    #[test]
    fn a_slower_pace_stretches_the_time() {
        let pace = Pace::new(true, 10.0);

        assert_eq!(pace.of(100 * MS), 1000 * MS);
        assert_eq!(Pace::new(true, f32::NAN), Pace::default());
    }

    #[test]
    fn an_icon_hops_until_a_window_comes_and_finishes_its_hop() {
        let now = Instant::now();
        let pace = Pace::default();
        let mut launch = Launch::new("kitty".into(), 1, now, pace);
        let hop = durations::HOP;

        assert_eq!(launch.height(now), 0.0);
        assert!((launch.height(now + hop / 2) - 1.0).abs() < 1e-3);
        launch.watch(1, now + hop / 4);
        assert!(!launch.over(now + hop * 3), "no new window yet");

        // A window comes a quarter into the second hop.
        let came = now + hop + hop / 4;
        launch.watch(2, came);
        assert!(!launch.over(came));
        assert!(launch.height(came) > 0.0);
        assert!(launch.over(now + hop * 2));
    }

    #[test]
    fn an_icon_stops_hopping_when_nothing_comes() {
        let now = Instant::now();
        let mut launch = Launch::new("ghost".into(), 0, now, Pace::default());
        let late = now + durations::LAUNCH;
        launch.watch(0, late);

        assert!(launch.over(late + durations::HOP));
    }

    #[test]
    fn an_icon_that_cannot_hop_beats() {
        let now = Instant::now();
        let launch = Launch::new("kitty".into(), 0, now, Pace::default());

        assert_eq!(launch.beat(now), 0.0);
        assert!((launch.beat(now + durations::HOP / 2) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_tile_grows_past_its_size_and_settles() {
        let now = Instant::now();
        let pace = Pace::default();
        let peak = (1..10)
            .map(|tenth| arrival_scale(now, now + durations::ARRIVAL / 10 * tenth, pace))
            .fold(0.0, f32::max);

        assert!((arrival_scale(now, now, pace) - 0.4).abs() < 1e-6);
        assert!(peak > 1.0 && peak < 1.1);
        assert_eq!(arrival_scale(now, now + durations::ARRIVAL, pace), 1.0);
        assert!(!arriving(now, now + durations::ARRIVAL, pace));
    }
}
