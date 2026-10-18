//! The keyring of the coquille shell: the Secret Service of the session
//! (`org.freedesktop.secrets`), where applications keep their passwords and
//! keys, and the backend of the Secret portal for sandboxed ones.
//!
//! It is always open and never asks anything: its file is encrypted with a
//! key sealed by the TPM of this machine (see `vault`), unsealed when it
//! starts. The session starts it before the compositor, and it stops with it
//! (`--until`).

mod service;
mod store;
mod transport;
mod vault;

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use futures_util::StreamExt;
use tracing::{error, info, warn};
use zbus::fdo::{self, RequestNameFlags, RequestNameReply};
use zbus::names::BusName;

use crate::service::{BUS_NAME, Shared};
use crate::store::Keyring;
use crate::vault::{KeySource, Vault};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Where the keyring is kept (default: ~/.local/state/coquille).
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// A raw key instead of the sealed one: for trials only, made if missing.
    #[arg(long)]
    key_file: Option<PathBuf>,
    /// Stops when this process ends (the compositor of the session).
    #[arg(long)]
    until: Option<u32>,
}

fn default_state_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))?;
    Some(base.join("coquille"))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,coquille_keyring=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let directory = cli
        .state_dir
        .or_else(default_state_dir)
        .ok_or_else(|| anyhow::anyhow!("no directory for the keyring: HOME is not set"))?;
    let source = match cli.key_file {
        Some(file) => KeySource::File(file),
        None => KeySource::Sealed,
    };
    // What cannot be read is never replaced by an empty keyring.
    let opened = Vault::open(&directory, source).and_then(|vault| {
        let saved = vault.load()?;
        Ok((vault, saved))
    });
    let (vault, saved) = opened.inspect_err(|error| error!(%error, "the keyring cannot open"))?;
    let keyring = Keyring::new(saved, service::now());
    let shared = Shared::new(keyring, vault);

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(shared, cli.until))
}

async fn run(shared: Shared, until: Option<u32>) -> anyhow::Result<()> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    service::serve(&connection, &shared).await?;
    // Another keyring that lets itself be replaced is; one that does not
    // keeps the name until it leaves, and this one waits in line.
    let reply = connection
        .request_name_with_flags(BUS_NAME, RequestNameFlags::ReplaceExisting.into())
        .await?;
    match reply {
        RequestNameReply::InQueue => {
            warn!("another keyring holds {BUS_NAME}: waiting for it to leave");
        }
        _ => info!("serving {BUS_NAME}"),
    }

    let dbus = fdo::DBusProxy::new(&connection).await?;
    let mut changes = dbus.receive_name_owner_changed().await?;
    let mut acquired = dbus.receive_name_acquired().await?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        tokio::select! {
            Some(change) = changes.next() => {
                let Ok(args) = change.args() else { continue };
                // A client that leaves the bus takes its sessions with it.
                if let (BusName::Unique(name), None) = (args.name(), args.new_owner().as_ref()) {
                    shared.client_left(&connection, name).await;
                }
            }
            Some(signal) = acquired.next() => {
                if signal.args().is_ok_and(|args| args.name() == BUS_NAME) {
                    info!("serving {BUS_NAME}");
                }
            }
            _ = tick.tick(), if until.is_some() => {
                if let Some(pid) = until.filter(|pid| !PathBuf::from(format!("/proc/{pid}")).exists()) {
                    info!(pid, "the session ended: stopping");
                    break;
                }
            }
            _ = terminate.recv() => break,
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}
