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
    // Tasks belong to one server and run in one directory, so unlike the
    // lifecycle tools these never take a group.
    let one_id = json!({
        "type": "string",
        "description": "The server's id, as shown by cucina_list. Not a group — a task runs in one directory."
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
        },
        {
            "name": "cucina_tasks",
            "description": "List the tasks kept on a server — the one-off commands its owner runs there, like a migration or a test suite — with how each one ended last time. A task is not the server's own start command; use cucina_start for that.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": one_id },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_run_task",
            "description": "Run a task that is already on the server, by its taskId from cucina_tasks. Returns a runId; poll it with cucina_run to get the exit code and output. Only one task runs at a time per server.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": one_id,
                    "taskId": {
                        "type": "string",
                        "description": "The task's id, as shown by cucina_tasks."
                    },
                    "session": session_arg
                },
                "required": ["id", "taskId"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_run_command",
            "description": "Run a one-off command in a server's directory, with its environment and in whichever worktree it currently points at — and keep it on the server's task list, so the user sees what you ran and can run it again themselves. Use this for migrations, seeds, test runs and generators. Do not use it to start the server; that is cucina_start. Returns a taskId and a runId.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": one_id,
                    "command": {
                        "type": "string",
                        "description": "The command, exactly as you would type it in that directory."
                    },
                    "session": session_arg
                },
                "required": ["id", "command"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_run",
            "description": "Check on a run started by cucina_run_task or cucina_run_command: whether it is still going, its exit code once it is not, how long it took, and its output. Call this after starting a run rather than assuming it worked.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": one_id,
                    "tail": {
                        "type": "integer",
                        "description": "How many recent output lines to return. Defaults to 200.",
                        "minimum": 1,
                        "maximum": 2000
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_stop_run",
            "description": "Kill a task run and everything it spawned, including one the user started. Use it for a command that will not end on its own, like a console or a file watcher.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "runId": {
                        "type": "string",
                        "description": "The runId returned when the run started, or reported by cucina_run."
                    }
                },
                "required": ["runId"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_delete_task",
            "description": "Take a task off a server's list. Never stops a run that is using it. The list is the user's, so remove something only when they asked you to.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": one_id,
                    "taskId": {
                        "type": "string",
                        "description": "The task's id, as shown by cucina_tasks."
                    }
                },
                "required": ["id", "taskId"],
                "additionalProperties": false
            }
        },
        {
            "name": "cucina_suggest_tasks",
            "description": "What this server's project offers, read from its package.json, Gemfile, Makefile or equivalent. Call this before inventing a command — proposing what the project actually defines beats guessing at one.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": one_id },
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

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Resolve to exactly one server. The task tools run a command in a single
/// directory, so a group would have to mean "run it once per server", and
/// nothing about `db:migrate` says that is what the agent meant.
fn arg_one(c: &mut Client, args: &Value) -> Result<String, String> {
    let key = arg_id(args)?;
    let ids = resolve(c, &key)?;
    match ids.len() {
        1 => Ok(ids.into_iter().next().unwrap_or_default()),
        n => Err(format!(
            "{key} is a project of {n} servers. A task runs in one directory, so name the server: {}",
            ids.join(", ")
        )),
    }
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
        "cucina_tasks" => {
            let id = arg_one(&mut c, args)?;
            let tasks = c.request(&Request::Tasks { id: id.clone() })?.tasks();
            if tasks.is_empty() {
                return Ok(format!(
                    "{id} has no tasks yet. Call cucina_suggest_tasks to see what its project defines, or cucina_run_command to run something and keep it."
                ));
            }
            Ok(pretty(&serde_json::to_value(tasks).unwrap_or_default()))
        }
        "cucina_run_task" | "cucina_run_command" => {
            let id = arg_one(&mut c, args)?;
            let request = if name == "cucina_run_task" {
                let task_id = arg_str(args, "taskId")
                    .ok_or("A taskId is required. Call cucina_tasks to see them.")?;
                Request::RunTask {
                    id: id.clone(),
                    task_id: task_id.to_string(),
                    origin,
                }
            } else {
                let command = arg_str(args, "command").ok_or("A command is required.")?;
                Request::RunCommand {
                    id: id.clone(),
                    command: command.to_string(),
                    origin,
                }
            };
            let run = c.request(&request)?;
            // The run is live and the exit code is the point, so say outright
            // that the answer is not in this reply.
            Ok(format!(
                "{}\n\nStarted. Poll cucina_run with id \"{id}\" for the exit code and output.",
                pretty(&run.data)
            ))
        }
        "cucina_run" => {
            let id = arg_one(&mut c, args)?;
            let tail = args
                .get("tail")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(200);
            let view = c
                .request(&Request::Run {
                    id: id.clone(),
                    tail: Some(tail),
                })?
                .run();
            let Some(run) = view.run else {
                return Ok(format!("{id} has not run a task yet."));
            };
            let elapsed = run.ended_at.unwrap_or_else(cucina_core::model::now_ms) - run.started_at;
            let head = match (run.is_live(), run.exit_code) {
                (true, _) => format!("running for {:.1}s", elapsed as f32 / 1000.0),
                (false, Some(code)) => {
                    format!("exited {code} after {:.1}s", elapsed as f32 / 1000.0)
                }
                (false, None) => format!("stopped after {:.1}s", elapsed as f32 / 1000.0),
            };
            let output: Vec<&str> = view.lines.iter().map(|l| l.text.as_str()).collect();
            Ok(format!(
                "{} · {head} · runId {}\n\n{}",
                run.command,
                run.run_id,
                match output.is_empty() {
                    true => "(no output)".to_string(),
                    false => output.join("\n"),
                }
            ))
        }
        "cucina_stop_run" => {
            let run_id = arg_str(args, "runId")
                .ok_or("A runId is required.")?
                .to_string();
            c.request(&Request::StopRun {
                run_id: run_id.clone(),
            })?;
            Ok(format!("{run_id} stopped."))
        }
        "cucina_delete_task" => {
            let id = arg_one(&mut c, args)?;
            let task_id = arg_str(args, "taskId")
                .ok_or("A taskId is required. Call cucina_tasks to see them.")?;
            c.request(&Request::RemoveTask {
                id: id.clone(),
                task_id: task_id.to_string(),
            })?;
            Ok(format!("{task_id} removed from {id}."))
        }
        "cucina_suggest_tasks" => {
            let id = arg_one(&mut c, args)?;
            let res = c.request(&Request::SuggestTasks { id: id.clone() })?;
            let commands = res
                .data
                .get("commands")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            if commands == 0 {
                return Ok(format!(
                    "Nothing recognisable in {id}'s directory — no package.json, Gemfile, Makefile or equivalent that defines commands."
                ));
            }
            Ok(pretty(&res.data))
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
