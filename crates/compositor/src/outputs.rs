//! Which screen is which, where each one sits, and what the pointer may reach.
//! Pure logic, shared by the nested and the hardware backends.

use coquille_model::{OutputRole, Point, Rect, Size};
use serde::Deserialize;

/// Connector names given a role in the configuration file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoleConfig {
    pub left: Option<String>,
    pub center: Option<String>,
    pub right: Option<String>,
}

/// A plugged screen, as the backend found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    /// Connector name, such as `DP-2`.
    pub name: String,
    pub size: Size,
}

/// A display mode offered by a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub size: Size,
    /// In mHz.
    pub refresh: i32,
    pub preferred: bool,
}

/// The role of each screen, in the order given. Configured names come first;
/// then the widest remaining screen is the center, and the others are the
/// left and right ones in the order they were found. A fourth screen gets
/// no role.
pub fn assign_roles(screens: &[Connected], config: &RoleConfig) -> Vec<Option<OutputRole>> {
    let mut roles = vec![None; screens.len()];
    let mut free = vec![OutputRole::Center, OutputRole::Left, OutputRole::Right];

    let configured = [
        (&config.left, OutputRole::Left),
        (&config.center, OutputRole::Center),
        (&config.right, OutputRole::Right),
    ];
    for (name, role) in configured {
        let index = name
            .as_ref()
            .and_then(|name| screens.iter().position(|screen| screen.name == *name));
        if let Some(index) = index.filter(|index| roles[*index].is_none()) {
            roles[index] = Some(role);
            free.retain(|other| *other != role);
        }
    }

    // The center goes to the widest screen left; `w1 / h1 > w2 / h2` is
    // compared without division. Ties keep the first screen found.
    if free.first() == Some(&OutputRole::Center) {
        let unassigned = (0..screens.len()).filter(|index| roles[*index].is_none());
        let widest = unassigned.reduce(|best, index| {
            let (a, b) = (screens[best].size, screens[index].size);
            let wider = i64::from(b.w) * i64::from(a.h) > i64::from(a.w) * i64::from(b.h);
            if wider { index } else { best }
        });
        if let Some(widest) = widest {
            roles[widest] = Some(free.remove(0));
        }
    }

    let unassigned = roles.iter_mut().filter(|role| role.is_none());
    for (role, free) in unassigned.zip(free) {
        *role = Some(free);
    }
    roles
}

/// Top-left corner of each screen: side by side from left to right, aligned
/// at the top.
pub fn origins(screens: &[(OutputRole, Size)]) -> Vec<Point> {
    screens
        .iter()
        .map(|(role, _)| {
            let before = screens.iter().filter(|(other, _)| other < role);
            Point::new(before.map(|(_, size)| size.w).sum(), 0)
        })
        .collect()
}

/// The closest position to `position` that is on a screen.
pub fn constrain(position: (f64, f64), screens: &[Rect]) -> (f64, f64) {
    let clamped = screens.iter().filter(|rect| !rect.is_empty()).map(|rect| {
        // The last reachable position of a screen is its last pixel.
        let x = position
            .0
            .clamp(f64::from(rect.x), f64::from(rect.right() - 1));
        let y = position
            .1
            .clamp(f64::from(rect.y), f64::from(rect.bottom() - 1));
        let distance = (x - position.0).powi(2) + (y - position.1).powi(2);
        (distance, (x, y))
    });
    clamped
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(position, |(_, closest)| closest)
}

/// Where the pointer goes when it moves from `from` to `to`. Passing from
/// one screen to the one beside it keeps the same share of the height: the
/// screens are aligned at the top, and without this the bottom of the tall
/// center would lead nowhere (a wall 360 px high on each side). Both edges
/// of each screen meet those of its neighbour, as they do on a desk where
/// the screens are about as tall as each other.
pub fn cross(from: (f64, f64), to: (f64, f64), screens: &[Rect]) -> (f64, f64) {
    let on = |rect: &Rect, (x, y): (f64, f64)| {
        !rect.is_empty()
            && (f64::from(rect.x)..f64::from(rect.right())).contains(&x)
            && (f64::from(rect.y)..f64::from(rect.bottom())).contains(&y)
    };
    let Some(left) = screens.iter().find(|rect| on(rect, from)) else {
        return constrain(to, screens);
    };
    if on(left, to) {
        return to;
    }
    let entered = screens.iter().find(|rect| {
        *rect != left
            && !rect.is_empty()
            && (f64::from(rect.x)..f64::from(rect.right())).contains(&to.0)
    });
    let Some(entered) = entered else {
        return constrain(to, screens);
    };
    let share = (to.1 - f64::from(left.y)) / f64::from(left.h);
    let y = f64::from(entered.y) + share * f64::from(entered.h);
    (
        to.0,
        y.clamp(f64::from(entered.y), f64::from(entered.bottom() - 1)),
    )
}

