use crate::model::{LogLine, Origin, Server, Status};
use serde::{Deserialize, Serialize};

/// One server, definition plus live state — what both the UI and the CLI want.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub server: Server,
    pub status: Status,
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
        Response { ok: true, error: None, data }
    }

    pub fn empty() -> Self {
        Response { ok: true, error: None, data: serde_json::Value::Null }
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
}
