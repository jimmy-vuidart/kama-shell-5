//! The battery of a laptop, through `upower`: found once, read again on
//! each change `upower --monitor` reports, and every minute should it say
//! nothing. A desktop has none, and the dock shows none.

use std::process::Stdio;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info};

use super::Update;
use crate::model::battery::{self, Battery};

/// The output of `upower` with these arguments, when it ran.
async fn upower(arguments: &[&str]) -> Option<String> {
    let output = Command::new("upower")
        .args(arguments)
        .env("LC_ALL", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn read(device: &str) -> Option<Battery> {
    battery::parse(&upower(&["-i", device]).await?)
}

pub fn subscription() -> Subscription<Update> {
    Subscription::run(service)
}

fn service() -> impl Stream<Item = Update> {
    iced::stream::channel(4, async move |mut output: Sender<Update>| {
        let devices = upower(&["-e"]).await.unwrap_or_default();
        let Some(device) = battery::pick(&devices).map(str::to_owned) else {
            debug!("no battery (or no upower): none is shown");
            return;
        };
        info!(device, "battery found");
        let mut known = read(&device).await;
        let _ = output.send(Update::Battery(known)).await;

        // Each change of any device prints lines: the battery is read again
        // when they stop coming for a moment.
        let monitor = Command::new("upower")
            .arg("--monitor")
            .env("LC_ALL", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let mut lines = monitor.ok().and_then(|mut child| {
            let stdout = child.stdout.take()?;
            // Kept with the reader: dropping it would end the monitor.
            Some((child, BufReader::new(stdout).lines()))
        });
        let mut minute = tokio::time::interval(Duration::from_secs(60));
        loop {
            let changed = async {
                match lines.as_mut() {
                    Some((_, reader)) => reader.next_line().await.ok().flatten().is_some(),
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                more = changed => {
                    if !more {
                        lines = None;
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                _ = minute.tick() => {}
            }
            let now = read(&device).await;
            if now != known {
                known = now;
                let _ = output.send(Update::Battery(now)).await;
            }
        }
    })
}
