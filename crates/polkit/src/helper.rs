//! The helper of polkit, `polkit-agent-helper-1`: the program that runs
//! PAM as root and tells polkit the user authenticated. The agent never
//! checks a password itself.
//!
//! With polkit 126 and later, systemd starts the helper for each connection
//! to its socket (`polkit-agent-helper.socket`): the agent writes the login
//! of the account, then the cookie of the request, one line each. Without
//! the socket, the helper is a setuid program that takes the login as its
//! argument and the cookie on its input. Either way, the conversation of
//! PAM follows, one line at a time (`PAM_PROMPT_ECHO_OFF Password: `,
//! `PAM_ERROR_MSG …`), escaped by `g_strescape`, until `SUCCESS` or
//! `FAILURE`; the agent answers a prompt with one line.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use zeroize::Zeroizing;

use crate::agent::Line;

/// Where systemd listens for conversations with the helper.
const SOCKET: &str = "/run/polkit/agent-helper.socket";
/// The setuid helper, when there is no socket.
const SETUID: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";
/// What the trial conversation (`--demo`) takes for the right password.
pub const DEMO_PASSWORD: &str = "coquille";

/// What the user typed: wiped from memory when dropped, never shown in a
/// log.
#[derive(Clone, Default)]
pub struct Password(Zeroizing<String>);

impl Password {
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Password(…)")
    }
}

/// What a conversation tells the agent, with its number.
#[derive(Debug)]
pub enum Said {
    Line(Line),
    /// It ended without `SUCCESS` nor `FAILURE`.
    Gone,
}

/// A conversation going on: dropping it ends it.
#[derive(Debug)]
pub struct Conversation {
    attempt: u64,
    answers: UnboundedSender<Password>,
    task: JoinHandle<()>,
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Conversation {
    /// Starts a conversation as `user` for the request `cookie`; what it
    /// says goes to `said`. A trial one (`demo`) needs no polkit.
    pub fn start(
        attempt: u64,
        user: String,
        cookie: String,
        said: UnboundedSender<(u64, Said)>,
        demo: bool,
    ) -> Self {
        let (answers, asked) = unbounded_channel();
        let task = tokio::spawn(async move {
            let ended = if demo {
                pretend(attempt, &said, asked).await
            } else {
                converse(&user, &cookie, attempt, &said, asked).await
            };
            match ended {
                Ok(true) => {}
                Ok(false) => {
                    debug!(attempt, "the helper of polkit hung up");
                    let _ = said.send((attempt, Said::Gone));
                }
                Err(error) => {
                    warn!(attempt, %error, "cannot talk with the helper of polkit");
                    let _ = said.send((attempt, Said::Gone));
                }
            }
        });
        Self {
            attempt,
            answers,
            task,
        }
    }

    pub fn attempt(&self) -> u64 {
        self.attempt
    }

    /// The answer to the question of the helper.
    pub fn send(&self, password: Password) {
        let _ = self.answers.send(password);
    }
}

/// A real conversation. `true` once it said `SUCCESS` or `FAILURE`.
async fn converse(
    user: &str,
    cookie: &str,
    attempt: u64,
    said: &UnboundedSender<(u64, Said)>,
    answers: UnboundedReceiver<Password>,
) -> io::Result<bool> {
    if Path::new(SOCKET).exists() {
        let stream = UnixStream::connect(SOCKET).await?;
        debug!(
            attempt,
            "talking with the helper of polkit through its socket"
        );
        let (reader, mut writer) = stream.into_split();
        writer.write_all(format!("{user}\n").as_bytes()).await?;
        talk(cookie, attempt, said, answers, reader, writer).await
    } else {
        let mut child = Command::new(SETUID)
            .arg(user)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        debug!(attempt, "talking with the setuid helper of polkit");
        let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(io::Error::other("the helper has no input or output"));
        };
        let ended = talk(cookie, attempt, said, answers, reader, writer).await;
        let _ = child.wait().await;
        ended
    }
}

async fn talk(
    cookie: &str,
    attempt: u64,
    said: &UnboundedSender<(u64, Said)>,
    mut answers: UnboundedReceiver<Password>,
    reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
) -> io::Result<bool> {
    // The cookie on the input, never as an argument other processes see.
    writer.write_all(format!("{cookie}\n").as_bytes()).await?;
    let mut lines = BufReader::new(reader).lines();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else {
                    return Ok(false);
                };
                let Some(line) = parse(&line) else {
                    warn!(attempt, "the helper of polkit said something unknown");
                    continue;
                };
                let last = matches!(line, Line::Success | Line::Failure);
                let _ = said.send((attempt, Said::Line(line)));
                if last {
                    return Ok(true);
                }
            }
            Some(password) = answers.recv() => {
                let mut answer = Zeroizing::new(Vec::with_capacity(password.as_str().len() + 1));
                answer.extend_from_slice(password.as_str().as_bytes());
                answer.push(b'\n');
                writer.write_all(&answer).await?;
            }
        }
    }
}