/// Splits the nested window into three simulated screens, in the proportions
/// of the real setup (16:9, 21:9, 16:9 side by side).
pub fn split_canvas(size: Size) -> [(OutputRole, Rect); 3] {
    const SIDE: i64 = 1920;
    const TOTAL: i64 = 1920 + 3440 + 1920;
    let side = ((i64::from(size.w) * SIDE + TOTAL / 2) / TOTAL) as i32;
    let center = size.w - 2 * side;
    [
        (OutputRole::Left, Rect::new(0, 0, side, size.h)),
        (OutputRole::Center, Rect::new(side, 0, center, size.h)),
        (OutputRole::Right, Rect::new(side + center, 0, side, size.h)),
    ]
}

/// The mode to use: the preferred resolution (else the largest), at its
/// highest refresh rate.
pub fn pick_mode(modes: &[ModeInfo]) -> Option<usize> {
    let area = |mode: &ModeInfo| i64::from(mode.size.w) * i64::from(mode.size.h);
    let reference = modes
        .iter()
        .find(|mode| mode.preferred)
        .or_else(|| modes.iter().max_by_key(|mode| area(mode)))?;
    let fastest = modes
        .iter()
        .enumerate()
        .filter(|(_, mode)| mode.size == reference.size)
        // Ties keep the first mode listed.
        .min_by_key(|(_, mode)| std::cmp::Reverse(mode.refresh))?;
    Some(fastest.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Size = Size::new(1920, 1080);
    const ULTRAWIDE: Size = Size::new(3440, 1440);

    fn screen(name: &str, size: Size) -> Connected {
        Connected {
            name: name.to_owned(),
            size,
        }
    }

    /// The real setup, in the order the kernel lists the connectors.
    fn desk() -> Vec<Connected> {
        vec![
            screen("DP-1", FULL_HD),
            screen("DP-2", ULTRAWIDE),
            screen("HDMI-A-1", FULL_HD),
        ]
    }

    #[test]
    fn configured_names_get_their_role() {
        let config = RoleConfig {
            left: Some("HDMI-A-1".into()),
            center: Some("DP-2".into()),
            right: Some("DP-1".into()),
        };

        assert_eq!(
            assign_roles(&desk(), &config),
            [
                Some(OutputRole::Right),
                Some(OutputRole::Center),
                Some(OutputRole::Left)
            ]
        );
    }

    #[test]
    fn without_configuration_the_widest_screen_is_the_center() {
        assert_eq!(
            assign_roles(&desk(), &RoleConfig::default()),
            [
                Some(OutputRole::Left),
                Some(OutputRole::Center),
                Some(OutputRole::Right)
            ]
        );
    }

    #[test]
    fn a_single_screen_is_the_center() {
        let screens = [screen("eDP-1", FULL_HD)];

        assert_eq!(
            assign_roles(&screens, &RoleConfig::default()),
            [Some(OutputRole::Center)]
        );
    }

    #[test]
    fn a_partial_configuration_leaves_the_rest_automatic() {
        let config = RoleConfig {
            right: Some("HDMI-A-1".into()),
            ..RoleConfig::default()
        };

        assert_eq!(
            assign_roles(&desk(), &config),
            [
                Some(OutputRole::Left),
                Some(OutputRole::Center),
                Some(OutputRole::Right)
            ]
        );
    }

    #[test]
    fn a_configured_name_that_is_not_plugged_is_ignored() {
        let config = RoleConfig {
            center: Some("DP-9".into()),
            ..RoleConfig::default()
        };
        let screens = [screen("DP-1", FULL_HD)];

        assert_eq!(assign_roles(&screens, &config), [Some(OutputRole::Center)]);
    }

    #[test]
    fn a_fourth_screen_gets_no_role() {
        let mut screens = desk();
        screens.push(screen("DP-3", FULL_HD));

        assert_eq!(assign_roles(&screens, &RoleConfig::default())[3], None);
    }

    #[test]
    fn screens_sit_side_by_side_from_left_to_right() {
        let screens = [
            (OutputRole::Right, FULL_HD),
            (OutputRole::Center, ULTRAWIDE),
            (OutputRole::Left, FULL_HD),
        ];

        assert_eq!(
            origins(&screens),
            [Point::new(5360, 0), Point::new(1920, 0), Point::new(0, 0)]
        );
    }

    #[test]
    fn a_missing_screen_leaves_no_hole() {
        let screens = [
            (OutputRole::Center, ULTRAWIDE),
            (OutputRole::Right, FULL_HD),
        ];

        assert_eq!(origins(&screens), [Point::new(0, 0), Point::new(3440, 0)]);
    }

    const LAYOUT: [Rect; 3] = [
        Rect::new(0, 0, 1920, 1080),
        Rect::new(1920, 0, 3440, 1440),
        Rect::new(5360, 0, 1920, 1080),
    ];

    #[test]
    fn a_position_on_a_screen_is_kept() {
        assert_eq!(constrain((2000.5, 1300.25), &LAYOUT), (2000.5, 1300.25));
    }

    #[test]
    fn a_position_past_the_last_screen_stops_at_its_edge() {
        assert_eq!(constrain((9000.0, 500.0), &LAYOUT), (7279.0, 500.0));
        assert_eq!(constrain((-40.0, -3.0), &LAYOUT), (0.0, 0.0));
    }

    #[test]
    fn leaving_the_tall_center_below_a_side_screen_stops_at_the_center_edge() {
        assert_eq!(constrain((1910.0, 1300.0), &LAYOUT), (1920.0, 1300.0));
    }

    #[test]
    fn a_position_under_a_side_screen_goes_to_its_bottom_edge() {
        assert_eq!(constrain((500.0, 1100.0), &LAYOUT), (500.0, 1079.0));
    }

    #[test]
    fn the_bottom_of_the_center_leads_to_the_bottom_of_a_side_screen() {
        let (x, y) = cross((1925.0, 1400.0), (1915.0, 1400.0), &LAYOUT);

        assert_eq!(x, 1915.0);
        assert!((1045.0..1080.0).contains(&y), "{y}");
    }

    #[test]
    fn crossing_keeps_the_share_of_the_height_both_ways() {
        let (_, low) = cross((1925.0, 720.0), (1915.0, 720.0), &LAYOUT);
        let (_, high) = cross((5365.0, 540.0), (5355.0, 540.0), &LAYOUT);

        assert_eq!(low, 540.0);
        assert_eq!(high, 720.0);
    }

    #[test]
    fn the_top_edges_still_meet() {
        assert_eq!(cross((5.0, 0.0), (1925.0, 0.0), &LAYOUT), (1925.0, 0.0));
    }

    #[test]
    fn a_move_inside_a_screen_is_kept() {
        assert_eq!(
            cross((2000.0, 1300.0), (2010.0, 1310.5), &LAYOUT),
            (2010.0, 1310.5)
        );
    }

    #[test]
    fn a_move_past_the_bottom_of_a_screen_stops_there() {
        assert_eq!(
            cross((500.0, 1070.0), (500.0, 1100.0), &LAYOUT),
            (500.0, 1079.0)
        );
        assert_eq!(cross((9.0, 9.0), (-40.0, 9.0), &LAYOUT), (0.0, 9.0));
    }

    #[test]
    fn a_pointer_off_every_screen_comes_back_to_the_closest_one() {
        assert_eq!(cross((9000.0, 9.0), (9001.0, 9.0), &LAYOUT), (7279.0, 9.0));
    }

    #[test]
    fn without_any_screen_the_position_is_kept() {
        assert_eq!(constrain((5.0, 6.0), &[]), (5.0, 6.0));
    }

    #[test]
    fn the_nested_window_is_split_like_the_real_setup() {
        let [left, center, right] = split_canvas(Size::new(1600, 900));

        assert_eq!(left, (OutputRole::Left, Rect::new(0, 0, 422, 900)));
        assert_eq!(center, (OutputRole::Center, Rect::new(422, 0, 756, 900)));
        assert_eq!(right, (OutputRole::Right, Rect::new(1178, 0, 422, 900)));
    }

    fn mode(w: i32, h: i32, refresh: i32, preferred: bool) -> ModeInfo {
        ModeInfo {
            size: Size::new(w, h),
            refresh,
            preferred,
        }
    }

    #[test]
    fn the_preferred_resolution_is_used_at_its_highest_refresh_rate() {
        let modes = [
            mode(3440, 1440, 59_973, true),
            mode(3440, 1440, 120_000, false),
            mode(2560, 1080, 144_000, false),
        ];

        assert_eq!(pick_mode(&modes), Some(1));
    }

    #[test]
    fn without_a_preferred_mode_the_largest_resolution_is_used() {
        let modes = [
            mode(1280, 720, 60_000, false),
            mode(1920, 1080, 60_000, false),
        ];

        assert_eq!(pick_mode(&modes), Some(1));
    }

    #[test]
    fn a_screen_without_modes_has_nothing_to_pick() {
        assert_eq!(pick_mode(&[]), None);
    }
}
