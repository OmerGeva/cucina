//! Cucina's kitchen: the process supervisor, its store, and the socket
//! protocol that the app, the CLI and the MCP server all speak.

pub mod client;
pub mod git;
pub mod ipc;
pub mod logs;
pub mod model;
pub mod paths;
pub mod ports;
pub mod proto;
pub mod store;
pub mod supervisor;

pub use git::Worktree;
pub use model::{Event, Group, LogLine, Origin, Server, State, Status, Stream};
pub use proto::{Request, Response, ServerView};
pub use supervisor::Supervisor;
