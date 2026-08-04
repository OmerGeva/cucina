mod mcp;
mod ui;

use cucina_core::client::Client;
use cucina_core::model::{Origin, Server, Stream};
use cucina_core::proto::Request;
use std::collections::{BTreeMap, HashMap};
use ui::Paint;

const DEFAULT_WAIT_MS: u64 = 45_000;

/// Flags that consume the following argument when written with a space.
const VALUE_FLAGS: &[&str] = &[
    "name", "dir", "command", "cmd", "env", "tail", "agent", "client", "group",
];

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Args {
        let mut positional = Vec::new();
        let mut flags: HashMap<String, Vec<String>> = HashMap::new();
        let mut i = 0;
        while i < raw.len() {
            let arg = raw[i].clone();
            if let Some(body) = arg.strip_prefix("--") {
                if let Some((k, v)) = body.split_once('=') {
                    flags.entry(k.to_string()).or_default().push(v.to_string());
                } else if VALUE_FLAGS.contains(&body) && i + 1 < raw.len() {
                    flags
                        .entry(body.to_string())
                        .or_default()
                        .push(raw[i + 1].clone());
                    i += 1;
                } else {
                    flags.entry(body.to_string()).or_default();
                }
            } else {
                positional.push(arg);
            }
            i += 1;
        }
        Args { positional, flags }
    }

    fn has(&self, key: &str) -> bool {
        self.flags.contains_key(key)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.flags.get(key)?.first().map(|s| s.as_str())
    }

    fn all(&self, key: &str) -> Vec<String> {
        self.flags.get(key).cloned().unwrap_or_default()
    }

    fn wait_ms(&self) -> Option<u64> {
        if !self.has("wait") {
            return None;
        }
        Some(
            self.get("wait")
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_WAIT_MS),
        )
    }
}

/// Work out whether a human or an agent is driving. Agents get tagged so the
/// menu bar can show who started what.
fn origin(args: &Args) -> Origin {
    if let Some(name) = args.get("agent").or_else(|| args.get("client")) {
        return Origin::Agent { client: name.to_string() };
    }
    if args.has("agent") {
        return Origin::Agent { client: "agent".into() };
    }
    if let Ok(name) = std::env::var("CUCINA_CLIENT") {
        return Origin::Agent { client: name };
    }
    if std::env::var_os("CLAUDECODE").is_some() {
        return Origin::Agent { client: "Claude Code".into() };
    }
    // A non-interactive stdout almost always means something scripted us.
    if !ui::is_tty(1) {
        return Origin::Agent { client: "agent".into() };
    }
    Origin::User
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let command = raw.first().cloned().unwrap_or_else(|| "list".into());

    // The MCP server owns stdio and must not print anything else.
    if command == "mcp" {
        mcp::run();
        return;
    }

    let args = Args::parse(raw.into_iter().skip(1).collect());
    let code = match run(&command, &args) {
        Ok(()) => 0,
        Err(message) => {
            let paint = Paint::detect();
            eprintln!("{} {message}", paint.red("✕"));
            1
        }
    };
    std::process::exit(code);
}

