//! Sound, through the PulseAudio interface of PipeWire: the volume of the
//! output, one stream per application playing, and how loud each one is
//! while the mixer is open.
//!
//! Everything goes through `pactl` and `parec`, which PipeWire ships for
//! that purpose: no library to link, nothing to keep in step with the
//! server.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::process::Stdio;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream as FutureStream};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, warn};

use super::{Audio, AudioSink, Backend, Request, Update};
use crate::model::mixer::Stream;

/// The loudest a PulseAudio volume goes without amplification.
const FULL: f64 = 65536.0;

/// The volume of an object of `pactl`: the loudest of its channels, from 0
/// to 1.
fn volume_of(object: &Value) -> f32 {
    let channels = object["volume"].as_object();
    let loudest = channels
        .into_iter()
        .flat_map(|channels| channels.values())
        .filter_map(|channel| channel["value"].as_f64())
        .fold(0.0, f64::max);
    (loudest / FULL).clamp(0.0, 1.0) as f32
}

/// The streams of `pactl -f json list sink-inputs`, without those nobody
/// listens to (the speech synthesizer keeps one open at all times).
pub fn parse_streams(json: &str) -> Vec<Stream> {
    let inputs: Vec<Value> = serde_json::from_str(json).unwrap_or_default();
    let stream = |input: &Value| {
        let property = |name: &str| input["properties"][name].as_str().unwrap_or_default();
        let program = property("application.process.binary");
        let name = property("application.name");
        if program.starts_with("sd_") || name.starts_with("speech-dispatcher") {
            return None;
        }
        let app = if program.is_empty() { name } else { program };
        // What plays is worth showing only when it says more than "audio".
        let media = property("media.name");
        let vague = ["playback", "audio stream", "audiostream"];
        let media = if vague.contains(&media.to_lowercase().as_str()) {
            ""
        } else {
            media
        };
        Some(Stream {
            id: u32::try_from(input["index"].as_u64()?).ok()?,
            app: app.to_owned(),
            media: media.to_owned(),
            volume: volume_of(input),
            muted: input["mute"].as_bool().unwrap_or(false),
            paused: input["corked"].as_bool().unwrap_or(false),
        })
    };
    inputs.iter().filter_map(stream).collect()
}

/// The output named `default` in `pactl -f json list sinks`: its name for
/// the user, its volume, whether it is muted.
pub fn parse_output(json: &str, default: &str) -> Option<(String, f32, bool)> {
    let sinks: Vec<Value> = serde_json::from_str(json).ok()?;
    let sink = sinks
        .iter()
        .find(|sink| sink["name"].as_str() == Some(default))?;
    let described = sink["description"]
        .as_str()
        .filter(|text| !text.is_empty() && *text != "(null)");
    let nick = sink["properties"]["node.nick"].as_str();
    let name = described.or(nick).unwrap_or(default);
    Some((
        name.to_owned(),
        volume_of(sink),
        sink["mute"].as_bool().unwrap_or(false),
    ))
}

/// The name of the default output in `pactl -f json info`.
pub fn parse_default(json: &str) -> Option<String> {
    let info: Value = serde_json::from_str(json).ok()?;
    info["default_sink_name"].as_str().map(str::to_owned)
}

/// The available output sinks in `pactl -f json list sinks`.
pub fn parse_sinks(json: &str) -> Vec<AudioSink> {
    let sinks: Vec<Value> = serde_json::from_str(json).unwrap_or_default();
    sinks
        .into_iter()
        .filter_map(|sink| {
            let name = sink["name"].as_str()?.to_owned();
            let described = sink["description"]
                .as_str()
                .filter(|text| !text.is_empty() && *text != "(null)");
            let nick = sink["properties"]["node.nick"].as_str();
            let description = described.or(nick).unwrap_or(&name).to_owned();
            Some(AudioSink { name, description })
        })
        .collect()
}

/// How loud a piece of sound is: the peak of its samples (16 bits, little
/// endian), from 0 to 1.
pub fn peak(samples: &[u8]) -> f32 {
    let loudest = samples
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs())
        .max()
        .unwrap_or(0);
    f32::from(loudest) / f32::from(i16::MAX.unsigned_abs())
}

/// A volume as `pactl` takes it.
fn percent(volume: f32) -> String {
    format!("{}%", (volume.clamp(0.0, 1.0) * 100.0).round() as u32)
}

