//! The backlight of a laptop's screen, from `/sys/class/backlight`: read
//! there (anyone may), set through logind (`SetBrightness` on the session,
//! which needs neither root nor another package). A desktop has none: its
//! screens are reached over DDC/CI instead (`services.rs`).

use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::warn;

const CLASS: &str = "/sys/class/backlight";

/// A backlight device, as the kernel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlight {
    /// Its name in `/sys/class/backlight` (`intel_backlight`, `amdgpu_bl1`).
    pub name: String,
    /// The value of full brightness.
    pub max: u32,
}

/// Which of several backlights drives the screen: the one of the firmware,
/// then the one of the platform, then a raw one (the GPU's own), as
/// systemd chooses. `devices` gives each name with its `type`.
pub fn pick(devices: &[(String, String)]) -> Option<&str> {
    let rank = |kind: &str| match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        "raw" => 2,
        _ => 3,
    };
    devices
        .iter()
        .min_by_key(|(name, kind)| (rank(kind), name.clone()))
        .map(|(name, _)| name.as_str())
}

/// The brightness from 0 to 1, given the values the kernel says.
pub fn level(current: u32, max: u32) -> Option<f32> {
    (max > 0).then(|| (current as f32 / max as f32).clamp(0.0, 1.0))
}

/// The value to give for a brightness from 0 to 1: never 0, which turns
/// some screens off.
pub fn value(level: f32, max: u32) -> u32 {
    let wanted = (level.clamp(0.0, 1.0) * max as f32).round() as u32;
    wanted.clamp(1.min(max), max)
}

fn read_number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The backlight of this machine, if it has one.
pub fn find() -> Option<Backlight> {
    find_in(Path::new(CLASS))
}

fn find_in(class: &Path) -> Option<Backlight> {
    let entries = std::fs::read_dir(class).ok()?;
    let devices: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = std::fs::read_to_string(entry.path().join("type")).unwrap_or_default();
            (name, kind)
        })
        .collect();
    let name = pick(&devices)?.to_owned();
    let max = read_number(&class.join(&name).join("max_brightness"))?;
    (max > 0).then_some(Backlight { name, max })
}

impl Backlight {
    fn path(&self) -> PathBuf {
        Path::new(CLASS).join(&self.name)
    }

    /// The brightness now, from 0 to 1.
    pub fn read(&self) -> Option<f32> {
        let current = read_number(&self.path().join("brightness"))?;
        level(current, self.max)
    }

    /// Sets the brightness, from 0 to 1, through logind.
    pub async fn set(&self, wanted: f32) {
        let value = value(wanted, self.max).to_string();
        let status = Command::new("busctl")
            .args([
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1/session/auto",
                "org.freedesktop.login1.Session",
                "SetBrightness",
                "ssu",
                "backlight",
                &self.name,
                &value,
            ])
            .status()
            .await;
        match status {
            Ok(status) if status.success() => {}
            Ok(status) => warn!(%status, device = self.name, "logind refused the brightness"),
            Err(error) => warn!(%error, "cannot run busctl: the brightness stays as it is"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, kind: &str) -> (String, String) {
        (name.to_owned(), format!("{kind}\n"))
    }

    #[test]
    fn the_firmware_backlight_wins_over_the_gpu_one() {
        let devices = [
            device("amdgpu_bl1", "raw"),
            device("acpi_video0", "firmware"),
        ];

        assert_eq!(pick(&devices), Some("acpi_video0"));
        assert_eq!(
            pick(&[device("intel_backlight", "raw")]),
            Some("intel_backlight")
        );
        assert_eq!(pick(&[]), None);
    }

    #[test]
    fn brightness_goes_between_values_and_levels() {
        assert_eq!(level(19200, 96000), Some(0.2));
        assert_eq!(level(5, 0), None);
        assert_eq!(value(0.5, 96000), 48000);
        // Never quite off.
        assert_eq!(value(0.0, 255), 1);
        assert_eq!(value(2.0, 255), 255);
    }

    #[test]
    fn a_machine_without_backlight_has_none() {
        let empty = std::env::temp_dir().join("coquille-no-backlight");
        let _ = std::fs::create_dir_all(&empty);

        assert_eq!(find_in(&empty), None);
        assert_eq!(find_in(Path::new("/nonexistent")), None);
    }
}
