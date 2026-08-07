//! What this crate exists to guarantee: a server Cucina starts is a server
//! Cucina can kill — including everything that server spawned, and including
//! the case where Cucina itself goes away without getting to run any cleanup.
//!
//! These drive the real supervisor against real processes rather than mocks,
//! because the property under test is about process groups and signals and
//! there is nothing left to test once those are stubbed out.
//!
//! `HOME` is redirected to a scratch directory before the first `Supervisor`
//! is built, so a test run never reads or writes the developer's own
//! `~/Library/Application Support/Cucina/servers.json`.

use cucina_core::model::{Origin, Server};
use cucina_core::Supervisor;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

/// Every test shares one scratch HOME and one store file, so they take turns
/// rather than racing each other's saves.
static LOCK: Mutex<()> = Mutex::new(());
static SCRATCH: OnceLock<PathBuf> = OnceLock::new();

fn scratch() -> PathBuf {
    SCRATCH
        .get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("cucina-tests-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("scratch dir");
            static ONCE: Once = Once::new();
            ONCE.call_once(|| std::env::set_var("HOME", &dir));
            dir
        })
        .clone()
}

fn server(name: &str, command: &str) -> Server {
    Server {
        id: String::new(),
        name: name.into(),
        dir: scratch(),
        command: command.into(),
        group: String::new(),
        tile: 0,
        env: BTreeMap::new(),
        auto_restart: false,
        auto_start: false,
        created_at: 0,
    }
}

/// Poll rather than sleep a fixed amount: starting a server goes through a
/// login shell, which is fast on a laptop and not always fast on CI.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn alive(pid: u32) -> bool {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("kill -0 {pid} 2>/dev/null"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pid_of(sup: &Supervisor, id: &str) -> Option<u32> {
    sup.statuses().into_iter().find(|s| s.id == id)?.pid
}

fn is_live(sup: &Supervisor, id: &str) -> bool {
    sup.statuses()
        .iter()
        .find(|s| s.id == id)
        .is_some_and(|s| s.state.is_live())
}

#[test]
fn starts_a_server_and_stops_it_again() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();
    let s = sup
        .upsert(server("start-stop", "sleep 120"))
        .expect("upsert");

    sup.start(&s.id, Origin::User).expect("start");
    until("the server to report a pid", || {
        pid_of(&sup, &s.id).is_some()
    });

    let pid = pid_of(&sup, &s.id).unwrap();
    assert!(alive(pid), "the process should be running");

    sup.stop(&s.id).expect("stop");
    until("the server to stop", || !is_live(&sup, &s.id));
    until("the process to die", || !alive(pid));

    sup.remove(&s.id).expect("remove");
}

/// The reason each server gets its own process group. A dev server that
/// forks — a bundler, a reloader, `npm` spawning `node` — must not leave the
/// grandchild holding the port after the parent is signalled.
#[test]
fn kills_processes_the_server_spawned_itself() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();

    let pidfile = scratch().join("grandchild.pid");
    let _ = std::fs::remove_file(&pidfile);
    let command = format!("sleep 120 & echo $! > {}; wait", pidfile.display());
    let s = sup.upsert(server("grandchild", &command)).expect("upsert");

    sup.start(&s.id, Origin::User).expect("start");
    until("the grandchild to record its pid", || {
        std::fs::read_to_string(&pidfile).is_ok_and(|t| !t.trim().is_empty())
    });

    let grandchild: u32 = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .expect("a pid");
    assert!(alive(grandchild), "the grandchild should be running");

    sup.stop(&s.id).expect("stop");
    until("the whole process group to die", || !alive(grandchild));

    sup.remove(&s.id).expect("remove");
}

/// What happens when the app quits: everything it started goes with it.
#[test]
fn shutdown_takes_every_running_server_with_it() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();

    let a = sup
        .upsert(server("shutdown-a", "sleep 120"))
        .expect("upsert a");
    let b = sup
        .upsert(server("shutdown-b", "sleep 120"))
        .expect("upsert b");
    sup.start(&a.id, Origin::User).expect("start a");
    sup.start(&b.id, Origin::User).expect("start b");

    until("both servers to report pids", || {
        pid_of(&sup, &a.id).is_some() && pid_of(&sup, &b.id).is_some()
    });
    let pids = [pid_of(&sup, &a.id).unwrap(), pid_of(&sup, &b.id).unwrap()];

    sup.shutdown();
    until("both processes to die", || !pids.iter().any(|p| alive(*p)));

    let _ = sup.remove(&a.id);
    let _ = sup.remove(&b.id);
}

/// Restarting is how a config change is picked up, so the old process must be
/// gone rather than merely replaced in the table.
#[test]
fn restart_replaces_the_running_process() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();
    let s = sup.upsert(server("restart", "sleep 120")).expect("upsert");

    sup.start(&s.id, Origin::User).expect("start");
    until("the first process", || pid_of(&sup, &s.id).is_some());
    let first = pid_of(&sup, &s.id).unwrap();

    sup.restart(&s.id, Origin::User).expect("restart");
    until("a different process", || {
        pid_of(&sup, &s.id).is_some_and(|p| p != first)
    });
    until("the first process to die", || !alive(first));

    let second = pid_of(&sup, &s.id).unwrap();
    sup.stop(&s.id).expect("stop");
    until("the second process to die", || !alive(second));

    sup.remove(&s.id).expect("remove");
}

#[test]
fn refuses_a_server_it_could_not_run() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();

    assert!(
        sup.upsert(server("", "sleep 1")).is_err(),
        "name is required"
    );
    assert!(
        sup.upsert(server("no-command", "  ")).is_err(),
        "command is required"
    );

    let mut bad = server("nowhere", "sleep 1");
    bad.dir = PathBuf::from("/definitely/not/a/directory");
    assert!(sup.upsert(bad).is_err(), "directory must exist");
}

/// Ids are derived from the name and have to stay unique, because the CLI and
/// the MCP tools address servers by id.
#[test]
fn gives_servers_distinct_ids() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sup = Supervisor::new();

    let a = sup.upsert(server("Same Name", "sleep 1")).expect("first");
    let b = sup.upsert(server("Same Name", "sleep 1")).expect("second");
    assert_eq!(a.id, "same-name");
    assert_ne!(a.id, b.id);

    sup.remove(&a.id).expect("remove a");
    sup.remove(&b.id).expect("remove b");
    assert!(sup.get(&a.id).is_none());
}
