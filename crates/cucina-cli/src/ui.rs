use cucina_core::model::{Origin, State};
use cucina_core::paths;
use cucina_core::proto::ServerView;

pub struct Paint {
    on: bool,
}

impl Paint {
    pub fn detect() -> Paint {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Paint {
            on: !no_color && is_tty(1),
        }
    }

    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub fn terracotta(&self, s: &str) -> String {
        // 208 is the closest 256-colour cell to Cucina's terracotta.
        self.wrap("38;5;208", s)
    }
}

pub fn is_tty(fd: i32) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

pub fn duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d", secs / 86_400)
    }
}

fn mark(state: State, paint: &Paint) -> String {
    match state {
        State::Running => paint.green("●"),
        State::Starting => paint.terracotta("◐"),
        State::Crashed => paint.red("✕"),
        State::Stopped => paint.dim("○"),
    }
}

fn word(state: State) -> &'static str {
    match state {
        State::Running => "running",
        State::Starting => "starting",
        State::Crashed => "crashed",
        State::Stopped => "stopped",
    }
}

/// The table `cucina` prints with no arguments. Columns are padded to the
/// widest value so it stays readable with long names.
pub fn table(views: &[ServerView], paint: &Paint) -> String {
    if views.is_empty() {
        return format!(
            "{}\n{}\n",
            paint.dim("Nothing in the kitchen yet."),
            paint.dim("  cucina add --name api --dir . --command \"npm run dev\"")
        );
    }

    let now = cucina_core::model::now_ms();
    let name_w = views
        .iter()
        .map(|v| v.server.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let state_w = views
        .iter()
        .map(|v| word(v.status.state).len())
        .max()
        .unwrap_or(7);

    let mut out = String::new();
    for v in views {
        let s = &v.status;
        let port = s.port.map(|p| format!(":{p}")).unwrap_or_default();
        let uptime = s
            .started_at
            .filter(|_| s.state.is_live())
            .map(|t| duration(now.saturating_sub(t)))
            .unwrap_or_default();
        let detail = match (s.state, s.exit_code) {
            (State::Crashed, Some(c)) => format!("exit {c}"),
            _ => String::new(),
        };
        let by = match &s.origin {
            Some(Origin::Agent { client, session }) => {
                let who = if client.is_empty() { "agent" } else { client };
                match session.is_empty() {
                    true => format!("  ⌁ {who}"),
                    false => format!("  ⌁ {who} · {session}"),
                }
            }
            _ => String::new(),
        };

        out.push_str(&format!(
            "  {}  {}  {}  {:>7}  {:>6}  {}{}\n",
            mark(s.state, paint),
            paint.bold(&format!("{:<name_w$}", v.server.name)),
            paint.dim(&format!("{:<state_w$}", word(s.state))),
            paint.terracotta(&port),
            paint.dim(&uptime),
            paint.dim(&if detail.is_empty() {
                paths::contract_tilde(&v.server.dir)
            } else {
                detail
            }),
            paint.dim(&by),
        ));
    }
    out
}
