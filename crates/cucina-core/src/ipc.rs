use crate::model::State;
use crate::paths;
use crate::proto::{Request, Response, ServerView};
use crate::supervisor::Supervisor;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The app owns the socket; the CLI and the MCP server are thin clients. That
/// separation is the whole point — an agent can hand a server over and walk
/// away, and you can still stop it from the menu bar.
pub fn serve(sup: Arc<Supervisor>) -> std::io::Result<()> {
    let path = paths::socket_path();

    // If another Cucina is already answering here, this process must not take
    // the socket from it. Unlinking a live socket orphans the running instance
    // while leaving it on screen, which is how duplicate windows and duplicate
    // menu bar icons appear.
    if UnixStream::connect(&path).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "another Cucina already owns the control socket",
        ));
    }

    // Nothing answered, so any file here is stale from a crash.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let sup = sup.clone();
            thread::spawn(move || handle(sup, stream));
        }
    });
    Ok(())
}

fn handle(sup: Arc<Supervisor>, stream: UnixStream) {
    let Ok(write_half) = stream.try_clone() else { return };
    let reader = BufReader::new(stream);
    let mut writer = write_half;

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(&sup, req),
            Err(e) => Response::err(format!("Bad request: {e}")),
        };
        let Ok(mut body) = serde_json::to_vec(&response) else { break };
        body.push(b'\n');
        if writer.write_all(&body).is_err() || writer.flush().is_err() {
            break;
        }
    }
}

fn views(sup: &Arc<Supervisor>) -> Vec<ServerView> {
    let statuses = sup.statuses();
    sup.servers()
        .into_iter()
        .zip(statuses)
        .map(|(server, status)| ServerView { server, status })
        .collect()
}

/// Block until the server is actually listening, so an agent can start one and
/// immediately curl it without racing.
fn await_ready(sup: &Arc<Supervisor>, id: &str, wait_ms: u64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    loop {
        let status = sup.statuses().into_iter().find(|s| s.id == id);
        match status {
            Some(s) if s.port.is_some() => return Ok(()),
            Some(s) if s.state == State::Crashed => {
                return Err(match s.exit_code {
                    Some(c) => format!("{id} exited with code {c} while starting."),
                    None => format!("{id} stopped while starting."),
                })
            }
            Some(s) if s.state == State::Stopped => {
                return Err(format!("{id} stopped before it was ready."))
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            // Not an error: plenty of processes are useful without a TCP port.
            return Ok(());
        }
        thread::sleep(Duration::from_millis(120));
    }
}

fn dispatch(sup: &Arc<Supervisor>, req: Request) -> Response {
    match req {
        Request::Ping => Response::ok(serde_json::json!({ "app": "cucina" })),

        Request::Show => {
            sup.request_show();
            Response::empty()
        }

        Request::List => match serde_json::to_value(views(sup)) {
            Ok(v) => Response::ok(v),
            Err(e) => Response::err(e.to_string()),
        },

        Request::Start { id, origin, wait_ms } => match sup.start(&id, origin) {
            Ok(()) => finish(sup, &id, wait_ms),
            Err(e) => Response::err(e),
        },

        Request::Restart { id, origin, wait_ms } => match sup.restart(&id, origin) {
            Ok(()) => finish(sup, &id, wait_ms),
            Err(e) => Response::err(e),
        },

        Request::Stop { id } => match sup.stop(&id) {
            Ok(()) => Response::empty(),
            Err(e) => Response::err(e),
        },

        Request::Logs { id, tail } => {
            let lines = sup.tail(&id, tail.unwrap_or(200));
            match serde_json::to_value(lines) {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }

        Request::Add { server } => match sup.upsert(server) {
            Ok(s) => match serde_json::to_value(s) {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            },
            Err(e) => Response::err(e),
        },

        Request::Remove { id } => match sup.remove(&id) {
            Ok(()) => Response::empty(),
            Err(e) => Response::err(e),
        },

        Request::Switch { id, path } => match sup.switch_dir(&id, path.into()) {
            Ok(()) => finish(sup, &id, None),
            Err(e) => Response::err(e),
        },
    }
}

fn finish(sup: &Arc<Supervisor>, id: &str, wait_ms: Option<u64>) -> Response {
    if let Some(ms) = wait_ms {
        if let Err(e) = await_ready(sup, id, ms) {
            return Response::err(e);
        }
    }
    let view = views(sup).into_iter().find(|v| v.server.id == id);
    match serde_json::to_value(view) {
        Ok(v) => Response::ok(v),
        Err(e) => Response::err(e.to_string()),
    }
}

/// Used by the app at startup: if another Cucina already holds the socket,
/// this one should not fight it.
pub fn already_running() -> bool {
    UnixStream::connect(paths::socket_path()).is_ok()
}
