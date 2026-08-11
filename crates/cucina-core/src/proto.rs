use crate::model::{LogLine, Origin, Server, Status, Task};
use serde::{Deserialize, Serialize};

/// One server, definition plus live state — what both the UI and the CLI want.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub server: Server,
    pub status: Status,
}

/// A task run and what it has printed. One shape, so the output box and an
/// agent polling a run are reading the same thing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// `None` when this server has never run a task.
    #[serde(default)]
    pub run: Option<crate::model::Run>,
    #[serde(default)]
    pub lines: Vec<LogLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Request {
    Ping,
    /// Bring the existing window forward.
    Show,
    List,
    Start {
        id: String,
        #[serde(default = "Origin::user")]
        origin: Origin,
        /// Block until the server is listening, up to this many milliseconds.
        #[serde(default)]
        wait_ms: Option<u64>,
    },
    Stop {
        id: String,
    },
    Restart {
        id: String,
        #[serde(default = "Origin::user")]
        origin: Origin,
        #[serde(default)]
        wait_ms: Option<u64>,
    },
    Logs {
        id: String,
        #[serde(default)]
        tail: Option<usize>,
    },
    Add {
        server: Server,
    },
    Remove {
        id: String,
    },
    /// Point a server at another directory, restarting it there if it was up.
    Switch {
        id: String,
        path: String,
    },

    // ---- tasks: saved commands that run once and exit --------------------
    /// Every task kept on a server.
    Tasks {
        id: String,
    },
    /// Keep a command without running it. The app never sends this — typing in
    /// the footer runs and adds in one step — but it lets a client build a
    /// list up front.
    AddTask {
        id: String,
        command: String,
    },
    RemoveTask {
        id: String,
        task_id: String,
    },
    RunTask {
        id: String,
        task_id: String,
        #[serde(default = "Origin::user")]
        origin: Origin,
    },
    /// Run a command not saved yet, adding it to the list.
    RunCommand {
        id: String,
        command: String,
        #[serde(default = "Origin::user")]
        origin: Origin,
    },
    /// The current or most recent run for a server, with its output.
    Run {
        id: String,
        #[serde(default)]
        tail: Option<usize>,
    },
    StopRun {
        run_id: String,
    },
    /// What this server's directory offers, read from its manifests.
    SuggestTasks {
        id: String,
    },

    // ---- strays: ports held by processes Cucina does not own --------------
    /// Look at what is listening right now. Runs three bounded probes; there
    /// is no cached answer, because a cached one would be a lie by the time
    /// anybody read it.
    Strays,
    StopStray {
        pid: u32,
    },
}

impl Origin {
    pub fn user() -> Origin {
        Origin::User
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

impl Response {
    pub fn ok(data: serde_json::Value) -> Self {
        Response {
            ok: true,
            error: None,
            data,
        }
    }

    pub fn empty() -> Self {
        Response {
            ok: true,
            error: None,
            data: serde_json::Value::Null,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Response {
            ok: false,
            error: Some(message.into()),
            data: serde_json::Value::Null,
        }
    }

    pub fn views(&self) -> Vec<ServerView> {
        serde_json::from_value(self.data.clone()).unwrap_or_default()
    }

    pub fn lines(&self) -> Vec<LogLine> {
        serde_json::from_value(self.data.clone()).unwrap_or_default()
    }

    pub fn tasks(&self) -> Vec<Task> {
        serde_json::from_value(self.data.clone()).unwrap_or_default()
    }

    pub fn run(&self) -> RunView {
        serde_json::from_value(self.data.clone()).unwrap_or_default()
    }

    pub fn strays(&self) -> Vec<crate::strays::Stray> {
        serde_json::from_value(self.data.clone()).unwrap_or_default()
    }
}
