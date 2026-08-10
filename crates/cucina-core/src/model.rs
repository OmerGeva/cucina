use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Turn a human name into a stable, CLI-friendly id: "My API" -> "my-api".
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = true; // leading dashes are suppressed
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("server");
    }
    out
}

/// A server definition: where to run, what to run, and how it should behave.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    pub command: String,
    /// Optional grouping, so a project's services travel together. Empty means
    /// ungrouped. Group membership is resolved by callers rather than baked
    /// into the protocol — the list response already carries it.
    #[serde(default)]
    pub group: String,
    /// Which ceramic tile the card wears. 0 lets Cucina choose a stable one
    /// from the id, so a fresh set of servers already looks varied.
    #[serde(default)]
    pub tile: u32,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub auto_restart: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub created_at: u64,
}

/// A project. Membership lives on the server (`Server::group`); this record
/// exists so a project can carry presentation of its own, like an icon.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Stopped,
    Starting,
    Running,
    Crashed,
}

impl State {
    pub fn is_live(self) -> bool {
        matches!(self, State::Starting | State::Running)
    }
}

/// Who asked for this run. Lets the UI show "started by agent" and lets us
/// apply idle timeouts only to runs a human didn't personally kick off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    User,
    Agent {
        #[serde(default)]
        client: String,
        /// What the agent said it was working on. MCP has no notion of a
        /// session, so this is only ever as good as what the agent chose to
        /// pass us — often empty, and never load-bearing.
        #[serde(default)]
        session: String,
    },
}

/// Agents are wordy when you let them, and a session name lands in a header
/// beside the server's own. Long enough for a ticket title, short enough that
/// the server stays the loudest thing on the screen.
const SESSION_MAX: usize = 48;

impl Origin {
    /// Clamped here rather than at each caller: the MCP tool argument and the
    /// CLI flag both feed this, and neither can vouch for what it was handed.
    pub fn agent(client: impl Into<String>, session: &str) -> Origin {
        let session = session.trim();
        let session = match session.char_indices().nth(SESSION_MAX) {
            Some((cut, _)) => format!("{}…", session[..cut].trim_end()),
            None => session.to_string(),
        };
        Origin::Agent {
            client: client.into(),
            session,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Origin::User => "you".into(),
            Origin::Agent { client, .. } if client.is_empty() => "an agent".into(),
            Origin::Agent { client, .. } => client.clone(),
        }
    }
}

/// A command the user keeps on a server — `bin/rails db:migrate`, `npm run
/// seed`. Distinct from the server's own `command`, which is the one that
/// starts it and never appears here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Slugged from the command, so it is stable across runs without asking
    /// the user to name anything. Never shown; it exists so an agent can
    /// address a task by name in a later session.
    pub id: String,
    pub command: String,
    /// How the last run ended. `Some(0)` succeeded, `Some(n)` failed, and
    /// `None` alongside a `last_run_at` means a signal ended it — which is
    /// what the user pressing Stop looks like from here.
    #[serde(default)]
    pub last_exit: Option<i32>,
    /// `None` until it has run once, which is what the UI reads as "no
    /// outcome to report yet".
    #[serde(default)]
    pub last_run_at: Option<u64>,
}

/// A command name is capped well below the session cap: it sits in a 352px
/// menu row and anything longer is ellipsised there anyway.
const COMMAND_MAX: usize = 300;

impl Task {
    /// A readable id derived from the command. Punctuation collapses, so two
    /// different commands can land on the same slug — `db:migrate` and
    /// `db_migrate` both flatten to `db-migrate`. `with_unique_id` is what
    /// keeps them apart; this only has to be stable and legible.
    pub fn slug(command: &str) -> String {
        let mut out = String::with_capacity(command.len());
        let mut prev_dash = true;
        for ch in command.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str("task");
        }
        out.chars().take(64).collect()
    }

    pub fn new(command: &str) -> Task {
        let command: String = command.trim().chars().take(COMMAND_MAX).collect();
        Task {
            id: Task::slug(&command),
            command,
            last_exit: None,
            last_run_at: None,
        }
    }

    /// Settle this task's id against the ones already on the server, the same
    /// way `upsert` settles a server id. An agent addresses a task by this, so
    /// two tasks sharing one would run whichever came first — silently, and
    /// with the user's database on the other end of it.
    pub fn with_unique_id(mut self, existing: &[Task]) -> Task {
        let base = self.id.clone();
        let mut n = 2;
        while existing.iter().any(|t| t.id == self.id) {
            self.id = format!("{base}-{n}");
            n += 1;
        }
        self
    }
}

/// One execution of a task. Unlike a server, a run is expected to end, and its
/// exit code is the point rather than an accident.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub run_id: String,
    pub server_id: String,
    pub task_id: String,
    pub command: String,
    pub started_at: u64,
    /// `None` while it is still running. That is the only "is it live" test
    /// there is — a run has no starting state to pass through.
    #[serde(default)]
    pub ended_at: Option<u64>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub origin: Option<Origin>,
    /// When output last arrived. A run that has gone quiet is reported, not
    /// called stuck — `rails console` and `tail -f` are quiet on purpose.
    #[serde(default)]
    pub last_output_at: u64,
}

