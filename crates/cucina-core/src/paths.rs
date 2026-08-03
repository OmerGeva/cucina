use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// ~/Library/Application Support/Cucina
pub fn data_dir() -> PathBuf {
    let dir = home().join("Library/Application Support/Cucina");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn servers_file() -> PathBuf {
    data_dir().join("servers.json")
}

/// The socket lives under Application Support so the CLI can find it without
/// any configuration. Unix socket paths are capped near 104 bytes, which this
/// comfortably fits for a normal home directory.
pub fn socket_path() -> PathBuf {
    data_dir().join("cucina.sock")
}

/// Expand a leading `~` so hand-typed and stored paths both work.
pub fn expand_tilde(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s == "~" {
        return home();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return home().join(rest);
    }
    p.to_path_buf()
}

/// Render a path with `~` for display, so the UI shows `~/code/api`.
pub fn contract_tilde(p: &Path) -> String {
    let h = home();
    match p.strip_prefix(&h) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// The user's login shell, which is what we run commands through so that
/// PATH, nvm, asdf, mise and friends all behave the way they do in Terminal.
///
/// The wrapper we generate is POSIX shell, so a non-POSIX login shell (fish,
/// nushell) falls back to zsh rather than failing to parse.
/// The PATH a Terminal window would have.
///
/// An app launched from Finder inherits launchd's bare environment, and a
/// *non-interactive* login shell does not read ~/.zshrc — which is exactly
/// where nvm, asdf and mise install themselves. So resolve the value from a
/// fully interactive shell once, and hand it to every server we spawn.
///
/// The value is fenced by markers because an interactive startup file may
/// print banners of its own (ssh-agent, version managers, MOTD) and none of
/// that is PATH.
pub fn login_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(resolve_login_path).as_deref()
}

fn resolve_login_path() -> Option<String> {
    const OPEN: &str = "__cucina_path(";
    const CLOSE: &str = ")cucina_path__";

    let output = Command::new(login_shell())
        .args(["-ilc", &format!("printf '{OPEN}%s{CLOSE}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find(OPEN)? + OPEN.len();
    let end = start + text[start..].find(CLOSE)?;
    let path = text[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Resolving costs a full interactive shell startup, so warm it off the hot
/// path rather than paying for it on the first server the user starts.
pub fn warm_login_path() {
    std::thread::spawn(|| {
        let _ = login_path();
    });
}

pub fn login_shell() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let name = Path::new(&shell)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match name.as_str() {
        "zsh" | "bash" | "sh" | "ksh" | "dash" => shell,
        _ => "/bin/zsh".to_string(),
    }
}