fn run(command: &str, args: &Args) -> Result<(), String> {
    let paint = Paint::detect();
    match command {
        "help" | "-h" | "--help" => {
            print!("{}", help(&paint));
            Ok(())
        }
        "version" | "-v" | "--version" => {
            println!("cucina {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "list" | "ls" | "status" => {
            let mut c = Client::connect_or_launch()?;
            let res = c.request(&Request::List)?;
            let mut views = res.views();
            if let Some(id) = args.positional.first() {
                views.retain(|v| &v.server.id == id || &v.server.name == id);
            }
            if args.has("json") {
                println!("{}", serde_json::to_string_pretty(&views).unwrap_or_default());
            } else {
                print!("{}", ui::table(&views, &paint));
            }
            Ok(())
        }
        "up" | "start" | "restart" => {
            let key = need_id(args)?;
            let mut c = Client::connect_or_launch()?;
            let targets = resolve(&c.request(&Request::List)?.views(), &key)?;
            let mut failed = false;
            for id in &targets {
                let request = if command == "restart" {
                    Request::Restart {
                        id: id.clone(),
                        origin: origin(args),
                        wait_ms: args.wait_ms(),
                    }
                } else {
                    Request::Start {
                        id: id.clone(),
                        origin: origin(args),
                        wait_ms: args.wait_ms(),
                    }
                };
                match c.request(&request) {
                    Ok(res) => report_start(id, &res, args, &paint),
                    Err(e) => {
                        eprintln!("  {} {id}: {e}", paint.red("✕"));
                        failed = true;
                    }
                }
            }
            if failed {
                Err("Some servers didn't start.".into())
            } else {
                Ok(())
            }
        }
        "down" | "stop" => {
            let key = need_id(args)?;
            let mut c = Client::connect_or_launch()?;
            let targets = resolve(&c.request(&Request::List)?.views(), &key)?;
            for id in &targets {
                c.request(&Request::Stop { id: id.clone() })?;
                if !args.has("json") {
                    println!("  {} {id} stopped", paint.dim("○"));
                }
            }
            Ok(())
        }
        "logs" => {
            let id = need_id(args)?;
            let tail = args.get("tail").and_then(|v| v.parse().ok()).unwrap_or(200);
            let mut c = Client::connect_or_launch()?;
            let res = c.request(&Request::Logs { id, tail: Some(tail) })?;
            let lines = res.lines();
            if args.has("json") {
                println!("{}", serde_json::to_string_pretty(&lines).unwrap_or_default());
            } else {
                for line in lines {
                    match line.stream {
                        Stream::System => println!("{}", paint.dim(&line.text)),
                        Stream::Stderr => println!("{}", paint.red(&line.text)),
                        Stream::Stdout => println!("{}", line.text),
                    }
                }
            }
            Ok(())
        }
        "add" => {
            let server = build_server(args)?;
            let mut c = Client::connect_or_launch()?;
            let res = c.request(&Request::Add { server })?;
            let added: Server = serde_json::from_value(res.data.clone())
                .map_err(|e| format!("Unexpected reply: {e}"))?;
            if args.has("json") {
                println!("{}", serde_json::to_string_pretty(&added).unwrap_or_default());
            } else {
                println!(
                    "  {} {} added — start it with {}",
                    paint.terracotta("+"),
                    paint.bold(&added.name),
                    paint.bold(&format!("cucina up {}", added.id))
                );
            }
            Ok(())
        }
        "rm" | "remove" => {
            let id = need_id(args)?;
            let mut c = Client::connect_or_launch()?;
            c.request(&Request::Remove { id: id.clone() })?;
            println!("  {} {id} removed", paint.dim("−"));
            Ok(())
        }
        "worktrees" | "wt" => {
            let key = need_id(args)?;
            let mut c = Client::connect_or_launch()?;
            let views = c.request(&Request::List)?.views();
            let view = views
                .iter()
                .find(|v| v.server.id == key || v.server.name.to_lowercase() == key.to_lowercase())
                .ok_or_else(|| format!("No server called {key}."))?;
            let trees = cucina_core::git::worktrees(&view.server.dir);
            if trees.is_empty() {
                return Err(format!("{} isn't a git worktree.", view.server.dir.display()));
            }
            if args.has("json") {
                println!("{}", serde_json::to_string_pretty(&trees).unwrap_or_default());
            } else {
                for tree in &trees {
                    let mark = if tree.is_current { paint.green("●") } else { paint.dim("○") };
                    let tag = if tree.is_main { paint.dim("  base") } else { String::new() };
                    println!("  {mark}  {}{tag}", paint.bold(&tree.branch));
                }
            }
            Ok(())
        }
        "switch" => {
            let key = need_id(args)?;
            let target = args
                .positional
                .get(1)
                .ok_or("Which worktree? e.g. `cucina switch api feat-620-diagram-editor`")?;
            let mut c = Client::connect_or_launch()?;
            let views = c.request(&Request::List)?.views();
            let view = views
                .iter()
                .find(|v| v.server.id == key || v.server.name.to_lowercase() == key.to_lowercase())
                .ok_or_else(|| format!("No server called {key}."))?;
            let was_live = view.status.state.is_live();
            let tree = resolve_worktree(&view.server.dir, target)?;
            let res = c.request(&Request::Switch {
                id: view.server.id.clone(),
                path: tree.path.to_string_lossy().to_string(),
            })?;
            if args.has("json") {
                println!("{}", serde_json::to_string_pretty(&res.data).unwrap_or_default());
            } else {
                println!(
                    "  {} {} now on {}{}",
                    paint.terracotta("⎇"),
                    view.server.id,
                    paint.bold(&tree.branch),
                    if was_live { " — restarted" } else { "" }
                );
            }
            Ok(())
        }
        "open" => {
            let id = need_id(args)?;
            let mut c = Client::connect_or_launch()?;
            let views = c.request(&Request::List)?.views();
            let view = views
                .iter()
                .find(|v| v.server.id == id || v.server.name == id)
                .ok_or_else(|| format!("No server called {id}."))?;
            let port = view
                .status
                .port
                .ok_or_else(|| format!("{id} isn't listening on a port."))?;
            let url = format!("http://localhost:{port}");
            std::process::Command::new("/usr/bin/open")
                .arg(&url)
                .status()
                .map_err(|e| e.to_string())?;
            println!("  {}", paint.terracotta(&url));
            Ok(())
        }
        other => Err(format!("Don't know how to `{other}`. Try `cucina help`.")),
    }
}

fn need_id(args: &Args) -> Result<String, String> {
    args.positional
        .first()
        .cloned()
        .ok_or_else(|| "Which server? e.g. `cucina up api`".to_string())
}

/// Resolve a worktree by branch name, or by a path that ends with what the
/// user typed. Listing git is the CLI's own business — only the switch itself
/// has to go through the app.
fn resolve_worktree(
    dir: &std::path::Path,
    target: &str,
) -> Result<cucina_core::git::Worktree, String> {
    let trees = cucina_core::git::worktrees(dir);
    if trees.is_empty() {
        return Err(format!("{} isn't a git worktree.", dir.display()));
    }
    let lowered = target.to_lowercase();
    let found = trees
        .iter()
        .find(|w| w.branch.to_lowercase() == lowered)
        .or_else(|| {
            trees
                .iter()
                .find(|w| w.path.to_string_lossy().to_lowercase().ends_with(&lowered))
        });
    match found {
        Some(w) => Ok(w.clone()),
        None => Err(format!(
            "No worktree matching `{target}`. Available:\n  {}",
            trees
                .iter()
                .map(|w| w.branch.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        )),
    }
}

/// A name can mean one server or a whole group — `cucina up acme` should
/// bring up every service in that project.
fn resolve(views: &[cucina_core::proto::ServerView], key: &str) -> Result<Vec<String>, String> {
    let key = key.to_lowercase();
    if let Some(view) = views
        .iter()
        .find(|v| v.server.id.to_lowercase() == key || v.server.name.to_lowercase() == key)
    {
        return Ok(vec![view.server.id.clone()]);
    }
    let group: Vec<String> = views
        .iter()
        .filter(|v| !v.server.group.is_empty() && v.server.group.to_lowercase() == key)
        .map(|v| v.server.id.clone())
        .collect();
    if !group.is_empty() {
        return Ok(group);
    }
    Err(format!("No server or group called {key}."))
}

fn report_start(id: &str, res: &cucina_core::proto::Response, args: &Args, paint: &Paint) {
    if args.has("json") {
        println!("{}", serde_json::to_string_pretty(&res.data).unwrap_or_default());
        return;
    }
    let view: Option<cucina_core::proto::ServerView> =
        serde_json::from_value(res.data.clone()).ok().flatten();
    match view.as_ref().and_then(|v| v.status.port) {
        Some(port) => println!(
            "  {} {id} ready on {}",
            paint.green("●"),
            paint.terracotta(&format!("http://localhost:{port}"))
        ),
        None => println!("  {} {id} started", paint.green("●")),
    }
}

fn build_server(args: &Args) -> Result<Server, String> {
    let name = args
        .get("name")
        .ok_or("A --name is required, e.g. --name api")?
        .to_string();
    let command = args
        .get("command")
        .or_else(|| args.get("cmd"))
        .ok_or("A --command is required, e.g. --command \"npm run dev\"")?
        .to_string();
    let dir_raw = args.get("dir").unwrap_or(".");
    let dir = std::fs::canonicalize(dir_raw)
        .map_err(|_| format!("Can't find the directory {dir_raw}."))?;

    let mut env = BTreeMap::new();
    for pair in args.all("env") {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| format!("--env wants KEY=VALUE, got `{pair}`"))?;
        env.insert(k.to_string(), v.to_string());
    }

    Ok(Server {
        id: String::new(),
        name,
        dir,
        command,
        group: args.get("group").unwrap_or_default().to_string(),
        tile: 0,
        env,
        auto_restart: args.has("auto-restart"),
        auto_start: args.has("auto-start"),
        created_at: 0,
    })
}

fn help(paint: &Paint) -> String {
    let b = |s: &str| paint.bold(s);
    let d = |s: &str| paint.dim(s);
    let mut out = String::new();
    out.push_str(&format!(
        "{}  {}\n\n",
        paint.terracotta("cucina"),
        d("— keep your local servers on the heat")
    ));
    let rows: &[(String, String)] = &[
        (b("cucina"), d("what's on the heat")),
        (format!("{} {}", b("cucina up"), d("<id>")), d("start one, --wait until it's listening")),
        (format!("{} {}", b("cucina down"), d("<id>")), d("stop one")),
        (format!("{} {}", b("cucina restart"), d("<id>")), d("stop and start again")),
        (format!("{} {}", b("cucina logs"), d("<id>")), d("recent output, --tail N")),
        (format!("{} {}", b("cucina open"), d("<id>")), d("open it in the browser")),
        (format!("{} {}", b("cucina worktrees"), d("<id>")), d("git worktrees it can run from")),
        (format!("{} {}", b("cucina switch"), d("<id> <branch>")), d("move it to another worktree")),
        (b("cucina add"), d("--name api --dir . --command \"npm run dev\"")),
        (format!("{} {}", b("cucina rm"), d("<id>")), d("remove one")),
        (b("cucina mcp"), d("run as an MCP server for coding agents")),
    ];
    for (left, right) in rows {
        // Pad on the visible width, ignoring the ANSI escapes.
        let visible = strip_ansi(left).chars().count();
        let pad = 34usize.saturating_sub(visible);
        out.push_str(&format!("  {left}{:pad$}{right}\n", "", pad = pad));
    }
    out.push_str(&format!(
        "\n{}\n  {}  {}\n  {}  {}\n  {}  {}\n  {}  {}\n",
        d("Flags"),
        b("--json          "),
        d("machine-readable output, for agents"),
        b("--agent <name>  "),
        d("attribute the run to an agent"),
        b("--env K=V       "),
        d("set an environment variable (repeatable)"),
        b("--auto-restart  "),
        d("bring it back if it crashes"),
    ));
    out
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
