//! The battery of a laptop, as `upower` tells it: how full, whether it
//! charges, how long it lasts; and when to warn that it runs low.

use std::time::Duration;

/// Whether the battery charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Charging,
    Discharging,
    /// Plugged in and full, or not charging for another reason.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// From 0 to 100.
    pub percent: u8,
    pub charge: Charge,
    /// Until empty when discharging, until full when charging.
    pub time: Option<Duration>,
}

/// A warning that the battery runs low.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    /// 10 % left.
    Low,
    /// 5 % left: shown even with « Ne pas déranger ».
    Critical,
}

const LOW: u8 = 10;
const CRITICAL: u8 = 5;

/// The battery among the devices `upower -e` lists: the laptop's own, not
/// a game controller's or a mouse's.
pub fn pick(devices: &str) -> Option<&str> {
    devices.lines().map(str::trim).find(|path| {
        path.rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("battery_"))
    })
}

/// The battery in the answer of `upower -i <device>`, read with `LC_ALL=C`.
pub fn parse(answer: &str) -> Option<Battery> {
    let field = |name: &str| {
        answer.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name).then(|| value.trim())
        })
    };
    if field("present") == Some("no") {
        return None;
    }
    let percent = field("percentage")?
        .trim_end_matches('%')
        .replace(',', ".")
        .parse::<f32>()
        .ok()?
        .round()
        .clamp(0.0, 100.0) as u8;
    let charge = match field("state").unwrap_or_default() {
        "charging" | "pending-charge" => Charge::Charging,
        "discharging" | "pending-discharge" | "empty" => Charge::Discharging,
        _ => Charge::Full,
    };
    let time = match charge {
        Charge::Charging => field("time to full"),
        Charge::Discharging => field("time to empty"),
        Charge::Full => None,
    };
    Some(Battery {
        percent,
        charge,
        time: time.and_then(duration),
    })
}

/// `3.2 hours`, `45.0 minutes`, `1.1 days`.
fn duration(text: &str) -> Option<Duration> {
    let mut words = text.split_whitespace();
    let amount: f64 = words.next()?.replace(',', ".").parse().ok()?;
    let unit = match words.next()? {
        "seconds" | "second" => 1.0,
        "minutes" | "minute" => 60.0,
        "hours" | "hour" => 3600.0,
        "days" | "day" => 86_400.0,
        _ => return None,
    };
    (amount > 0.0).then(|| Duration::from_secs_f64(amount * unit))
}

/// Whether the battery just went low: each warning once on the way down,
/// none while it charges.
pub fn alert(before: Option<Battery>, now: Battery) -> Option<Alert> {
    if now.charge != Charge::Discharging {
        return None;
    }
    let was = before
        .filter(|before| before.charge == Charge::Discharging)
        .map_or(u8::MAX, |before| before.percent);
    if now.percent <= CRITICAL && was > CRITICAL {
        Some(Alert::Critical)
    } else if now.percent <= LOW && was > LOW {
        Some(Alert::Low)
    } else {
        None
    }
}

impl Battery {
    /// `87 %`.
    pub fn label(&self) -> String {
        format!("{}\u{202f}%", self.percent)
    }

    /// What the control center says under it: `En charge · pleine dans
    /// 45 min`, `2 h 10 restantes`, `Branchée`.
    pub fn detail(&self) -> String {
        let time = self.time.map(spoken);
        match (self.charge, time) {
            (Charge::Charging, Some(time)) => format!("En charge · pleine dans {time}"),
            (Charge::Charging, None) => "En charge".to_owned(),
            (Charge::Discharging, Some(time)) => format!("{time} restantes"),
            (Charge::Discharging, None) => "Sur batterie".to_owned(),
            (Charge::Full, _) => "Branchée".to_owned(),
        }
    }
}

impl Alert {
    pub fn summary(self, battery: &Battery) -> String {
        format!("Batterie faible : {}", battery.label())
    }

    pub fn body(self) -> &'static str {
        match self {
            Self::Low => "Branchez le portable bientôt.",
            Self::Critical => "Branchez le portable : il va s'éteindre.",
        }
    }
}

/// `2 h 10`, `45 min`.
fn spoken(time: Duration) -> String {
    let minutes = (time.as_secs() + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "/org/freedesktop/UPower/devices/line_power_AC\n\
        /org/freedesktop/UPower/devices/battery_BAT0\n\
        /org/freedesktop/UPower/devices/gaming_input_dev_0C_35\n\
        /org/freedesktop/UPower/devices/DisplayDevice\n";

    const DISCHARGING: &str = "  native-path:          BAT0
  vendor:               ASUSTeK
  power supply:         yes
  battery
    present:             yes
    rechargeable:        yes
    state:               discharging
    warning-level:       none
    energy:              40.1 Wh
    time to empty:       3.2 hours
    percentage:          76%
    icon-name:          'battery-full-symbolic'
";

    const CHARGING: &str = "  battery
    present:             yes
    state:               charging
    time to full:        45.0 minutes
    percentage:          41,6%
";

    fn battery(percent: u8, charge: Charge) -> Battery {
        Battery {
            percent,
            charge,
            time: None,
        }
    }

    #[test]
    fn the_laptop_battery_is_picked_not_a_controller() {
        assert_eq!(
            pick(DEVICES),
            Some("/org/freedesktop/UPower/devices/battery_BAT0")
        );
        assert_eq!(
            pick("/org/freedesktop/UPower/devices/DisplayDevice\n"),
            None
        );
    }

    #[test]
    fn upower_says_how_full_and_how_long() {
        let discharging = parse(DISCHARGING).unwrap();
        let charging = parse(CHARGING).unwrap();

        assert_eq!(discharging.percent, 76);
        assert_eq!(discharging.charge, Charge::Discharging);
        assert_eq!(discharging.detail(), "3 h 12 restantes");
        assert_eq!(charging.percent, 42);
        assert_eq!(charging.detail(), "En charge · pleine dans 45 min");
        assert_eq!(charging.label(), "42\u{202f}%");
        assert_eq!(parse("    present:   no\n    percentage: 0%\n"), None);
        assert_eq!(battery(100, Charge::Full).detail(), "Branchée");
    }

    #[test]
    fn each_warning_comes_once_on_the_way_down() {
        let at = |percent| battery(percent, Charge::Discharging);

        assert_eq!(alert(Some(at(11)), at(10)), Some(Alert::Low));
        assert_eq!(alert(Some(at(10)), at(9)), None);
        assert_eq!(alert(Some(at(6)), at(5)), Some(Alert::Critical));
        assert_eq!(alert(Some(at(5)), at(4)), None);
        // Unplugged when already low: told at once.
        assert_eq!(
            alert(Some(battery(8, Charge::Charging)), at(8)),
            Some(Alert::Low)
        );
        assert_eq!(alert(None, at(3)), Some(Alert::Critical));
        // Charging: never.
        assert_eq!(alert(Some(at(11)), battery(10, Charge::Charging)), None);
    }
}
