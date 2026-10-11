//! Game mode: pure decisions about the pointer and the shell modifier, shared
//! by the nested and the hardware backends.

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

#[cfg(test)]
mod tests {
    use super::*;

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
