use crate::logs::Ring;
use crate::model::*;
use crate::paths;
use crate::ports;
use crate::store;

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::Duration;

pub type Listener = Box<dyn Fn(Event) + Send + Sync + 'static>;

/// How long we wait after SIGTERM before insisting with SIGKILL.
const TERM_GRACE: Duration = Duration::from_secs(6);
/// Log lines are batched over this window so a chatty server can't flood IPC.
const FLUSH_WINDOW: Duration = Duration::from_millis(120);
/// Port probing is strictly bounded — we never poll on an idle timer.
const PROBE_INTERVAL: Duration = Duration::from_millis(800);
const PROBE_ATTEMPTS: u32 = 40;
/// Crash-loop guard for auto-restart.
const RESTART_LIMIT: u32 = 5;
const RESTART_WINDOW_MS: u64 = 60_000;

/// Held for the life of a run. Dropping it closes the pipe, which is what
/// tells the child's watchdog that Cucina is gone.
/// Never read — held only so that dropping it closes the descriptor.
struct WatchPipe(#[allow(dead_code)] OwnedFd);

/// Cucina normally stops servers itself, but it cannot run any code at all if
/// it is force-quit, SIGKILLed or crashes. So every child also gets the read
/// end of a pipe whose only writer is this process, and a tiny shell watchdog
/// that blocks on it. However Cucina dies, that pipe closes, the watchdog
/// wakes on EOF and takes the whole process group down with it.
fn watch_pipe() -> std::io::Result<(i32, WatchPipe)> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // The write end must not reach any child, or EOF would never arrive.
    unsafe { libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC) };
    Ok((fds[0], WatchPipe(unsafe { OwnedFd::from_raw_fd(fds[1]) })))
}

/// Wrap the user's command so it carries its own dead-man's switch on fd 3.
fn wrap(command: &str) -> String {
    format!(
        "{{ read -r _ <&3; kill -TERM -$$ 2>/dev/null; sleep 3; kill -KILL -$$ 2>/dev/null; }} &\n\
         __cucina_watch=$!\n\
         exec 3<&-\n\
         {command}\n\
         __cucina_status=$?\n\
         kill \"$__cucina_watch\" 2>/dev/null\n\
         exit $__cucina_status\n"
    )
}

