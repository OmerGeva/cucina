//! A minimal MCP server over stdio: JSON-RPC 2.0, newline-delimited.
//!
//! Hand-rolled rather than pulled from an SDK — the surface is five tools and
//! three methods, and this keeps the binary tiny and dependency-free.

use cucina_core::client::Client;
use cucina_core::model::Origin;
use cucina_core::proto::Request;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::OnceLock;

const LATEST_PROTOCOL: &str = "2025-06-18";
const WAIT_MS: u64 = 45_000;

/// Whoever the client said it was in the handshake. One server process serves
/// exactly one client for its whole life, so the first answer is the only one.
static CLIENT: OnceLock<String> = OnceLock::new();

pub fn run() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        // No id means it's a notification: act on it, but stay silent.
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };

        let reply = match handle(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            }),
        };
        if writeln!(stdout, "{reply}").is_err() || stdout.flush().is_err() {
            return;
        }
    }
}

fn handle(method: &str, params: &Value) -> Result<Value, (i32, String)> {
    match method {
        "initialize" => {
            if let Some(name) = client_from(params) {
                let _ = CLIENT.set(name);
            }

            // Echo the client's protocol version when they name one.
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(LATEST_PROTOCOL)
                .to_string();
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "cucina", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Cucina supervises long-running local dev servers. \
            Start one and it keeps running after you finish — you do not need to hold a background \
            process open or remember to kill it, and the user can stop it from the menu bar."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((-32602, "Missing tool name.".to_string()))?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(name, &args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(message) => json!({
                    "content": [{ "type": "text", "text": message }],
                    "isError": true
                }),
            })
        }
        other => Err((-32601, format!("Unknown method: {other}"))),
    }
}

fn tools() -> Value {
    let id_arg = json!({
        "type": "string",
        "description": "A server id, or a group name to act on every server in that project at once. Both are shown by cucina_list."
    });
    // Nothing in MCP tells a server which conversation it is talking to, so
    // the only way to show the user which of their sessions started a server
    // is to ask the agent outright.
    let session_arg = json!({
        "type": "string",
        "description": "What this session is called. If your conversation or session already has a title, pass that verbatim. Otherwise a few words naming what you are working on — the task, ticket or feature. Shown to the user beside your name so they can tell which of your sessions started this server. Always pass it."
    });
    json!([
        {
            "name": "cucina_list",
            "description": "List every server Cucina knows about, with its current state, port, uptime and directory. Call this first to discover ids.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "cucina_start",
            "description": "Start a server. It keeps running after you finish your turn, so you do not need to hold a background shell open. Set wait=true to block until it is actually listening on a port.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": id_arg,
                    "session": session_arg,
                    "wait": {
                        "type": "boolean",
                        "description": "Block until the server is listening (up to 45s). Defaults to true."
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_stop",
            "description": "Stop a running server and everything it spawned.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": id_arg },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_restart",
            "description": "Restart a server — the usual way to pick up a config change.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": id_arg,
                    "session": session_arg,
                    "wait": { "type": "boolean", "description": "Block until listening. Defaults to true." }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_worktrees",
            "description": "List the git worktrees a server can run from, with the branch name of each, which one is the repository's main worktree, and which one the server currently points at. Use this to find out where a server is running before switching it.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": id_arg },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_switch",
            "description": "Point a server at a different git worktree, given a branch name. If the server is running it is stopped and started again in the new worktree, because a server still running from a directory you have moved away from is misleading. Does not accept a group.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The server's id, as shown by cucina_list."
                    },
                    "worktree": {
                        "type": "string",
                        "description": "Branch name of the target worktree, as shown by cucina_worktrees."
                    }
                },
                "required": ["id", "worktree"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_logs",
            "description": "Read a server's recent stdout and stderr. Use this to diagnose why something failed to start or is returning errors.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": id_arg,
                    "tail": {
                        "type": "integer",
                        "description": "How many recent lines to return. Defaults to 200.",
                        "minimum": 1,
                        "maximum": 2000
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        }
    ])
}

/// The handshake is the one place a client names itself, and every client
/// names itself to every server it opens — which is what makes the attribution
/// on a card cost the user no setup at all.
fn client_from(params: &Value) -> Option<String> {
    params
        .get("clientInfo")
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// An explicit env var wins: it lets a wrapper call itself something the
/// handshake has no way to say. Otherwise take the client at its word, and
/// only fall back to the anonymous label if it never introduced itself.
fn client_name() -> String {
    if let Ok(name) = std::env::var("CUCINA_CLIENT") {
        return name;
    }
    CLIENT
        .get()
        .cloned()
        .unwrap_or_else(|| "an agent".to_string())
}

fn arg_session(args: &Value) -> &str {
    args.get("session")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn arg_id(args: &Value) -> Result<String, String> {
    args.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "An id is required. Call cucina_list to see them.".to_string())
}

/// An id may name one server or a whole group, so an agent can bring a
/// project's services up together.
fn resolve(c: &mut Client, key: &str) -> Result<Vec<String>, String> {
    let views = c.request(&Request::List)?.views();
    let lowered = key.to_lowercase();
    if let Some(view) = views
        .iter()
        .find(|v| v.server.id.to_lowercase() == lowered || v.server.name.to_lowercase() == lowered)
    {
        return Ok(vec![view.server.id.clone()]);
    }
    let group: Vec<String> = views
        .iter()
        .filter(|v| !v.server.group.is_empty() && v.server.group.to_lowercase() == lowered)
        .map(|v| v.server.id.clone())
        .collect();
    if group.is_empty() {
        let known = views
            .iter()
            .map(|v| v.server.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "No server or group called {key}. Known ids: {known}"
        ));
    }
    Ok(group)
}

