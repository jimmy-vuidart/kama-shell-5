//! The agent at work: what polkit asks (`bus`), what the helper says
//! (`helper`) and what the user does in the dialog go through the
//! [`Agent`], and what it decides is carried out here. The dialog hears
//! what to show through [`Update`].

use std::collections::HashMap;

use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::agent::{self, Agent, Dialog, Effect, Outcome, Request, User};
use crate::bus::{self, FromBus, Subject};
use crate::helper::{Conversation, Password, Said};

/// What the dialog hears.
#[derive(Debug, Clone)]
pub enum Update {
    /// What the user does goes through this.
    Ready(UnboundedSender<Action>),
    Show(Dialog),
    Hide,
    /// The agent cannot work (no polkit, another agent in the session), or
    /// the trial request ended: the program stops.
    Stopped,
}

/// What the user does in the dialog.
#[derive(Debug, Clone)]
pub enum Action {
    Answer(Password),
    Choose(usize),
    Dismiss,
}

/// How the agent works.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Mode {
    /// For polkit, on the system bus.
    Real(Subject),
    /// One made-up request, without polkit: for trials and screenshots.
    Demo,
}

pub fn subscription(mode: Mode) -> Subscription<Update> {
    Subscription::run_with(mode, |mode| run(mode.clone()))
}

fn run(mode: Mode) -> impl Stream<Item = Update> {
    iced::stream::channel(32, async move |mut output| {
        let (to_agent, mut from_bus) = unbounded_channel();
        let (to_service, mut said) = unbounded_channel();
        let (actions, mut done) = unbounded_channel();
        let _ = output.send(Update::Ready(actions)).await;

        let demo = mode == Mode::Demo;
        if demo {
            let users = agent::ordered(demo_users().await, bus::my_uid());
            let (reply, _) = oneshot::channel();
            let request = demo_request(users);
            let _ = to_agent.send(FromBus::Begin { request, reply });
        }
        let mut bus = tokio::spawn(async move {
            match mode {
                Mode::Real(subject) => bus::serve(to_agent, &subject).await,
                Mode::Demo => std::future::pending().await,
            }
        });
        let mut on_bus = true;

        let mut agent = Agent::new();
        let mut replies: HashMap<String, oneshot::Sender<Outcome>> = HashMap::new();
        let mut conversation: Option<Conversation> = None;
        let mut typed: Option<Password> = None;
        let mut shown: Option<Dialog> = None;
        loop {
            let effects = tokio::select! {
                Some(message) = from_bus.recv() => match message {
                    FromBus::Begin { request, reply } => {
                        replies.insert(request.cookie.clone(), reply);
                        agent.begin(request)
                    }
                    FromBus::Cancel(cookie) => agent.cancel(&cookie),
                },
                Some((attempt, what)) = said.recv() => match what {
                    Said::Line(line) => agent.helper(attempt, line),
                    Said::Gone => agent.helper_gone(attempt),
                },
                Some(action) = done.recv() => match action {
                    Action::Answer(password) => {
                        typed = Some(password);
                        agent.answer()
                    }
                    Action::Choose(user) => agent.choose(user),
                    Action::Dismiss => agent.dismiss(),
                },
                ended = &mut bus, if on_bus => {
                    match ended {
                        Ok(Ok(())) => error!("the system bus hung up"),
                        Ok(Err(error)) => error!(%error, "the agent cannot work for polkit"),
                        Err(error) => error!(%error, "the agent stopped"),
                    }
                    on_bus = false;
                    let _ = output.send(Update::Stopped).await;
                    continue;
                }
            };

            let mut finished = false;
            for effect in effects {
                match effect {
                    Effect::Start {
                        attempt,
                        user,
                        cookie,
                    } => {
                        let said = to_service.clone();
                        conversation = Some(Conversation::start(attempt, user, cookie, said, demo));
                    }
                    Effect::Stop => {
                        conversation = None;
                        typed = None;
                    }
                    Effect::Send { attempt } => {
                        let current = conversation.as_ref().filter(|c| c.attempt() == attempt);
                        if let (Some(conversation), Some(password)) = (current, typed.take()) {
                            conversation.send(password);
                        }
                    }
                    Effect::Finish { cookie, outcome } => {
                        info!(?outcome, "request answered");
                        if let Some(reply) = replies.remove(&cookie) {
                            let _ = reply.send(outcome);
                        }
                        finished = true;
                    }
                }
            }

            let now = agent.dialog();
            if now.is_none() {
                // Nothing is asked any more: what was typed goes.
                typed = None;
            }
            if now != shown {
                let update = match &now {
                    Some(dialog) => Update::Show(dialog.clone()),
                    None => Update::Hide,
                };
                let _ = output.send(update).await;
                shown = now;
            }
            if demo && finished {
                let _ = output.send(Update::Stopped).await;
            }
        }
    })
}

/// The accounts of the trial request: the user, and root.
async fn demo_users() -> Vec<User> {
    bus::users_of(&[bus::my_uid(), 0]).await
}

fn demo_request(users: Vec<User>) -> Request {
    Request {
        cookie: "demo".to_owned(),
        action_id: "org.freedesktop.policykit.exec".to_owned(),
        message: "Une authentification est nécessaire pour lancer « /usr/bin/true » en tant \
                  que superutilisateur"
            .to_owned(),
        users,
    }
}
