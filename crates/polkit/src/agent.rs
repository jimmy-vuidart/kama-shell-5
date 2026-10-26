//! What the agent does with the requests of polkit, without any I/O: one
//! request is shown at a time, the others wait their turn; each attempt is
//! a conversation with the helper of polkit (see `helper`), until the user
//! succeeds or gives up.
//!
//! A conversation starts only once the user validates a password, and ends
//! with its answer: one cut short counts as a failed login for
//! `pam_faillock`, which locks the account after three. Opening the dialog,
//! choosing another account or giving up therefore costs nothing.
//!
//! The password itself never comes here: the agent only says when the one
//! the user typed may go to the helper (`Effect::Send`).

use std::collections::{HashSet, VecDeque};

/// Someone who can authenticate: an administrator, or the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub uid: u32,
    pub login: String,
    /// The full name, when the account has one.
    pub name: Option<String>,
}

impl User {
    /// How the dialog names this account.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) if name != &self.login => format!("{name} ({})", self.login),
            _ => self.login.clone(),
        }
    }
}

/// The accounts polkit offers, the one of the user first: it is the one
/// whose password they know, most of the time.
pub fn ordered(users: Vec<User>, me: u32) -> Vec<User> {
    let mut seen = HashSet::new();
    let mut users: Vec<User> = users
        .into_iter()
        .filter(|user| seen.insert(user.uid))
        .collect();
    users.sort_by_key(|user| user.uid != me);
    users
}

/// What polkit asks for: an action that needs someone to authenticate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// What polkit knows the request by.
    pub cookie: String,
    pub action_id: String,
    /// What the action does, in the language of the session.
    pub message: String,
    pub users: Vec<User>,
}

/// What the helper says, one line at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// PAM asks for something: a password (`echo` false), or something
    /// that may show as it is typed.
    Prompt {
        text: String,
        echo: bool,
    },
    Error(String),
    Info(String),
    Success,
    Failure,
}

/// How a request ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The helper told polkit the user is who they say.
    Authenticated,
    /// The user gave up, or polkit no longer needs it.
    Cancelled,
    /// No account polkit offered can authenticate here.
    Impossible,
}

/// What the agent asks the outside world to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Starts a conversation with the helper, as `user`: `attempt` numbers
    /// it, and what it says comes back with that number.
    Start {
        attempt: u64,
        user: String,
        cookie: String,
    },
    /// Ends the conversation going on, if any.
    Stop,
    /// The password typed goes to the conversation `attempt`.
    Send { attempt: u64 },
    /// polkit gets its answer for `cookie`.
    Finish { cookie: String, outcome: Outcome },
}

/// Where the request shown stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the user to validate a password; no conversation.
    Ready,
    /// A conversation starts, with the password validated waiting for its
    /// question.
    Starting,
    /// The helper asks something more (PAM wants a new password, a code).
    Asking { prompt: String, echo: bool },
    /// The answer went to the helper, which checks it.
    Checking,
}

/// A message under the password field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Info(String),
    Error(String),
}

/// What the dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub action_id: String,
    pub message: String,
    pub users: Vec<String>,
    /// The account chosen, among `users`.
    pub user: usize,
    pub phase: Phase,
    pub note: Option<Note>,
    /// Other requests waiting their turn.
    pub waiting: usize,
}

impl Dialog {
    /// A conversation goes on: nothing can be typed or validated.
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Checking)
    }
}

/// Said when a wrong password ends a conversation without PAM saying why.
pub const WRONG_PASSWORD: &str = "Mot de passe incorrect. Réessayez.";
/// Said when the helper stops before asking anything, without a reason.
pub const NOT_STARTED: &str = "L'authentification n'a pas pu commencer.";

#[derive(Debug)]
struct Shown {
    request: Request,
    user: usize,
    attempt: u64,
    phase: Phase,
    note: Option<Note>,
    /// What PAM said was wrong during this conversation.
    error: Option<String>,
}

/// The requests of polkit, one shown, the others waiting.
#[derive(Debug, Default)]
pub struct Agent {
    shown: Option<Shown>,
    waiting: VecDeque<Request>,
    attempts: u64,
}

impl Agent {
    pub fn new() -> Self {
        Self::default()
    }

    /// polkit asks for someone to authenticate.
    pub fn begin(&mut self, request: Request) -> Vec<Effect> {
        if self.shown.is_some() {
            self.waiting.push_back(request);
            return Vec::new();
        }
        self.show(request)
    }

