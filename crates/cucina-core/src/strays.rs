//! Strays: processes holding a TCP port that Cucina does not own.
//!
//! A stray is lost, not hostile — an agent's dev server whose shell went away,
//! a `npm run dev` from a terminal tab you closed a week ago. The scan is three
//! bounded subprocess calls, run when you ask for it and when the window comes
//! forward. Never on a timer.

use std::collections::HashMap;
use std::process::{Command, Stdio};

/// One process holding a port. Everything here is observed, not stored — a new
/// scan replaces the lot.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stray {
    pub port: u16,
    pub pid: u32,
    /// The full command line as the kernel has it — a runtime artefact, with
    /// resolved shims and absolute interpreter paths. Not a start command.
    pub command: String,
    /// `None` when the working directory could not be read, which is normal
    /// for a process started somewhere that has since been deleted.
    #[serde(default)]
    pub dir: Option<String>,
    /// Seconds since it started.
    pub age: u64,
    /// The terminal behind it, if there is one: `ZSH S004`. A process with an
    /// owner is somebody's live work and should not read like an escapee.
    #[serde(default)]
    pub owner: Option<String>,
}

impl Stray {
    /// Nothing is behind it. The whole reason the feature exists.
    pub fn is_orphan(&self) -> bool {
        self.owner.is_none()
    }
}

/// One row of `ps`, keyed by pid.
#[derive(Clone, Debug)]
struct Proc {
    ppid: u32,
    pgid: i32,
    /// `??` when the process has no controlling terminal.
    tty: String,
    age: u64,
    /// The executable on its own. Read separately from `command` because both
    /// can contain spaces, and only one space-bearing field can be last on a
    /// `ps` line — `Application Support` is enough to break a naive split.
    exe: String,
    command: String,
}

/// Anything the user did not start themselves. Installed applications and the
/// system's own agents hold ports all day — Control Centre on 5000, Docker on
/// 5432, Tailscale on a high port — and listing them would bury the one `vite`
/// that is actually loose.
fn is_dev_process(exe: &str) -> bool {
    if exe.contains(".app/Contents/") {
        return false;
    }
    !["/System/", "/usr/libexec/", "/Library/", "/sbin/"]
        .iter()
        .any(|prefix| exe.starts_with(prefix))
}

/// The same bounds `ports::detect` uses, for the same reason: a port outside
/// this range is not one you would have opened in a browser.
fn plausible(port: u16) -> bool {
    port > 1024 && port < 60_000
}

/// `[[dd-]hh:]mm:ss` — what BSD `ps` prints for `etime`.
fn parse_etime(text: &str) -> u64 {
    let (days, clock) = match text.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().unwrap_or(0), rest),
        None => (0, text),
    };
    let parts: Vec<u64> = clock
        .split(':')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    let secs = match parts.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        [s] => *s,
        _ => 0,
    };
    days * 86_400 + secs
}

/// `zsh` on `s004` becomes `ZSH S004`. The shell alone would not say which
/// window, and the tty alone would not say what is in it.
fn owner_label(parent: Option<&Proc>, tty: &str) -> Option<String> {
    if tty.is_empty() || tty == "??" || tty == "?" {
        return None;
    }
    let name = parent
        .map(|p| p.exe.as_str())
        .map(|exe| exe.rsplit('/').next().unwrap_or(exe))
        // A login shell is argv[0] `-zsh`, which is not a name anybody types.
        .map(|name| name.trim_start_matches('-'))
        .filter(|name| !name.is_empty())
        .unwrap_or("shell");
    Some(format!("{} {}", name.to_uppercase(), tty.to_uppercase()))
}

/// Everything after the first `n` whitespace-separated fields, spacing intact.
/// Searching for the sixth token instead would find its first occurrence
/// anywhere in the line, and a pid is a very ordinary thing to appear twice.
fn rest_after(line: &str, n: usize) -> &str {
    let mut at = 0;
    for _ in 0..n {
        let start = at + line[at..].len() - line[at..].trim_start().len();
        let end = line[start..]
            .find(char::is_whitespace)
            .map(|i| start + i)
            .unwrap_or(line.len());
        at = end;
    }
    line[at..].trim_start()
}