async fn pactl(arguments: &[&str]) -> Option<String> {
    let output = Command::new("pactl")
        .args(arguments)
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads the whole state of the audio server.
async fn read() -> Option<Audio> {
    let default = parse_default(&pactl(&["-f", "json", "info"]).await?)?;
    let sinks_json = pactl(&["-f", "json", "list", "sinks"]).await?;
    let (output, volume, muted) = parse_output(&sinks_json, &default)?;
    let sinks = parse_sinks(&sinks_json);
    let inputs = pactl(&["-f", "json", "list", "sink-inputs"]).await?;
    Some(Audio {
        output,
        output_name: default,
        volume,
        muted,
        streams: parse_streams(&inputs),
        sinks,
    })
}

async fn apply(request: Request) {
    let flag = |on: bool| if on { "1" } else { "0" };
    match request {
        Request::Volume(volume) => {
            pactl(&["set-sink-volume", "@DEFAULT_SINK@", &percent(volume)]).await;
        }
        Request::Mute(muted) => {
            pactl(&["set-sink-mute", "@DEFAULT_SINK@", flag(muted)]).await;
        }
        Request::SelectOutput(name) => {
            pactl(&["set-default-sink", &name]).await;
        }
        Request::StreamVolume(ids, volume) => {
            for id in ids {
                pactl(&["set-sink-input-volume", &id.to_string(), &percent(volume)]).await;
            }
        }
        Request::StreamMute(ids, muted) => {
            for id in ids {
                pactl(&["set-sink-input-mute", &id.to_string(), flag(muted)]).await;
            }
        }
        _ => {}
    }
}

/// Listens to one stream, to tell how loud it is. The sound is read at a
/// low rate and thrown away: only its peak matters.
fn listen(id: u32, levels: UnboundedSender<(u32, f32)>) -> Option<Child> {
    let mut child = Command::new("parec")
        .args(["--raw", "--format=s16le", "--rate=4000", "--channels=1"])
        .args(["--latency-msec=60", "--client-name=coquille-dock"])
        .arg(format!("--monitor-stream={id}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut sound = child.stdout.take()?;
    tokio::spawn(async move {
        let mut samples = [0_u8; 512];
        while let Ok(count) = sound.read(&mut samples).await {
            if count == 0 || levels.send((id, peak(&samples[..count]))).is_err() {
                break;
            }
        }
    });
    Some(child)
}

pub fn subscription() -> Subscription<Update> {
    Subscription::run(service)
}

fn service() -> impl FutureStream<Item = Update> {
    iced::stream::channel(64, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output.send(Update::Link(Backend::Audio, requests)).await;
        let (level_sender, mut heard) = unbounded_channel::<(u32, f32)>();

        // What the server says changed; the dock then reads everything
        // again, which is cheap and never out of step.
        let mut events = Command::new("pactl")
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok();
        let stdout = events.as_mut().and_then(|child| child.stdout.take());
        let Some(stdout) = stdout else {
            warn!("pactl is not there: no sound in the dock");
            return;
        };
        let mut changes = BufReader::new(stdout).lines();

        let mut listeners: HashMap<u32, Child> = HashMap::new();
        let mut listening = false;
        let mut levels: HashMap<u32, f32> = HashMap::new();
        let mut state = Audio::default();
        let mut stale = true;
        let mut pace = tokio::time::interval(Duration::from_millis(80));
        loop {
            if stale {
                stale = false;
                match read().await {
                    Some(audio) => {
                        // One listener per stream while the mixer shows.
                        let ids: Vec<u32> = audio.streams.iter().map(|stream| stream.id).collect();
                        listeners.retain(|id, _| ids.contains(id));
                        levels.retain(|id, _| ids.contains(id));
                        for id in ids.into_iter().filter(|_| listening) {
                            if let Entry::Vacant(place) = listeners.entry(id) {
                                if let Some(child) = listen(id, level_sender.clone()) {
                                    place.insert(child);
                                }
                            }
                        }
                        if audio != state {
                            state = audio.clone();
                            let _ = output.send(Update::Audio(audio)).await;
                        }
                    }
                    None => debug!("the audio server does not answer"),
                }
            }
            tokio::select! {
                change = changes.next_line() => match change {
                    Ok(Some(line)) => {
                        let about = ["sink", "server", "card"];
                        stale = about.iter().any(|kind| line.contains(kind));
                    }
                    _ => {
                        warn!("the audio server hung up");
                        return;
                    }
                },
                request = asked.recv() => match request {
                    Some(Request::Levels(on)) => {
                        listening = on;
                        if !on {
                            listeners.clear();
                            levels.clear();
                        }
                        stale = true;
                    }
                    Some(request) => {
                        apply(request).await;
                        stale = true;
                    }
                    None => return,
                },
                Some((id, level)) = heard.recv() => {
                    // The loudest moment since the levels were last sent.
                    let known = levels.entry(id).or_insert(0.0);
                    *known = known.max(level);
                }
                _ = pace.tick(), if listening && !levels.is_empty() => {
                    let now: Vec<(u32, f32)> = levels.drain().collect();
                    let _ = output.send(Update::Levels(now)).await;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUTS: &str = r#"[
      {"index": 178, "mute": false, "corked": false,
       "volume": {"mono": {"value": 65536, "value_percent": "100%"}},
       "properties": {"application.name": "speech-dispatcher-dummy",
                      "application.process.binary": "sd_dummy", "media.name": "playback"}},
      {"index": 201, "mute": false, "corked": true,
       "volume": {"front-left": {"value": 26214}, "front-right": {"value": 32768}},
       "properties": {"application.name": "Firefox",
                      "application.process.binary": "firefox", "media.name": "Guide du raid"}},
      {"index": 205, "mute": true, "corked": false,
       "volume": {"front-left": {"value": 98304}},
       "properties": {"application.name": "mpv Media Player", "media.name": "Playback"}}
    ]"#;

    const SINKS: &str = r#"[
      {"index": 54, "name": "usb-card", "description": "(null)", "mute": true,
       "volume": {"front-left": {"value": 27111}},
       "properties": {"node.nick": "Razer USB Sound Card"}},
      {"index": 253, "name": "hdmi", "description": "Écran Dell", "mute": false,
       "volume": {"front-left": {"value": 36045}, "front-right": {"value": 36045}}}
    ]"#;

    #[test]
    fn streams_are_read_with_their_application_and_volume() {
        let streams = parse_streams(INPUTS);

        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].id, 201);
        assert_eq!(streams[0].app, "firefox");
        assert_eq!(streams[0].media, "Guide du raid");
        assert!((streams[0].volume - 0.5).abs() < 0.001);
        assert!(streams[0].paused && !streams[0].muted);
    }

    #[test]
    fn a_stream_without_program_goes_by_the_name_of_its_application() {
        let streams = parse_streams(INPUTS);

        assert_eq!(streams[1].app, "mpv Media Player");
        assert_eq!(streams[1].media, "");
        assert!(streams[1].muted);
    }

    #[test]
    fn an_amplified_stream_shows_as_full() {
        let streams = parse_streams(INPUTS);

        assert!((streams[1].volume - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_speech_synthesizer_is_not_an_application_playing() {
        assert!(parse_streams(INPUTS).iter().all(|stream| stream.id != 178));
    }

    #[test]
    fn what_is_not_a_list_of_streams_gives_none() {
        assert!(parse_streams("").is_empty());
        assert!(parse_streams("{\"error\": true}").is_empty());
        assert!(parse_streams("[{\"mute\": false}]").is_empty());
    }

    #[test]
    fn the_default_output_is_found_by_its_name() {
        let (name, volume, muted) = parse_output(SINKS, "hdmi").unwrap();

        assert_eq!(name, "Écran Dell");
        assert!((volume - 0.55).abs() < 0.001);
        assert!(!muted);
    }

    #[test]
    fn an_output_without_description_goes_by_its_nickname() {
        let (name, _, muted) = parse_output(SINKS, "usb-card").unwrap();

        assert_eq!(name, "Razer USB Sound Card");
        assert!(muted);
        assert_eq!(parse_output(SINKS, "elsewhere"), None);
    }

    #[test]
    fn the_name_of_the_default_output_is_read() {
        let info =
            r#"{"server_name": "PulseAudio (on PipeWire 1.6.9)", "default_sink_name": "hdmi"}"#;

        assert_eq!(parse_default(info).as_deref(), Some("hdmi"));
        assert_eq!(parse_default("{}"), None);
    }

    #[test]
    fn the_peak_of_a_sound_is_its_loudest_sample() {
        let quiet = 100_i16.to_le_bytes();
        let loud = (-16384_i16).to_le_bytes();
        let samples = [quiet, loud, quiet].concat();

        assert!((peak(&samples) - 0.5).abs() < 0.001);
        assert!(peak(&[]).abs() < f32::EPSILON);
        assert!((peak(&i16::MIN.to_le_bytes()) - 1.0).abs() < 0.001);
    }

    #[test]
    fn volumes_are_given_to_pactl_as_percentages() {
        assert_eq!(percent(0.554), "55%");
        assert_eq!(percent(1.7), "100%");
        assert_eq!(percent(-1.0), "0%");
    }

    #[test]
    fn all_sinks_are_parsed_with_their_names_and_descriptions() {
        let sinks = parse_sinks(SINKS);
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0].name, "usb-card");
        assert_eq!(sinks[0].description, "Razer USB Sound Card");
        assert_eq!(sinks[1].name, "hdmi");
        assert_eq!(sinks[1].description, "Écran Dell");
    }
}