fn wait_ms(args: &Value) -> Option<u64> {
    let wants = args.get("wait").and_then(Value::as_bool).unwrap_or(true);
    wants.then_some(WAIT_MS)
}

fn call(name: &str, args: &Value) -> Result<String, String> {
    let mut c = Client::connect_or_launch()?;
    let origin = Origin::agent(client_name(), arg_session(args));

    let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());

    match name {
        "cucina_list" => {
            let res = c.request(&Request::List)?;
            let views = res.views();
            if views.is_empty() {
                return Ok("No servers defined yet. The user can add one in the Cucina app, or with `cucina add --name api --dir . --command \"npm run dev\"`.".into());
            }
            Ok(pretty(&res.data))
        }
        "cucina_start" | "cucina_restart" => {
            let targets = resolve(&mut c, &arg_id(args)?)?;
            let mut out = Vec::new();
            for id in targets {
                let request = if name == "cucina_restart" {
                    Request::Restart {
                        id: id.clone(),
                        origin: origin.clone(),
                        wait_ms: wait_ms(args),
                    }
                } else {
                    Request::Start {
                        id: id.clone(),
                        origin: origin.clone(),
                        wait_ms: wait_ms(args),
                    }
                };
                match c.request(&request) {
                    Ok(res) => out.push(pretty(&res.data)),
                    Err(e) => out.push(format!("{{\"id\": \"{id}\", \"error\": {e:?}}}")),
                }
            }
            Ok(out.join("\n"))
        }
        "cucina_stop" => {
            let targets = resolve(&mut c, &arg_id(args)?)?;
            let mut out = Vec::new();
            for id in targets {
                match c.request(&Request::Stop { id: id.clone() }) {
                    Ok(_) => out.push(format!("{id} stopped.")),
                    Err(e) => out.push(format!("{id}: {e}")),
                }
            }
            Ok(out.join("\n"))
        }
        "cucina_worktrees" => {
            let id = arg_id(args)?;
            let views = c.request(&Request::List)?.views();
            let view = views
                .iter()
                .find(|v| v.server.id == id)
                .ok_or_else(|| format!("No server called {id}."))?;
            let trees = cucina_core::git::worktrees(&view.server.dir);
            if trees.is_empty() {
                return Ok(format!(
                    "{id} is not in a git worktree ({}).",
                    view.server.dir.display()
                ));
            }
            Ok(pretty(&serde_json::to_value(trees).unwrap_or_default()))
        }
        "cucina_switch" => {
            let id = arg_id(args)?;
            let target = args
                .get("worktree")
                .and_then(Value::as_str)
                .ok_or("A worktree branch name is required. Call cucina_worktrees to see them.")?;
            let views = c.request(&Request::List)?.views();
            let view = views
                .iter()
                .find(|v| v.server.id == id)
                .ok_or_else(|| format!("No server called {id}."))?;
            let trees = cucina_core::git::worktrees(&view.server.dir);
            let lowered = target.to_lowercase();
            let tree = trees
                .iter()
                .find(|w| w.branch.to_lowercase() == lowered)
                .ok_or_else(|| {
                    format!(
                        "No worktree called {target}. Available: {}",
                        trees
                            .iter()
                            .map(|w| w.branch.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            let res = c.request(&Request::Switch {
                id: id.clone(),
                path: tree.path.to_string_lossy().to_string(),
            })?;
            Ok(pretty(&res.data))
        }
        "cucina_logs" => {
            let id = arg_id(args)?;
            let tail = args
                .get("tail")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(200);
            let res = c.request(&Request::Logs {
                id: id.clone(),
                tail: Some(tail),
            })?;
            let lines = res.lines();
            if lines.is_empty() {
                return Ok(format!("{id} has produced no output."));
            }
            Ok(lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"))
        }
        other => Err(format!("Unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole attribution rests on this one handshake field, and a client
    /// that stopped sending it would fail silently back to "an agent".
    #[test]
    fn a_client_is_read_from_the_handshake() {
        assert_eq!(
            client_from(&json!({ "clientInfo": { "name": "claude-code", "version": "2.0.1" } })),
            Some("claude-code".to_string())
        );
        assert_eq!(
            client_from(&json!({ "clientInfo": { "name": " codex " } })),
            Some("codex".to_string())
        );
    }

    /// A client that sends no clientInfo, or a blank name, must leave the
    /// anonymous label in place rather than blanking the attribution out.
    #[test]
    fn a_nameless_client_is_left_anonymous() {
        assert_eq!(client_from(&json!({})), None);
        assert_eq!(client_from(&json!({ "clientInfo": {} })), None);
        assert_eq!(
            client_from(&json!({ "clientInfo": { "name": "  " } })),
            None
        );
        assert_eq!(client_from(&json!({ "clientInfo": { "name": 7 } })), None);
    }

    /// One process serves one client for its whole life, so the first name we
    /// hear is the only one — a later handshake cannot relabel running servers.
    #[test]
    fn the_first_handshake_wins() {
        handle(
            "initialize",
            &json!({ "clientInfo": { "name": "claude-code" } }),
        )
        .expect("initialize");
        handle(
            "initialize",
            &json!({ "clientInfo": { "name": "cursor-vscode" } }),
        )
        .expect("initialize");

        assert_eq!(CLIENT.get().map(String::as_str), Some("claude-code"));
    }
}
