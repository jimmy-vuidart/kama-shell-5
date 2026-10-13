//! The link to the coquille compositor: its state comes in, commands go
//! out. When the compositor is not there (yet), the program keeps trying.

use std::path::PathBuf;
use std::time::Duration;

use coquille_ipc::{Command, Event, decode, encode};
use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, warn};

/// What happens on the link.
#[derive(Debug, Clone)]
pub enum Update {
    /// Connected: commands go through this.
    Connected(UnboundedSender<Command>),
    Event(Event),
    /// The compositor went away.
    Disconnected,
}

/// Listens to the compositor behind `socket` for as long as the program runs.
pub fn subscription(socket: PathBuf) -> Subscription<Update> {
    Subscription::run_with(socket, |socket| link(socket.clone()))
}

fn link(socket: PathBuf) -> impl Stream<Item = Update> {
    iced::stream::channel(64, async move |mut output| {
        loop {
            match UnixStream::connect(&socket).await {
                Ok(stream) => {
                    debug!(socket = %socket.display(), "connected to the compositor");
                    let (reader, mut writer) = stream.into_split();
                    let (sender, mut commands) = unbounded_channel::<Command>();
                    let _ = output.send(Update::Connected(sender)).await;
                    let mut lines = BufReader::new(reader).lines();
                    loop {
                        tokio::select! {
                            line = lines.next_line() => {
                                let Ok(Some(line)) = line else {
                                    break;
                                };
                                match decode::<Event>(&line) {
                                    Ok(event) => {
                                        let _ = output.send(Update::Event(event)).await;
                                    }
                                    // A newer compositor may say things this
                                    // program does not know yet.
                                    Err(error) => debug!(%error, "event not understood"),
                                }
                            }
                            command = commands.recv() => {
                                let Some(command) = command else {
                                    break;
                                };
                                let line = encode(&command);
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    warn!("the compositor hung up");
                    let _ = output.send(Update::Disconnected).await;
                }
                Err(error) => debug!(%error, "the compositor does not answer yet"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}
