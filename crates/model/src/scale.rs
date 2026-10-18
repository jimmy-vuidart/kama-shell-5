//! How much bigger than its pixels a screen draws everything: from 100 % to
//! 300 %, by steps of 25 %.

use serde::{Deserialize, Serialize};

use crate::geometry::Size;

/// The scale of a screen. Kept as a number of quarters, so that only the
/// steps offered exist, and every one is exact in the fractional scale
/// protocol (which counts in 120ths).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "f64", into = "f64")]
pub struct Scale(u8);

impl Scale {
    pub const ONE: Self = Self(4);
    const MIN: u8 = 4;
    const MAX: u8 = 12;

    /// Every scale offered, from the smallest.
    pub const ALL: [Self; 9] = [
        Self(4),
        Self(5),
        Self(6),
        Self(7),
        Self(8),
        Self(9),
        Self(10),
        Self(11),
        Self(12),
    ];

    /// The nearest step, within 100 % and 300 %.
    pub fn new(value: f64) -> Self {
        if !value.is_finite() {
            return Self::ONE;
        }
        let quarters = (value * 4.0)
            .round()
            .clamp(Self::MIN.into(), Self::MAX.into());
        Self(quarters as u8)
    }

    pub fn get(self) -> f64 {
        f64::from(self.0) / 4.0
    }

    pub fn percent(self) -> u32 {
        u32::from(self.0) * 25
    }

    /// User-visible: « 125 % ».
    pub fn label(self) -> String {
        format!("{}\u{202f}%", self.percent())
    }

    /// The scale clients that only know whole numbers are told
    /// (`wl_surface.preferred_buffer_scale`, `wl_output.scale`): rounded up,
    /// so that they draw sharp and the compositor shrinks.
    pub fn integer(self) -> i32 {
        i32::from(self.0.div_ceil(4))
    }

    /// How many pixels show `size`, logical.
    pub fn physical(self, size: Size) -> Size {
        let grow = |length: i32| (f64::from(length) * self.get()).round() as i32;
        Size::new(grow(size.w), grow(size.h))
    }

    /// The logical size of a screen of `pixels`.
    pub fn logical(self, pixels: Size) -> Size {
        let shrink = |length: i32| (f64::from(length) / self.get()).round() as i32;
        Size::new(shrink(pixels.w), shrink(pixels.h))
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::ONE
    }
}

impl From<f64> for Scale {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl From<Scale> for f64 {
    fn from(scale: Scale) -> Self {
        scale.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_goes_to_the_nearest_quarter_within_the_offered_range() {
        assert_eq!(Scale::new(1.0), Scale::ONE);
        assert_eq!(Scale::new(1.3).get(), 1.25);
        assert_eq!(Scale::new(1.6).get(), 1.5);
        assert_eq!(Scale::new(0.5), Scale::ONE);
        assert_eq!(Scale::new(5.0).get(), 3.0);
        assert_eq!(Scale::new(f64::NAN), Scale::ONE);
    }

    #[test]
    fn nine_steps_are_offered_from_100_to_300_percent() {
        let percents: Vec<u32> = Scale::ALL.iter().map(|scale| scale.percent()).collect();

        assert_eq!(percents, [100, 125, 150, 175, 200, 225, 250, 275, 300]);
        assert_eq!(Scale::new(1.75).label(), "175\u{202f}%");
    }

    #[test]
    fn whole_number_clients_are_told_the_scale_rounded_up() {
        assert_eq!(Scale::ONE.integer(), 1);
        assert_eq!(Scale::new(1.25).integer(), 2);
        assert_eq!(Scale::new(2.0).integer(), 2);
        assert_eq!(Scale::new(2.25).integer(), 3);
    }

    #[test]
    fn a_4k_screen_at_200_percent_is_a_full_hd_one() {
        let uhd = Size::new(3840, 2160);

        assert_eq!(Scale::new(2.0).logical(uhd), Size::new(1920, 1080));
        assert_eq!(Scale::new(1.5).logical(uhd), Size::new(2560, 1440));
        assert_eq!(Scale::new(1.75).logical(uhd), Size::new(2194, 1234));
        assert_eq!(Scale::ONE.logical(uhd), uhd);
        assert_eq!(
            Scale::new(1.25).physical(Size::new(100, 38)),
            Size::new(125, 48)
        );
    }

    #[test]
    fn a_scale_is_written_as_a_plain_number() {
        let scale: Scale = serde_json::from_str("1.5").unwrap();

        assert_eq!(scale.get(), 1.5);
        assert_eq!(serde_json::to_string(&Scale::new(2.0)).unwrap(), "2.0");
        assert_eq!(serde_json::from_str::<Scale>("2").unwrap().get(), 2.0);
    }
}
