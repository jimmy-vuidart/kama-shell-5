//! polkit on the system bus: the agent serves
//! `org.freedesktop.PolicyKit1.AuthenticationAgent` and registers with the
//! authority of polkit for the login session (or, for trials, for one
//! process). polkit then asks it to authenticate whenever an application
//! of that session needs it, and waits for the answer to
//! `BeginAuthentication`.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;

use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use zbus::message::Header;
use zbus::names::BusName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, fdo, interface};

use crate::agent::{self, Outcome, Request, User};

/// Where the agent is on the bus.
const AGENT_PATH: &str = "/org/coquille/PolkitAgent";
const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    Cancelled(String),
    NotAuthorized(String),
}

/// What polkit asks of the agent.
#[derive(Debug)]
pub enum FromBus {
    /// Someone has to authenticate; `reply` takes how it ended.
    Begin {
        request: Request,
        reply: oneshot::Sender<Outcome>,
    },
    /// polkit no longer needs this cookie.
    Cancel(String),
}

/// Who the agent works for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// The login session (`XDG_SESSION_ID`, else the one logind knows this
    /// process by).
    Session,
    /// Only the requests of one process, as `pkttyagent --process` does.
    Process(u32),
}

/// The agent on the bus: answers polkit for as long as the connection
/// lives, and registers again when polkit starts again.
pub async fn serve(to_agent: UnboundedSender<FromBus>, subject: &Subject) -> zbus::Result<()> {
    let connection = zbus::connection::Builder::system()?
        .serve_at(AGENT_PATH, AgentObject { to_agent })?
        .build()
        .await?;
    let subject = subject_of(&connection, subject).await?;
    register(&connection, &subject).await?;

    let dbus = fdo::DBusProxy::new(&connection).await?;
    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, AUTHORITY)])
        .await?;
    while let Some(change) = changes.next().await {
        let Ok(args) = change.args() else { continue };
        if args.new_owner().is_some() {
            info!("polkit started again");
            if let Err(error) = register(&connection, &subject).await {
                warn!(%error, "cannot register with polkit again");
            }
        }
    }
    Ok(())
}

/// A subject of polkit, as it goes on the bus.
#[derive(Debug, Clone)]
struct Wire {
    kind: &'static str,
    details: HashMap<&'static str, OwnedValue>,
}

impl Wire {
    fn as_value(&self) -> (&str, HashMap<&str, &Value<'static>>) {
        let details = self.details.iter().map(|(key, value)| (*key, &**value));
        (self.kind, details.collect())
    }
}

async fn subject_of(connection: &Connection, subject: &Subject) -> zbus::Result<Wire> {
    match subject {
        Subject::Session => {
            let id = session_id(connection).await?;
            info!(session = id, "agent of the login session");
            Ok(Wire {
                kind: "unix-session",
                details: HashMap::from([(
                    "session-id",
                    OwnedValue::from(zbus::zvariant::Str::from(id)),
                )]),
            })
        }
        Subject::Process(pid) => {
            let fail =
                |error: std::io::Error| zbus::Error::Failure(format!("process {pid}: {error}"));
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(fail)?;
            let start = start_time(&stat)
                .ok_or_else(|| zbus::Error::Failure(format!("process {pid}: no start time")))?;
            let uid = std::fs::metadata(format!("/proc/{pid}"))
                .map_err(fail)?
                .uid();
            info!(pid, "agent of one process");
            Ok(Wire {
                kind: "unix-process",
                details: HashMap::from([
                    ("pid", OwnedValue::from(*pid)),
                    ("start-time", OwnedValue::from(start)),
                    ("uid", OwnedValue::from(uid as i32)),
                ]),
            })
        }
    }
}

async fn session_id(connection: &Connection) -> zbus::Result<String> {
    if let Some(id) = std::env::var("XDG_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
    {
        return Ok(id);
    }
    let reply = connection
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetSessionByPID",
            &(std::process::id()),
        )
        .await?;
    let path: OwnedObjectPath = reply.body().deserialize()?;
    let session = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        path,
        "org.freedesktop.login1.Session",
    )
    .await?;
    session.get_property::<String>("Id").await
}

/// When a process started, in ticks since boot: the 22nd field of
/// `/proc/<pid>/stat`, counted after the name, which may hold anything.
fn start_time(stat: &str) -> Option<u64> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    after_name.split_whitespace().nth(19)?.parse().ok()
}

async fn register(connection: &Connection, subject: &Wire) -> zbus::Result<()> {
    connection
        .call_method(
            Some(AUTHORITY),
            AUTHORITY_PATH,
            Some(AUTHORITY_INTERFACE),
            "RegisterAuthenticationAgent",
            &(subject.as_value(), locale(), AGENT_PATH),
        )
        .await?;
    info!("registered with polkit");
    Ok(())
}

/// The language polkit writes its messages in: the one of the session.
fn locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "C".to_owned())
}

struct AgentObject {
    to_agent: UnboundedSender<FromBus>,
}