impl Run {
    pub fn is_live(&self) -> bool {
        self.ended_at.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: String,
    pub state: State,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub started_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub origin: Option<Origin>,
    #[serde(default)]
    pub restarts: u32,
}

impl Status {
    pub fn stopped(id: &str) -> Self {
        Status {
            id: id.to_string(),
            state: State::Stopped,
            pid: None,
            port: None,
            started_at: None,
            exit_code: None,
            origin: None,
            restarts: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
    /// Cucina's own commentary: "started", "exited with code 1", etc.
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub ts: u64,
    pub stream: Stream,
    pub text: String,
}

/// Pushed to the UI and to socket subscribers. Log lines arrive batched so a
/// chatty dev server can't flood the IPC channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Status(Status),
    /// Someone tried to launch a second Cucina; raise the window we have.
    Show,
    Log {
        id: String,
        lines: Vec<LogLine>,
    },
    ServersChanged,
    /// A task run started, produced its first output, or ended.
    Run(Run),
    /// A server's task list changed — added, removed, or an outcome recorded.
    Tasks {
        id: String,
        tasks: Vec<Task>,
    },
}

#[cfg(test)]
mod tests {
    use super::{slugify, Origin, State, Task, COMMAND_MAX, SESSION_MAX};

    #[test]
    fn slugs_are_stable_and_cli_friendly() {
        assert_eq!(slugify("My API"), "my-api");
        assert_eq!(slugify("Acme Data Service"), "acme-data-service");
        assert_eq!(slugify("api"), "api");
    }

    #[test]
    fn slugs_collapse_punctuation_and_trim_dashes() {
        assert_eq!(slugify("  API (v2)!  "), "api-v2");
        assert_eq!(slugify("a---b"), "a-b");
        assert_eq!(slugify("web_app"), "web-app");
    }

    /// A name with nothing usable in it still has to produce an id.
    #[test]
    fn slugs_never_come_back_empty() {
        assert_eq!(slugify(""), "server");
        assert_eq!(slugify("!!!"), "server");
        assert_eq!(slugify("日本語"), "server");
    }

    #[test]
    fn a_session_name_is_trimmed_and_clamped() {
        let session = |s: &str| match Origin::agent("claude-code", s) {
            Origin::Agent { session, .. } => session,
            Origin::User => unreachable!(),
        };

        assert_eq!(
            session("  Chemical patents analysis tool  "),
            "Chemical patents analysis tool"
        );
        assert_eq!(session(""), "");

        // Clamped on a char boundary, with the trailing space eaten so the
        // ellipsis sits against a word rather than floating off one.
        let long = session(&"a".repeat(SESSION_MAX + 10));
        assert_eq!(long.chars().count(), SESSION_MAX + 1);
        assert!(long.ends_with('…'));

        // Multi-byte input must not panic or split a character in half.
        let jp = session(&"日本語".repeat(40));
        assert_eq!(jp.chars().count(), SESSION_MAX + 1);
    }

    #[test]
    fn task_ids_read_like_the_command_they_came_from() {
        assert_eq!(Task::slug("bin/rails db:migrate"), "bin-rails-db-migrate");
        assert_eq!(Task::slug("npm run dev"), "npm-run-dev");
    }

    /// Two commands that differ only in punctuation slug to the same thing.
    /// An id that stayed collapsed would have an agent run `db_migrate` when
    /// it asked for `db:migrate`, so the second one has to be settled.
    #[test]
    fn colliding_task_ids_are_settled_against_the_list() {
        let first = Task::new("rake db:migrate");
        let second = Task::new("rake db_migrate").with_unique_id(std::slice::from_ref(&first));

        assert_eq!(first.id, "rake-db-migrate");
        assert_eq!(second.id, "rake-db-migrate-2");

        let third = Task::new("rake db.migrate").with_unique_id(&[first, second]);
        assert_eq!(third.id, "rake-db-migrate-3");
    }

    /// An id has to come back usable whatever the command looked like — it is
    /// what an agent addresses a task by.
    #[test]
    fn task_ids_are_never_empty_or_unbounded() {
        assert_eq!(Task::slug(""), "task");
        assert_eq!(Task::slug("!!!"), "task");
        assert_eq!(Task::slug("日本語"), "task");
        assert!(Task::slug(&"a".repeat(500)).chars().count() <= 64);
    }

    #[test]
    fn a_new_task_is_trimmed_clamped_and_unrun() {
        let task = Task::new("  bin/rails db:seed  ");
        assert_eq!(task.command, "bin/rails db:seed");
        assert_eq!(task.id, "bin-rails-db-seed");
        assert!(task.last_run_at.is_none());
        assert!(task.last_exit.is_none());

        let long = Task::new(&"x".repeat(COMMAND_MAX + 100));
        assert_eq!(long.command.chars().count(), COMMAND_MAX);
    }

    #[test]
    fn only_starting_and_running_count_as_live() {
        assert!(State::Running.is_live());
        assert!(State::Starting.is_live());
        assert!(!State::Stopped.is_live());
        assert!(!State::Crashed.is_live());
    }
}