fn read_procs() -> Result<HashMap<u32, Proc>, String> {
    // `command` holds spaces, so it has to be last and is taken as the rest of
    // the line. `-ww` stops ps truncating it at the terminal width.
    let out = Command::new("/bin/ps")
        .args(["-Awwo", "pid=,ppid=,pgid=,tty=,etime=,command="])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ps: {e}"))?;
    if !out.status.success() {
        return Err(trimmed_stderr("ps", &out.stderr, out.status.code()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut table = HashMap::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(pid), Some(ppid), Some(pgid), Some(tty), Some(etime)) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        let command = rest_after(line, 5).to_string();
        table.insert(
            pid,
            Proc {
                ppid: ppid.parse().unwrap_or(0),
                pgid: pgid.parse().unwrap_or(0),
                tty: tty.to_string(),
                age: parse_etime(etime),
                exe: String::new(),
                command,
            },
        );
    }
    for (pid, exe) in read_exes()? {
        if let Some(proc) = table.get_mut(&pid) {
            proc.exe = exe;
        }
    }
    Ok(table)
}

/// The executable path per pid, with no arguments after it.
fn read_exes() -> Result<Vec<(u32, String)>, String> {
    let out = Command::new("/bin/ps")
        .args(["-Awwo", "pid=,comm="])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ps: {e}"))?;
    if !out.status.success() {
        return Err(trimmed_stderr("ps", &out.stderr, out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let pid = line.split_whitespace().next()?.parse::<u32>().ok()?;
            Some((pid, rest_after(line, 1).to_string()))
        })
        .collect())
}

/// pid -> every port it is listening on.
fn read_listeners(uid: u32) -> Result<Vec<(u32, u16)>, String> {
    // `-a` matters: without it lsof ORs its selectors and the uid filter would
    // widen the result rather than narrow it.
    let out = Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-iTCP",
            "-sTCP:LISTEN",
            "-a",
            "-u",
            &uid.to_string(),
            "-F",
            "pn",
        ])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    // lsof exits 1 when it merely found nothing, so the exit code alone cannot
    // separate "nothing is listening" from a real failure.
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() && text.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.trim().is_empty() {
            return Err(trimmed_stderr("lsof", &out.stderr, out.status.code()));
        }
    }
    Ok(parse_listeners(&text))
}

/// lsof's field output: a `p<pid>` line, then an `n<address>` line per socket.
fn parse_listeners(text: &str) -> Vec<(u32, u16)> {
    let mut found: Vec<(u32, u16)> = Vec::new();
    let mut pid = 0u32;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            pid = rest.parse().unwrap_or(0);
        } else if let Some(addr) = line.strip_prefix('n') {
            let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) else {
                continue;
            };
            if pid != 0 && plausible(port) && !found.contains(&(pid, port)) {
                found.push((pid, port));
            }
        }
    }
    found
}

/// The working directory of each pid, in one call rather than one per process.
fn read_dirs(pids: &[u32]) -> HashMap<u32, String> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let list = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let Ok(out) = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &list, "-d", "cwd", "-Fn"])
        .stderr(Stdio::null())
        .output()
    else {
        return HashMap::new();
    };
    parse_dirs(&String::from_utf8_lossy(&out.stdout))
}

fn parse_dirs(text: &str) -> HashMap<u32, String> {
    let mut dirs = HashMap::new();
    let mut pid = 0u32;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            pid = rest.parse().unwrap_or(0);
        } else if let Some(dir) = line.strip_prefix('n') {
            // `/` is what a process gets when it was started with no meaningful
            // directory, and adopting into `/` would be worse than not offering.
            if pid != 0 && dir != "/" && !dir.is_empty() {
                dirs.entry(pid).or_insert_with(|| dir.to_string());
            }
        }
    }
    dirs
}

fn trimmed_stderr(tool: &str, bytes: &[u8], code: Option<i32>) -> String {
    let text = String::from_utf8_lossy(bytes);
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    match code {
        Some(c) if first.is_empty() => format!("{tool}: exited {c}"),
        Some(c) => format!("{tool}: exited {c} — {first}"),
        None => format!("{tool}: {first}"),
    }
}