    /// polkit no longer needs `cookie` (the application gave up).
    pub fn cancel(&mut self, cookie: &str) -> Vec<Effect> {
        if let Some(index) = self.waiting.iter().position(|r| r.cookie == cookie) {
            self.waiting.remove(index);
            return vec![finish(cookie.to_owned(), Outcome::Cancelled)];
        }
        if self.shown_cookie() == Some(cookie) {
            return self.end(Outcome::Cancelled);
        }
        Vec::new()
    }

    /// The user gives up the request shown.
    pub fn dismiss(&mut self) -> Vec<Effect> {
        if self.shown.is_none() {
            return Vec::new();
        }
        self.end(Outcome::Cancelled)
    }

    /// The user picks another account.
    pub fn choose(&mut self, user: usize) -> Vec<Effect> {
        let Some(shown) = &mut self.shown else {
            return Vec::new();
        };
        if user >= shown.request.users.len() || user == shown.user {
            return Vec::new();
        }
        shown.user = user;
        shown.note = None;
        let talking = shown.phase != Phase::Ready;
        shown.phase = Phase::Ready;
        if talking {
            vec![Effect::Stop]
        } else {
            Vec::new()
        }
    }

    /// The user validates what they typed.
    pub fn answer(&mut self) -> Vec<Effect> {
        let Some(shown) = &mut self.shown else {
            return Vec::new();
        };
        match shown.phase {
            Phase::Ready => {
                self.attempts += 1;
                shown.attempt = self.attempts;
                shown.phase = Phase::Starting;
                shown.note = None;
                shown.error = None;
                vec![Effect::Start {
                    attempt: shown.attempt,
                    user: shown.request.users[shown.user].login.clone(),
                    cookie: shown.request.cookie.clone(),
                }]
            }
            Phase::Asking { .. } => {
                shown.phase = Phase::Checking;
                shown.note = None;
                vec![Effect::Send {
                    attempt: shown.attempt,
                }]
            }
            Phase::Starting | Phase::Checking => Vec::new(),
        }
    }

    /// The conversation `attempt` says something. What an earlier one says
    /// is too late.
    pub fn helper(&mut self, attempt: u64, line: Line) -> Vec<Effect> {
        let current = |shown: &&mut Shown| shown.attempt == attempt && shown.phase != Phase::Ready;
        let Some(shown) = self.shown.as_mut().filter(current) else {
            return Vec::new();
        };
        match line {
            // The password validated answers the first question.
            Line::Prompt { .. } if shown.phase == Phase::Starting => {
                shown.phase = Phase::Checking;
                vec![Effect::Send { attempt }]
            }
            Line::Prompt { text, echo } => {
                shown.phase = Phase::Asking { prompt: text, echo };
                Vec::new()
            }
            Line::Info(text) => {
                shown.note = Some(Note::Info(text));
                Vec::new()
            }
            Line::Error(text) => {
                shown.error = Some(text.clone());
                shown.note = Some(Note::Error(text));
                Vec::new()
            }
            Line::Success => self.end(Outcome::Authenticated),
            Line::Failure => {
                let answered = shown.phase == Phase::Checking;
                let usual = if answered {
                    WRONG_PASSWORD
                } else {
                    NOT_STARTED
                };
                let reason = shown.error.take().unwrap_or_else(|| usual.to_owned());
                shown.phase = Phase::Ready;
                shown.note = Some(Note::Error(reason));
                vec![Effect::Stop]
            }
        }
    }

    /// The conversation `attempt` ended without a word: it failed.
    pub fn helper_gone(&mut self, attempt: u64) -> Vec<Effect> {
        self.helper(attempt, Line::Failure)
    }

    /// What the dialog shows, while a request is there.
    pub fn dialog(&self) -> Option<Dialog> {
        let shown = self.shown.as_ref()?;
        Some(Dialog {
            action_id: shown.request.action_id.clone(),
            message: shown.request.message.clone(),
            users: shown.request.users.iter().map(User::label).collect(),
            user: shown.user,
            phase: shown.phase.clone(),
            note: shown.note.clone(),
            waiting: self.waiting.len(),
        })
    }

    fn shown_cookie(&self) -> Option<&str> {
        self.shown
            .as_ref()
            .map(|shown| shown.request.cookie.as_str())
    }

    fn show(&mut self, request: Request) -> Vec<Effect> {
        if request.users.is_empty() {
            let mut effects = vec![finish(request.cookie, Outcome::Impossible)];
            effects.extend(self.next());
            return effects;
        }
        self.shown = Some(Shown {
            request,
            user: 0,
            attempt: 0,
            phase: Phase::Ready,
            note: None,
            error: None,
        });
        Vec::new()
    }

