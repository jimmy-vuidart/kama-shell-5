//! What is installed while the dock runs: the desktop files and the Steam
//! library are summed up every few seconds, and the dock is told when that
//! changed and holds still, so that the drawer lists an application as soon as it is
//! installed, without logging out.

use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};

use super::Update;
use crate::steam;

/// How often the folders are looked at: a few dozen `stat`, off the
/// interface.
const EVERY: Duration = Duration::from_secs(3);

/// The desktop files and the Steam library, summed up. Reads many files:
/// not on the thread of the interface.
pub fn fingerprint() -> u64 {
    coquille_apps::fingerprint() ^ steam::fingerprint().rotate_left(1)
}

pub fn subscription() -> Subscription<Update> {
    Subscription::run(service)
}

fn service() -> impl Stream<Item = Update> {
    iced::stream::channel(1, async move |mut output: Sender<Update>| {
        let (mut told, mut last) = (None, None);
        let mut every = tokio::time::interval(EVERY);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            let Ok(now) = tokio::task::spawn_blocking(fingerprint).await else {
                continue;
            };
            // Told once it holds still: a package manager writes the desktop
            // file before the icons, and an icon looked for too early would
            // stay missing. The first one is told too: something may have
            // been installed while the dock was starting.
            let settled = last.replace(now) == Some(now);
            if settled && told != Some(now) {
                told = Some(now);
                let _ = output.send(Update::Installed(now)).await;
            }
        }
    })
}
