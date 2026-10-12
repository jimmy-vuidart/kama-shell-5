//! The mixer: one line per application playing sound, whatever the number
//! of streams it opened, and the volumes kept from one session to the next.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Identity;

/// How much one notch of the wheel changes a volume.
const NOTCH: f32 = 0.05;

/// A stream of sound, as the audio server describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub id: u32,
    /// What identifies its application: the name of its program.
    pub app: String,
    /// What it plays, when it says.
    pub media: String,
    /// From 0 (silent) to 1 (full).
    pub volume: f32,
    pub muted: bool,
    /// Open, but not playing at the moment.
    pub paused: bool,
}

/// A line of the mixer: an application and all its streams.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub identity: Identity,
    /// "En lecture", "En pause", "Coupé", or what is being played.
    pub state: String,
    pub volume: f32,
    pub muted: bool,
    pub streams: Vec<u32>,
}

/// The lines of the mixer, by application name. An application with several
/// streams shows the loudest one; it is muted when all of them are.
pub fn lines(streams: &[Stream], identify: &dyn Fn(&str) -> Identity) -> Vec<Line> {
    let mut groups: Vec<(Identity, Vec<&Stream>)> = Vec::new();
    for stream in streams {
        let identity = identify(&stream.app);
        match groups
            .iter_mut()
            .find(|(known, _)| known.key == identity.key)
        {
            Some((_, group)) => group.push(stream),
            None => groups.push((identity, vec![stream])),
        }
    }
    groups.sort_by_key(|(identity, _)| super::drawer::fold(&identity.name));

    let line = |(identity, group): (Identity, Vec<&Stream>)| {
        let muted = group.iter().all(|stream| stream.muted);
        let playing = group.iter().find(|stream| !stream.paused);
        let state = match (muted, playing) {
            (true, _) => "Coupé".to_owned(),
            (false, None) => "En pause".to_owned(),
            (false, Some(stream)) if stream.media.is_empty() => "En lecture".to_owned(),
            (false, Some(stream)) => stream.media.clone(),
        };
        Line {
            identity,
            state,
            volume: group.iter().map(|stream| stream.volume).fold(0.0, f32::max),
            muted,
            streams: group.iter().map(|stream| stream.id).collect(),
        }
    };
    groups.into_iter().map(line).collect()
}

/// A volume kept inside what a slider shows.
pub fn clamp(volume: f32) -> f32 {
    if volume.is_nan() {
        0.0
    } else {
        volume.clamp(0.0, 1.0)
    }
}

/// The volume after the wheel turned over a slider: up when it turns up.
pub fn scrolled(volume: f32, notches_up: f32) -> f32 {
    clamp(volume + notches_up * NOTCH)
}

/// What an application sounded like when it was last heard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub volume: f32,
    #[serde(default)]
    pub muted: bool,
}

/// The volumes of applications, kept between sessions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Volumes {
    apps: BTreeMap<String, Saved>,
}

impl Volumes {
    /// Notes the volume the user gave an application. Returns whether that
    /// is news, to save only then.
    pub fn remember(&mut self, key: &str, volume: f32, muted: bool) -> bool {
        let saved = Saved {
            volume: clamp(volume),
            muted,
        };
        self.apps.insert(key.to_owned(), saved) != Some(saved)
    }

    /// What to give back to an application that starts playing: its volume
    /// of last time, unless it already has it.
    pub fn restore(&self, key: &str, stream: &Stream) -> Option<Saved> {
        let saved = *self.apps.get(key)?;
        let same = (saved.volume - stream.volume).abs() < 0.005 && saved.muted == stream.muted;
        (!same).then_some(saved)
    }