    /// The request shown ends; the next one shows.
    fn end(&mut self, outcome: Outcome) -> Vec<Effect> {
        let Some(shown) = self.shown.take() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        if shown.phase != Phase::Ready {
            effects.push(Effect::Stop);
        }
        effects.push(finish(shown.request.cookie, outcome));
        effects.extend(self.next());
        effects
    }

    fn next(&mut self) -> Vec<Effect> {
        match self.waiting.pop_front() {
            Some(request) => self.show(request),
            None => Vec::new(),
        }
    }
}

fn finish(cookie: String, outcome: Outcome) -> Effect {
    Effect::Finish { cookie, outcome }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(uid: u32, login: &str) -> User {
        User {
            uid,
            login: login.to_owned(),
            name: None,
        }
    }

    fn request(cookie: &str) -> Request {
        Request {
            cookie: cookie.to_owned(),
            action_id: "org.freedesktop.policykit.exec".to_owned(),
            message: "Exécuter un programme".to_owned(),
            users: vec![user(1000, "jimmy"), user(0, "root")],
        }
    }

    fn password() -> Line {
        Line::Prompt {
            text: "Password: ".to_owned(),
            echo: false,
        }
    }

    fn start(attempt: u64, user: &str, cookie: &str) -> Effect {
        Effect::Start {
            attempt,
            user: user.to_owned(),
            cookie: cookie.to_owned(),
        }
    }

    fn finished(cookie: &str, outcome: Outcome) -> Effect {
        finish(cookie.to_owned(), outcome)
    }

    /// A request shown, its password validated and sent to the helper.
    fn checking(agent: &mut Agent, cookie: &str) {
        agent.begin(request(cookie));
        agent.answer();
        let attempt = agent.attempts;
        agent.helper(attempt, password());
    }

    #[test]
    fn a_request_shows_without_talking_to_the_helper() {
        let mut agent = Agent::new();

        assert_eq!(agent.begin(request("a")), Vec::new());

        let dialog = agent.dialog().unwrap();
        assert_eq!(dialog.users, vec!["jimmy", "root"]);
        assert_eq!(dialog.phase, Phase::Ready);
        assert!(!dialog.busy());
    }

    #[test]
    fn validating_starts_a_conversation_that_takes_the_password() {
        let mut agent = Agent::new();
        agent.begin(request("a"));

        assert_eq!(agent.answer(), vec![start(1, "jimmy", "a")]);
        assert!(agent.dialog().unwrap().busy());
        assert_eq!(agent.answer(), Vec::new());
        assert_eq!(
            agent.helper(1, password()),
            vec![Effect::Send { attempt: 1 }]
        );
        assert_eq!(agent.dialog().unwrap().phase, Phase::Checking);
    }

    #[test]
    fn success_answers_polkit_and_closes_the_dialog() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");

        let effects = agent.helper(1, Line::Success);

        assert_eq!(
            effects,
            vec![Effect::Stop, finished("a", Outcome::Authenticated)]
        );
        assert_eq!(agent.dialog(), None);
    }

    #[test]
    fn a_wrong_password_waits_for_another_with_an_error() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");

        assert_eq!(agent.helper(1, Line::Failure), vec![Effect::Stop]);

        let dialog = agent.dialog().unwrap();
        assert_eq!(dialog.phase, Phase::Ready);
        assert_eq!(dialog.note, Some(Note::Error(WRONG_PASSWORD.to_owned())));
        // The next try is a new conversation.
        assert_eq!(agent.answer(), vec![start(2, "jimmy", "a")]);
        assert_eq!(agent.dialog().unwrap().note, None);
    }

    #[test]
    fn what_pam_says_replaces_the_usual_error() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");
        let locked = "Le compte est verrouillé.";

        agent.helper(1, Line::Error(locked.to_owned()));
        agent.helper(1, Line::Failure);

        let note = agent.dialog().unwrap().note;
        assert_eq!(note, Some(Note::Error(locked.to_owned())));
    }

    #[test]
    fn a_helper_that_fails_before_asking_says_so() {
        let mut agent = Agent::new();
        agent.begin(request("a"));
        agent.answer();

        assert_eq!(agent.helper_gone(1), vec![Effect::Stop]);

        let dialog = agent.dialog().unwrap();
        assert_eq!(dialog.phase, Phase::Ready);
        assert_eq!(dialog.note, Some(Note::Error(NOT_STARTED.to_owned())));
    }

    #[test]
    fn a_further_question_of_pam_waits_for_the_user() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");
        let renew = Line::Prompt {
            text: "New password: ".to_owned(),
            echo: false,
        };

        assert_eq!(agent.helper(1, renew), Vec::new());
        assert!(matches!(
            agent.dialog().unwrap().phase,
            Phase::Asking { .. }
        ));
        assert_eq!(agent.answer(), vec![Effect::Send { attempt: 1 }]);
    }

    #[test]
    fn an_earlier_conversation_is_not_listened_to() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");
        agent.helper(1, Line::Failure);
        agent.answer();

        assert_eq!(agent.helper(1, Line::Success), Vec::new());
        assert!(agent.dialog().is_some());
    }

    #[test]
    fn choosing_another_account_costs_no_conversation() {
        let mut agent = Agent::new();
        agent.begin(request("a"));

        assert_eq!(agent.choose(1), Vec::new());
        assert_eq!(agent.dialog().unwrap().user, 1);
        assert_eq!(agent.answer(), vec![start(1, "root", "a")]);
        assert_eq!(agent.choose(5), Vec::new());
    }

    #[test]
    fn choosing_another_account_while_talking_ends_the_conversation() {
        let mut agent = Agent::new();
        agent.begin(request("a"));
        agent.answer();

        assert_eq!(agent.choose(1), vec![Effect::Stop]);
        assert_eq!(agent.dialog().unwrap().phase, Phase::Ready);
    }

    #[test]
    fn giving_up_before_validating_starts_nothing() {
        let mut agent = Agent::new();
        agent.begin(request("a"));

        assert_eq!(agent.dismiss(), vec![finished("a", Outcome::Cancelled)]);
        assert_eq!(agent.dialog(), None);
    }

    #[test]
    fn one_request_at_a_time_the_others_wait() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");

        assert_eq!(agent.begin(request("b")), Vec::new());
        assert_eq!(agent.dialog().unwrap().waiting, 1);

        let effects = agent.dismiss();

        assert_eq!(
            effects,
            vec![Effect::Stop, finished("a", Outcome::Cancelled)]
        );
        let dialog = agent.dialog().unwrap();
        assert_eq!(dialog.waiting, 0);
        assert_eq!(dialog.phase, Phase::Ready);
    }

    #[test]
    fn polkit_can_cancel_a_request_that_waits() {
        let mut agent = Agent::new();
        agent.begin(request("a"));
        agent.begin(request("b"));

        assert_eq!(agent.cancel("b"), vec![finished("b", Outcome::Cancelled)]);
        assert_eq!(agent.dialog().unwrap().waiting, 0);
        assert_eq!(agent.cancel("unknown"), Vec::new());
    }

    #[test]
    fn polkit_can_cancel_the_request_shown() {
        let mut agent = Agent::new();
        checking(&mut agent, "a");

        assert_eq!(
            agent.cancel("a"),
            vec![Effect::Stop, finished("a", Outcome::Cancelled)]
        );
        assert_eq!(agent.dialog(), None);
    }

    #[test]
    fn a_request_nobody_can_answer_ends_at_once() {
        let mut agent = Agent::new();
        let nobody = Request {
            users: Vec::new(),
            ..request("a")
        };

        assert_eq!(
            agent.begin(nobody),
            vec![finished("a", Outcome::Impossible)]
        );
        assert_eq!(agent.dialog(), None);
    }

    #[test]
    fn what_pam_tells_shows_under_the_field() {
        let mut agent = Agent::new();
        agent.begin(request("a"));
        agent.answer();

        agent.helper(1, Line::Info("Posez le doigt sur le lecteur".to_owned()));

        let note = agent.dialog().unwrap().note;
        assert_eq!(
            note,
            Some(Note::Info("Posez le doigt sur le lecteur".to_owned()))
        );
    }

    #[test]
    fn the_account_of_the_user_comes_first() {
        let users = vec![
            user(0, "root"),
            user(1001, "alice"),
            user(1000, "jimmy"),
            user(0, "root"),
        ];

        let ordered = ordered(users, 1000);

        let logins: Vec<&str> = ordered.iter().map(|user| user.login.as_str()).collect();
        assert_eq!(logins, vec!["jimmy", "root", "alice"]);
    }

    #[test]
    fn an_account_shows_its_full_name_and_login() {
        let named = User {
            name: Some("Jimmy Vuidart".to_owned()),
            ..user(1000, "jimmy")
        };

        assert_eq!(named.label(), "Jimmy Vuidart (jimmy)");
        assert_eq!(user(0, "root").label(), "root");
    }
}