#[interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl AgentObject {
    /// Answers once the user authenticated, gave up, or polkit cancelled.
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        action_id: String,
        message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> Result<(), AgentError> {
        // Anyone else could make a dialog show and ask for a password.
        if !from_polkit(connection, &header).await {
            warn!("a request that does not come from polkit was refused");
            return Err(AgentError::NotAuthorized("only polkit may ask".to_owned()));
        }
        let uids: Vec<u32> = identities.iter().filter_map(uid_of).collect();
        let users = agent::ordered(users_of(&uids).await, my_uid());
        info!(
            action_id,
            accounts = users.len(),
            "polkit asks for a password"
        );
        let request = Request {
            cookie,
            action_id,
            message,
            users,
        };
        let (reply, outcome) = oneshot::channel();
        let sent = self.to_agent.send(FromBus::Begin { request, reply });
        if sent.is_err() {
            return Err(AgentError::Failed("the agent stopped".to_owned()));
        }
        match outcome.await {
            Ok(Outcome::Authenticated) => Ok(()),
            Ok(Outcome::Cancelled) => Err(AgentError::Cancelled("dismissed".to_owned())),
            Ok(Outcome::Impossible) => {
                Err(AgentError::Failed("no account can authenticate".to_owned()))
            }
            Err(_) => Err(AgentError::Failed("the agent stopped".to_owned())),
        }
    }

    async fn cancel_authentication(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        cookie: String,
    ) -> Result<(), AgentError> {
        if !from_polkit(connection, &header).await {
            return Err(AgentError::NotAuthorized("only polkit may ask".to_owned()));
        }
        debug!("polkit cancels a request");
        let _ = self.to_agent.send(FromBus::Cancel(cookie));
        Ok(())
    }
}

/// The message comes from the process that owns the name of polkit.
async fn from_polkit(connection: &Connection, header: &Header<'_>) -> bool {
    let Some(sender) = header.sender() else {
        return false;
    };
    let Ok(dbus) = fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(name) = BusName::try_from(AUTHORITY) else {
        return false;
    };
    let owner = dbus.get_name_owner(name).await;
    owner.is_ok_and(|owner| owner.as_str() == sender.as_str())
}

/// The account of an identity of polkit: groups were already turned into
/// their members by polkit.
fn uid_of((kind, details): &(String, HashMap<String, OwnedValue>)) -> Option<u32> {
    if kind != "unix-user" {
        debug!(kind, "identity ignored");
        return None;
    }
    let uid = details.get("uid")?;
    u32::try_from(uid).ok().or_else(|| {
        i32::try_from(uid)
            .ok()
            .and_then(|uid| u32::try_from(uid).ok())
    })
}

/// The user this agent runs as.
pub fn my_uid() -> u32 {
    std::fs::metadata("/proc/self").map_or(u32::MAX, |meta| meta.uid())
}

/// The accounts behind `uids`, as the system knows them (`getent`, which
/// reads every source of accounts, not only `/etc/passwd`).
pub async fn users_of(uids: &[u32]) -> Vec<User> {
    if uids.is_empty() {
        return Vec::new();
    }
    let output = tokio::process::Command::new("getent")
        .arg("passwd")
        .args(uids.iter().map(u32::to_string))
        .output()
        .await;
    match output {
        // `getent` fails when one of them is unknown, and still lists the
        // others.
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(account)
            .collect(),
        Err(error) => {
            warn!(%error, "cannot read the accounts");
            Vec::new()
        }
    }
}

/// One line of `passwd`: `login:x:uid:gid:full name,room,…:home:shell`.
fn account(line: &str) -> Option<User> {
    let mut fields = line.split(':');
    let login = fields.next().filter(|login| !login.is_empty())?;
    let uid = fields.nth(1)?.parse().ok()?;
    let gecos = fields.nth(1)?;
    let name = gecos.split(',').next().filter(|name| !name.is_empty());
    Some(User {
        uid,
        login: login.to_owned(),
        name: name.map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_has_its_login_uid_and_full_name() {
        let user = account("jimmy:x:1000:1000:Jimmy Vuidart,,,:/home/jimmy:/bin/fish");

        assert_eq!(
            user,
            Some(User {
                uid: 1000,
                login: "jimmy".to_owned(),
                name: Some("Jimmy Vuidart".to_owned()),
            })
        );
    }

    #[test]
    fn an_account_without_a_full_name_has_none() {
        let user = account("root:x:0:0::/root:/bin/bash").unwrap();

        assert_eq!(user.name, None);
        assert_eq!(account("broken"), None);
    }

    #[test]
    fn the_start_time_is_counted_after_the_name() {
        // A name with spaces and a parenthesis, as `comm` may have.
        let stat =
            "4242 (a b) c) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 1000 200";

        assert_eq!(start_time(stat), Some(987_654));
        assert_eq!(start_time("garbage"), None);
    }

    #[test]
    fn only_users_are_identities_of_the_agent() {
        let user = (
            "unix-user".to_owned(),
            HashMap::from([("uid".to_owned(), OwnedValue::from(1000_u32))]),
        );
        let group = (
            "unix-group".to_owned(),
            HashMap::from([("gid".to_owned(), OwnedValue::from(998_u32))]),
        );

        assert_eq!(uid_of(&user), Some(1000));
        assert_eq!(uid_of(&group), None);
    }
}
