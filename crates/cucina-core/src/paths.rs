use std::path::{Path, PathBuf};

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