    /// Reads the volumes kept in a file; none when there is no file, or
    /// when it cannot be read.
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str) -> Identity {
        // As a catalog would: "firefox" and "Firefox" are one application.
        let key = name.to_lowercase();
        Identity {
            name: key.clone(),
            icon: None,
            cover: None,
            command: None,
            names: vec![key.clone()],
            game: false,
            key,
        }
    }

    fn stream(id: u32, app: &str, volume: f32) -> Stream {
        Stream {
            id,
            app: app.into(),
            media: String::new(),
            volume,
            muted: false,
            paused: false,
        }
    }

    #[test]
    fn each_application_has_one_line_by_name() {
        let streams = [stream(1, "mpv", 0.4), stream(2, "discord", 0.8)];

        let lines = lines(&streams, &identity);

        let keys: Vec<&str> = lines
            .iter()
            .map(|line| line.identity.key.as_str())
            .collect();
        assert_eq!(keys, ["discord", "mpv"]);
    }

    #[test]
    fn the_streams_of_an_application_share_a_line() {
        let streams = [
            stream(1, "firefox", 0.3),
            stream(2, "Firefox", 0.7),
            stream(3, "mpv", 0.5),
        ];

        let lines = lines(&streams, &identity);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].streams, [1, 2]);
        assert!((lines[0].volume - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn an_application_is_muted_when_all_its_streams_are() {
        let mut first = stream(1, "firefox", 0.3);
        let mut second = stream(2, "firefox", 0.7);
        first.muted = true;

        let half = lines(&[first.clone(), second.clone()], &identity);
        second.muted = true;
        let all = lines(&[first, second], &identity);

        assert!(!half[0].muted);
        assert!(all[0].muted);
        assert_eq!(all[0].state, "Coupé");
    }

    #[test]
    fn the_state_says_what_plays_or_that_nothing_does() {
        let mut video = stream(1, "firefox", 0.5);
        video.media = "Guide du raid".into();
        let mut idle = stream(2, "mpv", 0.5);
        idle.paused = true;

        let lines = lines(&[video, idle, stream(3, "discord", 0.5)], &identity);

        let states: Vec<&str> = lines.iter().map(|line| line.state.as_str()).collect();
        assert_eq!(states, ["En lecture", "Guide du raid", "En pause"]);
    }

    #[test]
    fn the_wheel_changes_a_volume_by_steps_within_bounds() {
        assert!((scrolled(0.5, 1.0) - 0.55).abs() < 1e-6);
        assert!((scrolled(0.5, -2.0) - 0.4).abs() < 1e-6);
        assert!((scrolled(0.98, 1.0) - 1.0).abs() < 1e-6);
        assert!(scrolled(0.02, -1.0).abs() < 1e-6);
        assert!(clamp(f32::NAN).abs() < 1e-6);
    }

    #[test]
    fn an_application_gets_back_the_volume_it_had() {
        let mut volumes = Volumes::default();
        volumes.remember("mpv", 0.25, false);

        let restored = volumes.restore("mpv", &stream(7, "mpv", 1.0));

        assert_eq!(
            restored,
            Some(Saved {
                volume: 0.25,
                muted: false
            })
        );
    }

    #[test]
    fn nothing_is_restored_to_a_stream_that_is_right_already() {
        let mut volumes = Volumes::default();
        volumes.remember("mpv", 0.25, false);

        assert_eq!(volumes.restore("mpv", &stream(7, "mpv", 0.25)), None);
        assert_eq!(volumes.restore("vlc", &stream(8, "vlc", 1.0)), None);
    }

    #[test]
    fn a_muted_application_comes_back_muted() {
        let mut volumes = Volumes::default();
        volumes.remember("mpv", 0.6, true);

        let restored = volumes.restore("mpv", &stream(7, "mpv", 0.6));

        assert_eq!(restored.map(|saved| saved.muted), Some(true));
    }

    #[test]
    fn remembering_says_whether_anything_changed() {
        let mut volumes = Volumes::default();

        assert!(volumes.remember("mpv", 0.6, false));
        assert!(!volumes.remember("mpv", 0.6, false));
        assert!(volumes.remember("mpv", 0.6, true));
    }

    #[test]
    fn volumes_survive_being_written_and_read() {
        let mut volumes = Volumes::default();
        volumes.remember("mpv", 0.25, false);
        volumes.remember("discord", 0.8, true);
        let file =
            std::env::temp_dir().join(format!("coquille-volumes-{}.json", std::process::id()));

        volumes.save(&file).unwrap();
        let read = Volumes::load(&file);
        let _ = std::fs::remove_file(&file);

        assert_eq!(read, volumes);
    }

    #[test]
    fn a_missing_or_broken_file_gives_no_volume() {
        let missing = std::env::temp_dir().join("coquille-no-such-volumes.json");

        assert_eq!(Volumes::load(&missing), Volumes::default());
    }
}
