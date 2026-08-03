use std::process::{Command, Stdio};

/// Ports we'd never want to surface as "the server's port".
fn plausible(port: u16) -> bool {
    port > 1024 && port < 60_000
}

/// Fast path: most dev servers announce themselves on stdout. Catching the
/// port here means we usually never have to shell out to lsof at all.
pub fn scan_line(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    let mut best: Option<u16> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            // Require a delimiter after the digits so we don't read "1.2.3:45x"
            let terminated = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
            if end > start && end - start <= 5 && terminated {
                if let Ok(port) = text[start..end].parse::<u16>() {
                    // Only trust a bare `:3000` when it looks like a host:port
                    // pair, i.e. preceded by a hostname/ip character or nothing.
                    if plausible(port) && best.is_none() {
                        best = Some(port);
                    }
                }
            }
            i = end.max(start);
        } else {
            i += 1;
        }
    }
    best
}

/// Every pid in the process group, so we catch ports opened by a child of the
/// shell we spawned (`npm run dev` -> `vite`).
fn pids_in_group(pgid: i32) -> Vec<u32> {
    let Ok(out) = Command::new("/bin/ps")
        .args(["-Ao", "pid=,pgid="])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid = it.next()?.parse::<u32>().ok()?;
            let gid = it.next()?.parse::<i32>().ok()?;
            (gid == pgid).then_some(pid)
        })
        .collect()
}

/// Ask the kernel which TCP port this process group is listening on.
/// Only called during a bounded window after start — never on an idle loop.
pub fn detect(pgid: i32) -> Option<u16> {
    let pids = pids_in_group(pgid);
    if pids.is_empty() {
        return None;
    }
    let list = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let out = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", &list, "-F", "n"])
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&out.stdout);
    let mut found: Vec<u16> = text
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .filter_map(|addr| addr.rsplit(':').next())
        .filter_map(|p| p.parse::<u16>().ok())
        .filter(|p| plausible(*p))
        .collect();

    found.sort_unstable();
    found.dedup();
    found.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ports_out_of_dev_server_banners() {
        assert_eq!(scan_line("  ➜  Local:   http://localhost:5173/"), Some(5173));
        assert_eq!(scan_line("Listening on http://127.0.0.1:3000"), Some(3000));
        assert_eq!(scan_line("server started on :8080"), Some(8080));
    }

    #[test]
    fn ignores_things_that_are_not_ports() {
        assert_eq!(scan_line("compiled in 1.2s"), None);
        assert_eq!(scan_line("12:34:56 ready"), None); // timestamps are too low
        assert_eq!(scan_line("error at file.ts:10:5"), None);
    }
}
