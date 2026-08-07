//! The promise the README makes: if Cucina dies without getting to run any
//! cleanup — `kill -9`, a panic, a power-user with Activity Monitor — the
//! servers it started die too.
//!
//! Nothing inside a single process can test that, because the process under
//! test has to be the one that gets killed. So this runs without the libtest
//! harness (`harness = false` in Cargo.toml) and re-executes itself:
//!
//!   parent  spawns the same binary with CUCINA_CRASH_CHILD set
//!   child   builds a real Supervisor, starts a server, writes both pids out,
//!           then blocks forever
//!   parent  SIGKILLs the child and asserts the server and everything it
//!           spawned are gone
//!
//! A SIGKILLed child runs no destructors, so nothing but the pipe watchdog can
//! save us here — which is exactly the mechanism under test.

use cucina_core::model::{Origin, Server};
use cucina_core::Supervisor;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "CUCINA_CRASH_CHILD";

fn main() {
    match std::env::var(CHILD_ENV) {
        Ok(dir) => child(PathBuf::from(dir)),
        Err(_) => parent(),
    }
}

// ---- the child: a Cucina that is about to be killed -----------------------

fn child(dir: PathBuf) -> ! {
    std::env::set_var("HOME", &dir);

    let sup = Supervisor::new();
    let grandchild_pidfile = dir.join("grandchild.pid");

    // The server spawns a background process of its own, the way a bundler or
    // a reloader would, so we can prove the whole group goes and not just the
    // command we launched.
    let command = format!(
        "sleep 300 & echo $! > {}; wait",
        grandchild_pidfile.display()
    );
    let server = Server {
        id: String::new(),
        name: "victim".into(),
        dir: dir.clone(),
        command,
        group: String::new(),
        tile: 0,
        env: BTreeMap::new(),
        auto_restart: false,
        auto_start: false,
        created_at: 0,
    };

    let saved = sup.upsert(server).expect("upsert");
    sup.start(&saved.id, Origin::User).expect("start");

    // Publish the server's pid once it has one; the parent waits on this file.
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(pid) = sup
            .statuses()
            .into_iter()
            .find(|s| s.id == saved.id)
            .and_then(|s| s.pid)
        {
            std::fs::write(dir.join("server.pid"), pid.to_string()).expect("write server pid");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Wait to be killed. Deliberately no signal handler and no cleanup: the
    // point is that this process gets no chance to tidy up after itself.
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

// ---- the parent: the executioner ------------------------------------------

fn parent() {
    let dir = std::env::temp_dir().join(format!("cucina-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");

    let mut child = std::process::Command::new(std::env::current_exe().expect("current exe"))
        .env(CHILD_ENV, &dir)
        .spawn()
        .expect("spawn the child Cucina");

    let server_pid: u32 = read_pid(&dir.join("server.pid"), "the server's pid");
    let grandchild_pid: u32 = read_pid(&dir.join("grandchild.pid"), "the grandchild's pid");

    check(
        alive(server_pid),
        "the server should be running before the kill",
    );
    check(
        alive(grandchild_pid),
        "the grandchild should be running before the kill",
    );

    // No SIGTERM, no shutdown() — the harshest exit there is.
    unsafe { libc::kill(child.id() as i32, libc::SIGKILL) };
    let _ = child.wait();

    wait_until(
        || !alive(server_pid),
        "the server to die with the supervisor",
    );
    wait_until(
        || !alive(grandchild_pid),
        "the grandchild to die with the supervisor",
    );

    let _ = std::fs::remove_dir_all(&dir);
    println!("test crash_safety::children_die_when_the_supervisor_is_killed ... ok");
    println!("\ntest result: ok. 1 passed; 0 failed");
}

fn read_pid(path: &std::path::Path, what: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(pid) = text.trim().parse() {
                return pid;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    fail(&format!("timed out waiting for {what}"));
}

fn wait_until(mut done: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    fail(&format!("timed out waiting for {what}"));
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn check(ok: bool, what: &str) {
    if !ok {
        fail(what);
    }
}

fn fail(what: &str) -> ! {
    eprintln!("crash_safety FAILED: {what}");
    std::process::exit(1);
}