/// The one place a child is configured, so a task run inherits every property
/// that makes a server well-behaved: the login shell that finds `npm`, its own
/// process group, and the fd-3 watchdog that kills it if Cucina dies.
fn child_command(
    dir: &std::path::Path,
    command: &str,
    env: &BTreeMap<String, String>,
    read_fd: i32,
) -> Command {
    let mut cmd = Command::new(paths::login_shell());
    // A login shell so PATH, nvm, asdf and mise resolve exactly as they do
    // in Terminal — Finder-launched apps otherwise get a bare environment.
    cmd.arg("-lc")
        .arg(wrap(command))
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Without this, a Finder-launched Cucina has only launchd's bare PATH
    // and commands like `npm` are simply not found.
    if let Some(path) = paths::login_path() {
        cmd.env("PATH", path);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.env("CUCINA", "1");
    cmd.env("FORCE_COLOR", "0");
    unsafe {
        // Own process group, so stopping kills the whole tree rather than
        // orphaning children. The pipe's read end lands on fd 3, where the
        // watchdog in `wrap` waits for it.
        cmd.pre_exec(move || {
            libc::setsid();
            if libc::dup2(read_fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if read_fd != 3 {
                libc::close(read_fd);
            }
            Ok(())
        });
    }
    cmd
}

struct Runtime {
    status: Status,
    pgid: Option<i32>,
    /// Kept alive for the duration of the run; see `watch_pipe`.
    watch: Option<WatchPipe>,
    /// Set when a stop was requested, so the monitor doesn't auto-restart.
    stopping: bool,
    /// Bumped on every start; stale threads compare against it and bail.
    generation: u64,
    restart_window_start: u64,
}

impl Runtime {
    fn new(id: &str) -> Self {
        Runtime {
            status: Status::stopped(id),
            pgid: None,
            watch: None,
            stopping: false,
            generation: 0,
            restart_window_start: 0,
        }
    }
}

/// A task run in flight, or the last one that finished. One per server, which
/// is the whole concurrency policy: most of these commands mutate one database
/// in one directory, and two migrations at once is a corruption rather than an
/// inconvenience.
struct Active {
    run: Run,
    pgid: Option<i32>,
    /// Kept alive for the duration of the run; see `watch_pipe`.
    watch: Option<WatchPipe>,
    /// Set when the user pressed Stop, so the exit reads as deliberate.
    stopping: bool,
    generation: u64,
    /// Where this run's output starts in the server's log. A run writes into
    /// the same stream the server does — one window, in order — so this is the
    /// only thing that marks which lines were its own.
    from_seq: u64,
}

pub struct Supervisor {
    servers: Mutex<Vec<Server>>,
    groups: Mutex<Vec<Group>>,
    /// Saved commands, keyed by server id. See `store::Document::tasks` for
    /// why these do not live on `Server`.
    tasks: Mutex<BTreeMap<String, Vec<Task>>>,
    /// The current or most recent task run, keyed by server id.
    runs: Mutex<HashMap<String, Active>>,
    next_run: AtomicU64,
    rt: Mutex<HashMap<String, Runtime>>,
    logs: Mutex<HashMap<String, Ring>>,
    listeners: Mutex<Vec<Listener>>,
    /// (has_pending, condvar) — the flusher parks here and burns no CPU while
    /// nothing is producing output.
    flush: Arc<(Mutex<bool>, Condvar)>,
}

fn kill_group(pgid: i32, sig: i32) {
    // Negative pid targets the whole process group, which is what catches
    // `npm run dev` spawning vite spawning esbuild.
    unsafe {
        libc::killpg(pgid, sig);
    }
}

fn group_alive(pgid: i32) -> bool {
    unsafe { libc::killpg(pgid, 0) == 0 }
}

impl Supervisor {
    pub fn new() -> Arc<Self> {
        let doc = store::load();
        let mut rt = HashMap::new();
        for s in &doc.servers {
            rt.insert(s.id.clone(), Runtime::new(&s.id));
        }
        let sup = Arc::new(Supervisor {
            servers: Mutex::new(doc.servers),
            groups: Mutex::new(doc.groups),
            tasks: Mutex::new(doc.tasks),
            runs: Mutex::new(HashMap::new()),
            next_run: AtomicU64::new(1),
            rt: Mutex::new(rt),
            logs: Mutex::new(HashMap::new()),
            listeners: Mutex::new(Vec::new()),
            flush: Arc::new((Mutex::new(false), Condvar::new())),
        });
        sup.spawn_flusher();
        sup
    }

    // ---- events -----------------------------------------------------------

    pub fn subscribe(&self, listener: Listener) {
        self.listeners.lock().unwrap().push(listener);
    }

    /// Never call while holding another lock: a listener may re-enter.
    fn emit(&self, ev: Event) {
        let listeners = self.listeners.lock().unwrap();
        for l in listeners.iter() {
            l(ev.clone());
        }
    }

    /// Ask the app to bring its window forward.
    pub fn request_show(&self) {
        self.emit(Event::Show);
    }

    fn emit_status(&self, id: &str) {
        let status = self.rt.lock().unwrap().get(id).map(|r| r.status.clone());
        if let Some(status) = status {
            self.emit(Event::Status(status));
        }
    }

    // ---- server definitions ----------------------------------------------

    pub fn servers(&self) -> Vec<Server> {
        self.servers.lock().unwrap().clone()
    }

    /// Every project that currently has members, in first-appearance order,
    /// carrying its stored icon if it has one.
    pub fn groups(&self) -> Vec<Group> {
        let stored = self.groups.lock().unwrap().clone();
        let mut out: Vec<Group> = Vec::new();
        for server in self.servers.lock().unwrap().iter() {
            if server.group.is_empty() || out.iter().any(|g| g.name == server.group) {
                continue;
            }
            out.push(Group {
                name: server.group.clone(),
                icon: stored
                    .iter()
                    .find(|g| g.name == server.group)
                    .map(|g| g.icon.clone())
                    .unwrap_or_default(),
            });
        }
        out
    }

    pub fn set_group_icon(&self, name: &str, icon: &str) -> Result<(), String> {
        {
            let mut groups = self.groups.lock().unwrap();
            match groups.iter_mut().find(|g| g.name == name) {
                Some(existing) => existing.icon = icon.to_string(),
                None => groups.push(Group {
                    name: name.to_string(),
                    icon: icon.to_string(),
                }),
            }
        }
        let servers = self.servers.lock().unwrap().clone();
        self.persist(&servers)?;
        self.emit(Event::ServersChanged);
        Ok(())
    }

    /// Save servers alongside whatever project records and tasks we hold.
    fn persist(&self, servers: &[Server]) -> Result<(), String> {
        let groups = self.groups.lock().unwrap().clone();
        let tasks = self.tasks.lock().unwrap().clone();
        store::save(servers, &groups, &tasks).map_err(|e| format!("Couldn't save: {e}"))
    }

    /// Save after a task list changed. Separate from `persist` only so callers
    /// that already hold the server list don't have to clone it twice.
    fn persist_tasks(&self) -> Result<(), String> {
        let servers = self.servers.lock().unwrap().clone();
        self.persist(&servers)
    }

    pub fn get(&self, id: &str) -> Option<Server> {
        self.servers
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn statuses(&self) -> Vec<Status> {
        let rt = self.rt.lock().unwrap();
        self.servers
            .lock()
            .unwrap()
            .iter()
            .map(|s| {
                rt.get(&s.id)
                    .map(|r| r.status.clone())
                    .unwrap_or_else(|| Status::stopped(&s.id))
            })
            .collect()
    }

    /// Insert or update. A blank id means "new server"; we derive one from the
    /// name and make it unique.
    pub fn upsert(&self, mut server: Server) -> Result<Server, String> {
        if server.name.trim().is_empty() {
            return Err("A name is required.".into());
        }
        if server.command.trim().is_empty() {
            return Err("A command is required.".into());
        }
        let dir = paths::expand_tilde(&server.dir);
        if !dir.is_dir() {
            return Err(format!("{} is not a directory.", dir.display()));
        }
        server.dir = dir;

        let mut servers = self.servers.lock().unwrap();
        if server.id.is_empty() {
            let base = slugify(&server.name);
            let mut id = base.clone();
            let mut n = 2;
            while servers.iter().any(|s| s.id == id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            server.id = id;
            server.created_at = now_ms();
            self.rt
                .lock()
                .unwrap()
                .insert(server.id.clone(), Runtime::new(&server.id));
            servers.push(server.clone());
        } else {
            let Some(slot) = servers.iter_mut().find(|s| s.id == server.id) else {
                return Err(format!("No server called {}.", server.id));
            };
            server.created_at = slot.created_at;
            *slot = server.clone();
        }
        let snapshot = servers.clone();
        drop(servers);

        self.persist(&snapshot)?;
        self.emit(Event::ServersChanged);
        Ok(server)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let _ = self.stop(id);
        // A run outlives the server's own process, so deleting the server has
        // to take it down too or the command keeps going with nothing on
        // screen that could stop it.
        if let Some(run) = self.run_of(id) {
            let _ = self.stop_run(&run.run_id);
        }
        let mut servers = self.servers.lock().unwrap();
        servers.retain(|s| s.id != id);
        let snapshot = servers.clone();
        drop(servers);
        self.rt.lock().unwrap().remove(id);
        self.runs.lock().unwrap().remove(id);
        self.tasks.lock().unwrap().remove(id);
        self.logs.lock().unwrap().remove(id);
        self.persist(&snapshot)?;
        self.emit(Event::ServersChanged);
        Ok(())
    }

    // ---- logs -------------------------------------------------------------

    pub fn tail(&self, id: &str, n: usize) -> Vec<LogLine> {
        self.logs
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.tail(n))
            .unwrap_or_default()
    }

    pub fn clear_logs(&self, id: &str) {
        if let Some(ring) = self.logs.lock().unwrap().get_mut(id) {
            ring.clear();
        }
    }

    fn log(&self, id: &str, stream: Stream, text: &str) {
        {
            let mut logs = self.logs.lock().unwrap();
            logs.entry(id.to_string()).or_default().push(stream, text);
        }
        let (lock, cv) = &*self.flush;
        *lock.lock().unwrap() = true;
        cv.notify_one();
    }

    fn spawn_flusher(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let flush = self.flush.clone();
        thread::spawn(move || loop {
            {
                let (lock, cv) = &*flush;
                let mut pending = lock.lock().unwrap();
                // Park indefinitely while nothing is producing output.
                while !*pending {
                    let (guard, _) = cv.wait_timeout(pending, Duration::from_secs(30)).unwrap();
                    pending = guard;
                    if Weak::strong_count(&weak) == 0 {
                        return;
                    }
                }
                *pending = false;
            }
            // Coalesce whatever else arrives in this window into one batch.
            thread::sleep(FLUSH_WINDOW);
            let Some(sup) = weak.upgrade() else { return };
            sup.flush_logs();
        });
    }

    fn flush_logs(&self) {
        let batches: Vec<(String, Vec<LogLine>)> = {
            let mut logs = self.logs.lock().unwrap();
            logs.iter_mut()
                .filter(|(_, r)| r.has_pending())
                .map(|(id, r)| (id.clone(), r.take_pending()))
                .collect()
        };
        for (id, lines) in batches {
            if !lines.is_empty() {
                self.emit(Event::Log { id, lines });
            }
        }
    }

    // ---- lifecycle --------------------------------------------------------

    pub fn start(self: &Arc<Self>, id: &str, origin: Origin) -> Result<(), String> {
        let Some(server) = self.get(id) else {
            return Err(format!("No server called {id}."));
        };
        {
            let rt = self.rt.lock().unwrap();
            if rt.get(id).is_some_and(|r| r.status.state.is_live()) {
                return Err(format!("{} is already running.", server.name));
            }
        }
        self.spawn(&server, origin, 0)
    }

    fn spawn(
        self: &Arc<Self>,
        server: &Server,
        origin: Origin,
        restarts: u32,
    ) -> Result<(), String> {
        let dir = paths::expand_tilde(&server.dir);
        if !dir.is_dir() {
            return Err(format!("{} no longer exists.", dir.display()));
        }

        let (read_fd, watch) = watch_pipe()
            .map_err(|e| format!("Couldn't set up the watchdog for {}: {e}", server.name))?;

        let spawned = child_command(&dir, &server.command, &server.env, read_fd).spawn();
        // The parent has no further use for the read end either way.
        unsafe { libc::close(read_fd) };
        let mut child: Child =
            spawned.map_err(|e| format!("Couldn't start {}: {e}", server.name))?;

        let pid = child.id();
        let pgid = pid as i32;
        let generation = {
            let mut rt = self.rt.lock().unwrap();
            let entry = rt
                .entry(server.id.clone())
                .or_insert_with(|| Runtime::new(&server.id));
            entry.generation += 1;
            entry.stopping = false;
            entry.pgid = Some(pgid);
            entry.watch = Some(watch);
            entry.status = Status {
                id: server.id.clone(),
                state: State::Starting,
                pid: Some(pid),
                port: None,
                started_at: Some(now_ms()),
                exit_code: None,
                origin: Some(origin.clone()),
                restarts,
            };
            if restarts == 0 {
                entry.restart_window_start = now_ms();
            }
            entry.generation
        };

        self.log(
            &server.id,
            Stream::System,
            &format!("$ {} — in {}", server.command, paths::contract_tilde(&dir)),
        );
        self.emit_status(&server.id);

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Some(out) = stdout {
            self.pipe(&server.id, out, Stream::Stdout, generation);
        }
        if let Some(err) = stderr {
            self.pipe(&server.id, err, Stream::Stderr, generation);
        }

        self.probe_port(&server.id, pgid, generation);
        self.monitor(server.clone(), child, generation, origin);
        Ok(())
    }

    /// Read one pipe into the ring buffer, watching for a port announcement.
    fn pipe<R: std::io::Read + Send + 'static>(
        self: &Arc<Self>,
        id: &str,
        reader: R,
        stream: Stream,
        generation: u64,
    ) {
        let sup = self.clone();
        let id = id.to_string();
        thread::spawn(move || {
            let mut buf = BufReader::new(reader);
            let mut line = Vec::new();
            loop {
                line.clear();
                match buf.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&line).into_owned();

                // Promote Starting -> Running on the first sign of life, and
                // grab the port if the server announced one.
                let found = ports::scan_line(&text);
                let mut changed = false;
                {
                    let mut rt = sup.rt.lock().unwrap();
                    if let Some(r) = rt.get_mut(&id) {
                        if r.generation != generation {
                            return;
                        }
                        if r.status.state == State::Starting {
                            r.status.state = State::Running;
                            changed = true;
                        }
                        if r.status.port.is_none() {
                            if let Some(p) = found {
                                r.status.port = Some(p);
                                changed = true;
                            }
                        }
                    }
                }
                sup.log(&id, stream, &text);
                if changed {
                    sup.emit_status(&id);
                }
            }
        });
    }

    /// Ask lsof what we're listening on, but only for a bounded window after
    /// start, and stop the moment we know.
    fn probe_port(self: &Arc<Self>, id: &str, pgid: i32, generation: u64) {
        let sup = self.clone();
        let id = id.to_string();
        thread::spawn(move || {
            for _ in 0..PROBE_ATTEMPTS {
                thread::sleep(PROBE_INTERVAL);
                {
                    let rt = sup.rt.lock().unwrap();
                    match rt.get(&id) {
                        Some(r) if r.generation == generation => {
                            if r.status.port.is_some() || !r.status.state.is_live() {
                                return;
                            }
                        }
                        _ => return,
                    }
                }
                if !group_alive(pgid) {
                    return;
                }
                if let Some(port) = ports::detect(pgid) {
                    let mut apply = false;
                    {
                        let mut rt = sup.rt.lock().unwrap();
                        if let Some(r) = rt.get_mut(&id) {
                            if r.generation == generation && r.status.port.is_none() {
                                r.status.port = Some(port);
                                if r.status.state == State::Starting {
                                    r.status.state = State::Running;
                                }
                                apply = true;
                            }
                        }
                    }
                    if apply {
                        sup.emit_status(&id);
                    }
                    return;
                }
            }
        });
    }

    /// Reap the child and decide whether to bring it back.
    fn monitor(
        self: &Arc<Self>,
        server: Server,
        mut child: Child,
        generation: u64,
        origin: Origin,
    ) {
        let sup = self.clone();
        thread::spawn(move || {
            let status = child.wait();
            let code = status.ok().and_then(|s| s.code());

            let mut should_restart = false;
            let mut restarts = 0;
            {
                let mut rt = sup.rt.lock().unwrap();
                let Some(r) = rt.get_mut(&server.id) else {
                    return;
                };
                if r.generation != generation {
                    return; // superseded by a newer run
                }
                let deliberate = r.stopping;
                r.pgid = None;
                r.watch = None; // the run is over; let the pipe go
                r.status.pid = None;
                r.status.port = None;
                r.status.exit_code = code;

                if deliberate {
                    r.status.state = State::Stopped;
                    r.status.started_at = None;
                    r.status.origin = None;
                    r.status.restarts = 0;
                } else if code == Some(0) {
                    r.status.state = State::Stopped;
                    r.status.started_at = None;
                    r.status.origin = None;
                } else {
                    r.status.state = State::Crashed;
                    if server.auto_restart {
                        let now = now_ms();
                        if now.saturating_sub(r.restart_window_start) > RESTART_WINDOW_MS {
                            r.restart_window_start = now;
                            r.status.restarts = 0;
                        }
                        if r.status.restarts < RESTART_LIMIT {
                            r.status.restarts += 1;
                            restarts = r.status.restarts;
                            should_restart = true;
                        }
                    }
                }
            }

            match code {
                Some(0) => sup.log(&server.id, Stream::System, "Finished cleanly."),
                Some(c) => sup.log(
                    &server.id,
                    Stream::System,
                    &format!("Exited with code {c}."),
                ),
                None => sup.log(&server.id, Stream::System, "Stopped."),
            }
            sup.emit_status(&server.id);

            if should_restart {
                let delay = Duration::from_millis(500 * restarts.min(6) as u64);
                sup.log(
                    &server.id,
                    Stream::System,
                    &format!(
                        "Restarting in {:.1}s (attempt {restarts} of {RESTART_LIMIT}).",
                        delay.as_secs_f32()
                    ),
                );
                thread::sleep(delay);
                if let Err(e) = sup.spawn(&server, origin, restarts) {
                    sup.log(&server.id, Stream::System, &e);
                }
            } else if !server.auto_restart {
                // nothing further to do
            } else if code.is_some() && code != Some(0) {
                sup.log(
                    &server.id,
                    Stream::System,
                    "Gave up restarting — it's failing too fast. Fix it and start again.",
                );
            }
        });
    }

    // ---- tasks ------------------------------------------------------------

    pub fn tasks(&self, id: &str) -> Vec<Task> {
        self.tasks
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    fn emit_tasks(&self, id: &str) {
        self.emit(Event::Tasks {
            id: id.to_string(),
            tasks: self.tasks(id),
        });
    }

    /// Add a command to a server's list, or return the entry it already had.
    /// Adding is idempotent because the id is derived from the command: typing
    /// a command you already keep should run that one, not shadow it.
    pub fn add_task(&self, id: &str, command: &str) -> Result<Task, String> {
        if self.get(id).is_none() {
            return Err(format!("No server called {id}."));
        }
        let task = Task::new(command);
        if task.command.is_empty() {
            return Err("A command is required.".into());
        }
        let added = {
            let mut all = self.tasks.lock().unwrap();
            let list = all.entry(id.to_string()).or_default();
            // Matched on the command, not the id: two commands can slug to
            // the same id, and it is the command the user typed that decides
            // whether this is one they already keep.
            match list.iter().find(|t| t.command == task.command) {
                Some(existing) => existing.clone(),
                None => {
                    let task = task.with_unique_id(list);
                    // Newest first, matching the order the menu reads in.
                    list.insert(0, task.clone());
                    task
                }
            }
        };
        self.persist_tasks()?;
        self.emit_tasks(id);
        Ok(added)
    }

    /// Forget a task. Never touches a run that is using it — the process keeps
    /// going and the output box keeps showing it, because killing something
    /// mid-migration to tidy a list would be the worse surprise.
    pub fn remove_task(&self, id: &str, task_id: &str) -> Result<(), String> {
        {
            let mut all = self.tasks.lock().unwrap();
            let Some(list) = all.get_mut(id) else {
                return Err(format!("No server called {id}."));
            };
            let before = list.len();
            list.retain(|t| t.id != task_id);
            if list.len() == before {
                return Err(format!("No task called {task_id} on {id}."));
            }
        }
        self.persist_tasks()?;
        self.emit_tasks(id);
        Ok(())
    }

    /// The current or most recent run for a server.
    pub fn run_of(&self, id: &str) -> Option<Run> {
        self.runs.lock().unwrap().get(id).map(|a| a.run.clone())
    }

    pub fn find_run(&self, run_id: &str) -> Option<Run> {
        self.runs
            .lock()
            .unwrap()
            .values()
            .find(|a| a.run.run_id == run_id)
            .map(|a| a.run.clone())
    }

    /// Just this run's lines, pulled back out of the server's stream. The app
    /// never needs this — it shows the whole window — but an agent that
    /// started a run wants what the run printed, not the server's traffic
    /// around it.
    pub fn run_tail(&self, id: &str, n: usize) -> Vec<LogLine> {
        let Some(from) = self.runs.lock().unwrap().get(id).map(|a| a.from_seq) else {
            return Vec::new();
        };
        self.logs
            .lock()
            .unwrap()
            .get(id)
            .map(|ring| ring.since(from, n))
            .unwrap_or_default()
    }

    /// Run a command that is not saved yet, adding it to the list. This is
    /// exactly what the footer field does, and what an agent calls — so an
    /// agent's command shows up in the user's list afterwards rather than
    /// building a second, invisible history.
    pub fn run_command(
        self: &Arc<Self>,
        id: &str,
        command: &str,
        origin: Origin,
    ) -> Result<Run, String> {
        let task = self.add_task(id, command)?;
        self.run_task(id, &task.id, origin)
    }

    pub fn run_task(
        self: &Arc<Self>,
        id: &str,
        task_id: &str,
        origin: Origin,
    ) -> Result<Run, String> {
        let Some(server) = self.get(id) else {
            return Err(format!("No server called {id}."));
        };
        let Some(task) = self.tasks(id).into_iter().find(|t| t.id == task_id) else {
            return Err(format!("No task called {task_id} on {id}."));
        };
        // Refused rather than queued, and the active run is named: an agent
        // that did not know a migration was already going should be told which
        // one, not left to wait on something it cannot see.
        if let Some(active) = self.run_of(id).filter(Run::is_live) {
            return Err(format!(
                "{} is already running `{}` (run {}). Only one task runs at a time per server.",
                server.name, active.command, active.run_id
            ));
        }

        let dir = paths::expand_tilde(&server.dir);
        if !dir.is_dir() {
            return Err(format!("{} no longer exists.", dir.display()));
        }

        let (read_fd, watch) =
            watch_pipe().map_err(|e| format!("Couldn't set up the watchdog: {e}"))?;
        let spawned = child_command(&dir, &task.command, &server.env, read_fd).spawn();
        unsafe { libc::close(read_fd) };
        let mut child: Child =
            spawned.map_err(|e| format!("Couldn't run {}: {e}", task.command))?;

        let pgid = child.id() as i32;
        let started = now_ms();
        let run = Run {
            run_id: format!("run-{}", self.next_run.fetch_add(1, Ordering::SeqCst)),
            server_id: id.to_string(),
            task_id: task.id.clone(),
            command: task.command.clone(),
            started_at: started,
            ended_at: None,
            exit_code: None,
            origin: Some(origin),
            last_output_at: started,
        };

        // Where this run's lines begin in the server's stream. Read before the
        // first line is written, so the banner below is part of the run.
        let from_seq = self
            .logs
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_default()
            .next_seq();

        let generation = {
            let mut runs = self.runs.lock().unwrap();
            let slot = runs.entry(id.to_string()).or_insert(Active {
                run: run.clone(),
                pgid: None,
                watch: None,
                stopping: false,
                generation: 0,
                from_seq,
            });
            slot.generation += 1;
            slot.run = run.clone();
            slot.pgid = Some(pgid);
            slot.watch = Some(watch);
            slot.stopping = false;
            slot.from_seq = from_seq;
            slot.generation
        };

        self.log(id, Stream::System, &format!("$ {}", task.command));
        self.emit(Event::Run(run.clone()));

        if let Some(out) = child.stdout.take() {
            self.pipe_run(id, out, Stream::Stdout, generation);
        }
        if let Some(err) = child.stderr.take() {
            self.pipe_run(id, err, Stream::Stderr, generation);
        }
        self.monitor_run(id.to_string(), child, generation);
        Ok(run)
    }

    /// Read one of a run's pipes into the server's own log. Simpler than the
    /// server's: there is no state to promote and no port to look for, only
    /// the clock on last output.
    fn pipe_run<R: std::io::Read + Send + 'static>(
        self: &Arc<Self>,
        id: &str,
        reader: R,
        stream: Stream,
        generation: u64,
    ) {
        let sup = self.clone();
        let id = id.to_string();
        thread::spawn(move || {
            let mut buf = BufReader::new(reader);
            let mut line = Vec::new();
            loop {
                line.clear();
                match buf.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                {
                    let mut runs = sup.runs.lock().unwrap();
                    match runs.get_mut(&id) {
                        Some(a) if a.generation == generation => a.run.last_output_at = now_ms(),
                        _ => return,
                    }
                }
                sup.log(&id, stream, &String::from_utf8_lossy(&line));
            }
        });
    }

    /// Reap a run and record how it ended on the task it came from.
    fn monitor_run(self: &Arc<Self>, id: String, mut child: Child, generation: u64) {
        let sup = self.clone();
        thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code());

            let (finished, task_id) = {
                let mut runs = sup.runs.lock().unwrap();
                let Some(a) = runs.get_mut(&id) else { return };
                if a.generation != generation {
                    return; // superseded by a newer run
                }
                a.pgid = None;
                a.watch = None; // the run is over; let the pipe go
                a.run.ended_at = Some(now_ms());
                a.run.exit_code = code;
                (a.run.clone(), a.run.task_id.clone())
            };

            {
                let mut all = sup.tasks.lock().unwrap();
                if let Some(task) = all
                    .get_mut(&id)
                    .and_then(|list| list.iter_mut().find(|t| t.id == task_id))
                {
                    task.last_exit = code;
                    task.last_run_at = finished.ended_at;
                }
            }

            let secs = finished
                .ended_at
                .unwrap_or_default()
                .saturating_sub(finished.started_at) as f32
                / 1000.0;
            let note = match code {
                Some(c) => format!("exited {c} after {secs:.1}s"),
                None => format!("stopped after {secs:.1}s"),
            };
            sup.log(&id, Stream::System, &note);

            let _ = sup.persist_tasks();
            sup.emit(Event::Run(finished));
            sup.emit_tasks(&id);
        });
    }

    /// Stop a run, including one the user started from the app. Same escalation
    /// as a server: SIGTERM to the group, SIGKILL if it will not go.
    pub fn stop_run(&self, run_id: &str) -> Result<(), String> {
        let pgid = {
            let mut runs = self.runs.lock().unwrap();
            let Some(a) = runs.values_mut().find(|a| a.run.run_id == run_id) else {
                return Err(format!("No run called {run_id}."));
            };
            if !a.run.is_live() {
                return Ok(()); // already finished; nothing to do
            }
            a.stopping = true;
            a.pgid
        };
        let Some(pgid) = pgid else { return Ok(()) };

        kill_group(pgid, libc::SIGTERM);
        thread::spawn(move || {
            thread::sleep(TERM_GRACE);
            if group_alive(pgid) {
                kill_group(pgid, libc::SIGKILL);
            }
        });
        Ok(())
    }

    // ---- strays: ports held by processes that are not ours -----------------

    /// Every process group Cucina answers for — its own, its servers', and any
    /// task run in flight. Everything else listening is a stray.
    fn own_groups(&self) -> Vec<i32> {
        // Cucina's own group covers the app, the CLI talking to it, and any
        // helper either of them spawned.
        let mut groups = vec![unsafe { libc::getpgrp() }];
        groups.extend(self.rt.lock().unwrap().values().filter_map(|r| r.pgid));
        groups.extend(self.runs.lock().unwrap().values().filter_map(|a| a.pgid));
        groups.sort_unstable();
        groups.dedup();
        groups
    }

    pub fn strays(&self) -> Result<Vec<crate::strays::Stray>, String> {
        crate::strays::scan(&self.own_groups())
    }

    /// Stop something Cucina does not own. Refuses anything that turns out to
    /// be one of ours, so a stale list cannot be used to kill a live server
    /// behind the supervisor's back — that is what the Stop button is for.
    pub fn stop_stray(&self, pid: u32) -> Result<(), String> {
        if self.strays()?.iter().all(|s| s.pid != pid) {
            return Err(format!(
                "pid {pid} is not a stray any more. Scan again to see what is out there."
            ));
        }
        crate::strays::stop(pid)
    }

    pub fn stop(&self, id: &str) -> Result<(), String> {
        let pgid = {
            let mut rt = self.rt.lock().unwrap();
            let Some(r) = rt.get_mut(id) else {
                return Err(format!("No server called {id}."));
            };
            if !r.status.state.is_live() {
                return Ok(()); // already stopped; nothing to do
            }
            r.stopping = true;
            r.pgid
        };
        let Some(pgid) = pgid else { return Ok(()) };

        self.log(id, Stream::System, "Stopping…");
        kill_group(pgid, libc::SIGTERM);

        // Escalate if it doesn't go quietly.
        thread::spawn(move || {
            thread::sleep(TERM_GRACE);
            if group_alive(pgid) {
                kill_group(pgid, libc::SIGKILL);
            }
        });
        Ok(())
    }

    /// Point a server at a different directory — in practice, another git
    /// worktree. A server left running from a directory you have navigated
    /// away from is a trap, so a live one is stopped and started again in its
    /// new home. One that was already idle stays idle.
    pub fn switch_dir(self: &Arc<Self>, id: &str, dir: std::path::PathBuf) -> Result<(), String> {
        let dir = paths::expand_tilde(&dir);
        if !dir.is_dir() {
            return Err(format!("{} is not a directory.", dir.display()));
        }

        let was_live = {
            let rt = self.rt.lock().unwrap();
            rt.get(id).is_some_and(|r| r.status.state.is_live())
        };
        if was_live {
            self.stop(id)?;
            self.await_settled(id);
        }

        {
            let mut servers = self.servers.lock().unwrap();
            let Some(server) = servers.iter_mut().find(|s| s.id == id) else {
                return Err(format!("No server called {id}."));
            };
            if server.dir == dir {
                return Ok(());
            }
            server.dir = dir;
            let snapshot = servers.clone();
            drop(servers);
            self.persist(&snapshot)?;
        }
        self.emit(Event::ServersChanged);

        if was_live {
            self.start(id, Origin::User)?;
        }
        Ok(())
    }

    /// Wait for a stop to actually finish before reusing the slot.
    fn await_settled(&self, id: &str) {
        for _ in 0..80 {
            thread::sleep(Duration::from_millis(100));
            let done = {
                let rt = self.rt.lock().unwrap();
                rt.get(id).is_some_and(|r| !r.status.state.is_live())
            };
            if done {
                return;
            }
        }
    }

    pub fn restart(self: &Arc<Self>, id: &str, origin: Origin) -> Result<(), String> {
        let was_live = {
            let rt = self.rt.lock().unwrap();
            rt.get(id).is_some_and(|r| r.status.state.is_live())
        };
        if was_live {
            self.stop(id)?;
            self.await_settled(id);
        }
        self.start(id, origin)
    }

    /// Start everything marked auto-start. Called once, at app launch.
    pub fn start_auto(self: &Arc<Self>) {
        for s in self.servers() {
            if s.auto_start {
                let _ = self.start(&s.id, Origin::User);
            }
        }
    }

    /// Kill every running process. Called on quit so nothing is orphaned.
    pub fn shutdown(&self) {
        let mut groups: Vec<i32> = {
            let mut rt = self.rt.lock().unwrap();
            rt.values_mut()
                .filter_map(|r| {
                    r.stopping = true;
                    r.pgid.take()
                })
                .collect()
        };
        // Task runs die with the app for the same reason servers do: a
        // migration nobody can see is worse than one that was interrupted.
        groups.extend({
            let mut runs = self.runs.lock().unwrap();
            runs.values_mut()
                .filter_map(|a| {
                    a.stopping = true;
                    a.pgid.take()
                })
                .collect::<Vec<i32>>()
        });
        for pgid in &groups {
            kill_group(*pgid, libc::SIGTERM);
        }
        if groups.is_empty() {
            return;
        }
        // Brief grace period, then insist.
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(100));
            if !groups.iter().any(|g| group_alive(*g)) {
                return;
            }
        }
        for pgid in &groups {
            if group_alive(*pgid) {
                kill_group(*pgid, libc::SIGKILL);
            }
        }
    }
}