/// Everything listening that is not one of `ours`, which is Cucina's own
/// process group plus the group of every server and task run it is supervising.
pub fn scan(ours: &[i32]) -> Result<Vec<Stray>, String> {
    let uid = unsafe { libc::getuid() };
    let listeners = read_listeners(uid)?;
    let procs = read_procs()?;

    let mut keep: Vec<(u32, u16)> = Vec::new();
    for (pid, port) in listeners {
        let Some(proc) = procs.get(&pid) else {
            continue;
        };
        if ours.contains(&proc.pgid) || !is_dev_process(&proc.exe) {
            continue;
        }
        keep.push((pid, port));
    }

    let pids: Vec<u32> = {
        let mut seen: Vec<u32> = keep.iter().map(|(pid, _)| *pid).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    let dirs = read_dirs(&pids);

    let mut strays: Vec<Stray> = keep
        .into_iter()
        .filter_map(|(pid, port)| {
            let proc = procs.get(&pid)?;
            Some(Stray {
                port,
                pid,
                command: proc.command.clone(),
                dir: dirs.get(&pid).cloned(),
                age: proc.age,
                owner: owner_label(procs.get(&proc.ppid), &proc.tty),
            })
        })
        .collect();

    // Orphans first, then oldest — the ones nobody is watching, and the ones
    // that have been loose longest, are the ones worth clearing.
    strays.sort_by(|a, b| {
        b.is_orphan()
            .cmp(&a.is_orphan())
            .then(b.age.cmp(&a.age))
            .then(a.port.cmp(&b.port))
    });
    Ok(strays)
}

/// Ask a stray to stop, then insist.
///
/// The signal goes to the process group only when the process leads its own —
/// which is what job control gives anything started from a shell prompt. A
/// process sharing its parent's group is a child of something bigger, and
/// signalling that group could take the user's shell down with it.
pub fn stop(pid: u32) -> Result<(), String> {
    let procs = read_procs()?;
    let Some(proc) = procs.get(&pid) else {
        return Err(format!("Nothing is running as pid {pid} any more."));
    };
    let leads_group = proc.pgid == pid as i32;
    let signal = |sig: i32| unsafe {
        if leads_group {
            libc::killpg(proc.pgid, sig);
        } else {
            libc::kill(pid as i32, sig);
        }
    };
    let alive = || unsafe { libc::kill(pid as i32, 0) == 0 };

    signal(libc::SIGTERM);
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !alive() {
            return Ok(());
        }
    }
    signal(libc::SIGKILL);
    std::thread::sleep(std::time::Duration::from_millis(200));
    if alive() {
        return Err(format!("pid {pid} did not stop."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_dev_processes_and_drops_the_machine_furniture() {
        assert!(is_dev_process("/opt/homebrew/bin/node"));
        assert!(is_dev_process("/Users/me/.rbenv/shims/ruby"));
        assert!(is_dev_process("python3"));
        // A dev server run by an editor's bundled runtime is still yours.
        assert!(is_dev_process(
            "/Users/me/Library/Application Support/Zed/node/node-v24/bin/node"
        ));

        assert!(!is_dev_process("/usr/libexec/rapportd"));
        assert!(!is_dev_process(
            "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter"
        ));
        assert!(!is_dev_process(
            "/Applications/Docker.app/Contents/MacOS/com.docker.backend"
        ));
        // An app bundle under the user's own Library, path spaces and all, is
        // still an application.
        assert!(!is_dev_process(
            "/Users/me/Library/Application Support/Figma/FigmaAgent.app/Contents/MacOS/figma_agent"
        ));
    }

    #[test]
    fn reads_the_clock_ps_prints() {
        assert_eq!(parse_etime("05:31"), 331);
        assert_eq!(parse_etime("02:40:20"), 9620);
        assert_eq!(parse_etime("28-04:15:42"), 28 * 86_400 + 15_342);
        assert_eq!(parse_etime("nonsense"), 0);
    }

    #[test]
    fn takes_the_command_as_the_rest_of_the_ps_line() {
        let line = "  455 47991 34770 ??          02:40:20 /usr/bin/node --inspect 455 server.js";
        assert_eq!(
            rest_after(line, 5),
            "/usr/bin/node --inspect 455 server.js",
            "the pid appearing again inside the command must not truncate it"
        );
        assert_eq!(rest_after("1 2 3", 5), "");
    }

    #[test]
    fn names_the_terminal_behind_a_process() {
        let shell = Proc {
            ppid: 1,
            pgid: 900,
            tty: "s004".into(),
            age: 10,
            exe: "-zsh".into(),
            command: "-zsh".into(),
        };
        assert_eq!(owner_label(Some(&shell), "s004"), Some("ZSH S004".into()));
        // No terminal is the whole point: that one is an orphan.
        assert_eq!(owner_label(Some(&shell), "??"), None);
        // A terminal whose parent has already gone is still a terminal.
        assert_eq!(owner_label(None, "s001"), Some("SHELL S001".into()));
    }

    #[test]
    fn pairs_every_socket_with_the_process_holding_it() {
        let out = "p455\nn*:5173\nn127.0.0.1:5173\np456\nn*:3000\nn*:80\n";
        assert_eq!(
            parse_listeners(out),
            vec![(455, 5173), (456, 3000)],
            "duplicates collapse and port 80 is out of range"
        );
    }

    #[test]
    fn takes_the_first_directory_lsof_gives_for_a_process() {
        let out = "p455\nn/Users/me/code/web\np456\nn/\n";
        let dirs = parse_dirs(out);
        assert_eq!(
            dirs.get(&455).map(String::as_str),
            Some("/Users/me/code/web")
        );
        assert_eq!(
            dirs.get(&456),
            None,
            "root is not a directory worth adopting"
        );
    }

    #[test]
    fn an_orphan_is_one_with_nothing_behind_it() {
        let loose = Stray {
            port: 5173,
            pid: 1,
            command: "node vite".into(),
            dir: None,
            age: 60,
            owner: None,
        };
        assert!(loose.is_orphan());
        assert!(!Stray {
            owner: Some("ZSH S004".into()),
            ..loose
        }
        .is_orphan());
    }
}