/// The trial conversation: asks once, takes `DEMO_PASSWORD`.
async fn pretend(
    attempt: u64,
    said: &UnboundedSender<(u64, Said)>,
    mut answers: UnboundedReceiver<Password>,
) -> io::Result<bool> {
    tokio::time::sleep(Duration::from_millis(150)).await;
    let prompt = Line::Prompt {
        text: "Password: ".to_owned(),
        echo: false,
    };
    let _ = said.send((attempt, Said::Line(prompt)));
    let Some(password) = answers.recv().await else {
        return Ok(false);
    };
    tokio::time::sleep(Duration::from_millis(600)).await;
    let line = if password.as_str() == DEMO_PASSWORD {
        Line::Success
    } else {
        Line::Failure
    };
    let _ = said.send((attempt, Said::Line(line)));
    Ok(true)
}

/// One line of the helper, unescaped.
pub fn parse(line: &str) -> Option<Line> {
    let line = compress(line);
    let after = |prefix: &str| line.strip_prefix(prefix).map(str::to_owned);
    if let Some(text) = after("PAM_PROMPT_ECHO_OFF ") {
        return Some(Line::Prompt { text, echo: false });
    }
    if let Some(text) = after("PAM_PROMPT_ECHO_ON ") {
        return Some(Line::Prompt { text, echo: true });
    }
    if let Some(text) = after("PAM_ERROR_MSG ") {
        return Some(Line::Error(text));
    }
    if let Some(text) = after("PAM_TEXT_INFO ") {
        return Some(Line::Info(text));
    }
    if line.starts_with("SUCCESS") {
        return Some(Line::Success);
    }
    if line.starts_with("FAILURE") {
        return Some(Line::Failure);
    }
    None
}

/// Undoes `g_strescape`, as `g_strcompress` does: C escapes, and octal ones
/// for every byte outside ASCII (an accented letter is two of them).
fn compress(line: &str) -> String {
    let mut bytes = Vec::with_capacity(line.len());
    let mut rest = line.as_bytes().iter().copied().peekable();
    while let Some(byte) = rest.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let Some(escaped) = rest.next() else {
            break;
        };
        let byte = match escaped {
            b'0'..=b'7' => {
                let mut value = u32::from(escaped - b'0');
                for _ in 0..2 {
                    match rest.peek() {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            rest.next();
                        }
                        _ => break,
                    }
                }
                (value & 0xff) as u8
            }
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 0x0b,
            other => other,
        };
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_prompt_hides_what_is_typed() {
        assert_eq!(
            parse("PAM_PROMPT_ECHO_OFF Password: "),
            Some(Line::Prompt {
                text: "Password: ".to_owned(),
                echo: false
            })
        );
        assert_eq!(
            parse("PAM_PROMPT_ECHO_ON Login: "),
            Some(Line::Prompt {
                text: "Login: ".to_owned(),
                echo: true
            })
        );
    }

    #[test]
    fn messages_errors_and_the_end_are_understood() {
        assert_eq!(
            parse("PAM_ERROR_MSG Account locked"),
            Some(Line::Error("Account locked".to_owned()))
        );
        assert_eq!(
            parse("PAM_TEXT_INFO Swipe your finger"),
            Some(Line::Info("Swipe your finger".to_owned()))
        );
        assert_eq!(parse("SUCCESS"), Some(Line::Success));
        assert_eq!(parse("FAILURE"), Some(Line::Failure));
        assert_eq!(parse("WHAT"), None);
    }

    #[test]
    fn accented_letters_come_as_octal_escapes() {
        // `g_strescape("Échec \"total\"\n")`.
        let escaped = r#"PAM_ERROR_MSG \303\211chec \"total\"\n\\"#;

        assert_eq!(
            parse(escaped),
            Some(Line::Error("Échec \"total\"\n\\".to_owned()))
        );
    }

    #[test]
    fn a_short_octal_escape_ends_at_the_first_other_character() {
        assert_eq!(compress(r"a\0b\7"), "a\0b\x07");
        assert_eq!(compress(r"tab\there"), "tab\there");
    }

    #[test]
    fn a_password_never_shows_in_a_log() {
        let password = Password::new("secret".to_owned());

        assert_eq!(format!("{password:?}"), "Password(…)");
    }
}
