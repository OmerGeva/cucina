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
    },
}

impl Origin {
    pub fn label(&self) -> String {
        match self {
            Origin::User => "you".into(),
            Origin::Agent { client } if client.is_empty() => "an agent".into(),
            Origin::Agent { client } => client.clone(),
        }
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
}

#[cfg(test)]
mod tests {
    use super::{slugify, State};

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
    fn only_starting_and_running_count_as_live() {
        assert!(State::Running.is_live());
        assert!(State::Starting.is_live());
        assert!(!State::Stopped.is_live());
        assert!(!State::Crashed.is_live());
    }
}
