mod tray;

use cucina_core::manifest::Suggestions;
use cucina_core::model::{Event, Group, LogLine, Origin, Run, Server, Task};
use cucina_core::proto::ServerView;
use cucina_core::{ipc, paths, Stray, Supervisor};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WindowEvent};

pub struct AppState {
    pub sup: Arc<Supervisor>,
    /// True while a slow ticker is refreshing the menu bar's uptime figures.
    ticking: AtomicBool,
}

const EVENT: &str = "cucina://event";

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// ---- commands -------------------------------------------------------------

#[tauri::command]
fn list_servers(state: State<'_, AppState>) -> Vec<ServerView> {
    tray::views(&state.sup)
}

#[tauri::command]
fn start_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sup.start(&id, Origin::User)
}

#[tauri::command]
fn stop_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sup.stop(&id)
}

#[tauri::command]
async fn restart_server(app: AppHandle, id: String) -> Result<(), String> {
    // restart() blocks while it waits for the old process to die.
    let sup = app.state::<AppState>().sup.clone();
    tauri::async_runtime::spawn_blocking(move || sup.restart(&id, Origin::User))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn save_server(state: State<'_, AppState>, server: Server) -> Result<Server, String> {
    state.sup.upsert(server)
}

#[tauri::command]
fn delete_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sup.remove(&id)
}

#[tauri::command]
fn list_groups(state: State<'_, AppState>) -> Vec<Group> {
    state.sup.groups()
}

#[tauri::command]
fn set_group_icon(state: State<'_, AppState>, name: String, icon: String) -> Result<(), String> {
    // One or two glyphs; anything longer would break the sidebar's rhythm.
    let icon: String = icon.chars().take(2).collect();
    state.sup.set_group_icon(&name, &icon)
}

/// Every worktree a server could run from. Empty when its directory isn't a
/// git repository, which the UI reads as "hide the picker".
#[tauri::command]
fn list_worktrees(state: State<'_, AppState>, id: String) -> Vec<cucina_core::Worktree> {
    state
        .sup
        .get(&id)
        .map(|s| cucina_core::git::worktrees(&s.dir))
        .unwrap_or_default()
}

#[tauri::command]
async fn switch_worktree(app: AppHandle, id: String, path: String) -> Result<(), String> {
    // Blocks while the old process is torn down.
    let sup = app.state::<AppState>().sup.clone();
    tauri::async_runtime::spawn_blocking(move || sup.switch_dir(&id, path.into()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn read_logs(state: State<'_, AppState>, id: String, tail: Option<usize>) -> Vec<LogLine> {
    state.sup.tail(&id, tail.unwrap_or(500))
}

#[tauri::command]
fn clear_logs(state: State<'_, AppState>, id: String) {
    state.sup.clear_logs(&id);
}

// ---- tasks ----------------------------------------------------------------

#[tauri::command]
fn list_tasks(state: State<'_, AppState>, id: String) -> Vec<Task> {
    state.sup.tasks(&id)
}

#[tauri::command]
fn run_task(state: State<'_, AppState>, id: String, task_id: String) -> Result<Run, String> {
    state.sup.run_task(&id, &task_id, Origin::User)
}

#[tauri::command]
fn run_command(state: State<'_, AppState>, id: String, command: String) -> Result<Run, String> {
    state.sup.run_command(&id, &command, Origin::User)
}

#[tauri::command]
fn stop_run(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    state.sup.stop_run(&run_id)
}

#[tauri::command]
fn delete_task(state: State<'_, AppState>, id: String, task_id: String) -> Result<(), String> {
    state.sup.remove_task(&id, &task_id)
}

/// The current or most recent run. No output comes back with it: a run writes
/// into the server's own log, so the window the user is already reading has it.
#[tauri::command]
fn read_run(state: State<'_, AppState>, id: String) -> Option<Run> {
    state.sup.run_of(&id)
}

#[tauri::command]
fn suggest_tasks(state: State<'_, AppState>, id: String) -> Suggestions {
    let Some(server) = state.sup.get(&id) else {
        return Suggestions {
            source: String::new(),
            commands: Vec::new(),
        };
    };
    let kept: Vec<String> = state
        .sup
        .tasks(&id)
        .into_iter()
        .map(|t| t.command)
        .collect();
    cucina_core::manifest::suggest(&paths::expand_tilde(&server.dir), &server.command, &kept)
}

/// Runs the probes and answers. Deliberately has no cached companion: the
/// caller asks when the window comes forward and when Scan is pressed, and a
/// stored list would be wrong by the time anybody read it.
#[tauri::command]
fn scan_strays(state: State<'_, AppState>) -> Result<Vec<Stray>, String> {
    state.sup.strays()
}

/// Blocks for as long as the process takes to go, up to about three seconds.
#[tauri::command]
async fn stop_stray(state: State<'_, AppState>, pid: u32) -> Result<(), String> {
    state.sup.stop_stray(pid)
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Only ever hand the browser a local http(s) URL.
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Not a web address.".into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .args(["-R", &path])
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_directory(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv()
            .ok()
            .flatten()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
fn login_item_enabled(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_login_item(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

/// Where the `cucina` binary lives, whether we're running from a bundle or
/// straight out of `cargo tauri dev`.
fn find_cli_binary(app: &AppHandle) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("cucina-cli"));
        }
    }
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("cucina-cli"));
    }
    // Development only: the workspace target directory sits next to this
    // crate. CARGO_MANIFEST_DIR is resolved at compile time, so leaving this
    // in a release build would bake the building machine's home directory
    // into the shipped binary and hand every user a path that exists only on
    // that machine.
    #[cfg(debug_assertions)]
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        if let Some(root) = std::path::Path::new(manifest).parent() {
            candidates.push(root.join("target/release/cucina-cli"));
            candidates.push(root.join("target/debug/cucina-cli"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[tauri::command]
fn install_cli(app: AppHandle) -> Result<String, String> {
    let binary = find_cli_binary(&app)
        .ok_or("Couldn't find the cucina binary. Run `npm run cli:build` in the project first.")?;
    let bin_dir = paths::home().join(".local/bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let link = bin_dir.join("cucina");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&binary, &link).map_err(|e| e.to_string())?;

    let on_path = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|p| std::path::Path::new(p) == bin_dir);
    Ok(if on_path {
        format!(
            "Installed. Try `cucina` in a new terminal.\n{}",
            link.display()
        )
    } else {
        format!(
            "Installed to {}.\nAdd it to your PATH:\n  echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.zshrc",
            link.display()
        )
    })
}

#[tauri::command]
fn mcp_snippet(app: AppHandle) -> String {
    // Prefer the symlink Install writes: it survives the app being replaced
    // by an update, and it is the path the README documents. Fall back to the
    // copy inside the bundle so the snippet still works before Install is
    // pressed — an MCP client does not inherit a shell PATH, so a bare
    // `cucina` would not resolve.
    let installed = paths::home().join(".local/bin/cucina");
    let command = if installed.exists() {
        installed.display().to_string()
    } else {
        find_cli_binary(&app)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "cucina".into())
    };
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": { "cucina": { "command": command, "args": ["mcp"] } }
    }))
    .unwrap_or_default()
}

/// What an update check found. `None` for `version` means we are current.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: Option<String>,
    notes: Option<String>,
}

/// Ask GitHub whether there is a newer release. Only ever runs when the user
/// presses the button — the app makes no network requests on its own.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(match found {
        Some(update) => UpdateInfo {
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        },
        None => UpdateInfo {
            version: None,
            notes: None,
        },
    })
}

/// Download and swap the bundle in place. The payload is verified against the
/// public key compiled into this binary before anything is written, so a
/// tampered release is refused rather than installed.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Already up to date.")?;

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;

    // Servers are ours to clean up; restarting must not orphan them.
    app.state::<AppState>().sup.shutdown();
    app.restart();
}

/// The version from tauri.conf.json, so Settings reports what is actually
/// installed rather than whatever the frontend was compiled against.
#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn home_dir() -> String {
    paths::home().display().to_string()
}

// ---- setup ----------------------------------------------------------------

/// Keep the menu bar's uptime figures honest without polling when idle: the
/// ticker only exists while something is actually running.
fn ensure_ticker(app: &AppHandle, sup: &Arc<Supervisor>) {
    let state = app.state::<AppState>();
    if state.ticking.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    let weak = Arc::downgrade(sup);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let Some(sup) = weak.upgrade() else { return };
            if !sup.statuses().iter().any(|s| s.state.is_live()) {
                break;
            }
            tray::refresh(&handle, &sup);
        }
        handle
            .state::<AppState>()
            .ticking
            .store(false, Ordering::SeqCst);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Single instance. Launching Cucina again — by hand, from the Dock, or via
    // `open -ga Cucina` in the CLI's connect-or-launch path — should raise the
    // window we already have rather than start a rival that shows its own
    // window and its own menu bar icon.
    if ipc::already_running() {
        if let Ok(mut client) = cucina_core::client::Client::connect() {
            let _ = client.request(&cucina_core::proto::Request::Show);
        }
        return;
    }

    let sup = Supervisor::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            sup: sup.clone(),
            ticking: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            list_servers,
            start_server,
            stop_server,
            restart_server,
            save_server,
            delete_server,
            list_groups,
            set_group_icon,
            list_worktrees,
            switch_worktree,
            read_logs,
            clear_logs,
            list_tasks,
            run_task,
            run_command,
            stop_run,
            delete_task,
            read_run,
            suggest_tasks,
            scan_strays,
            stop_stray,
            open_url,
            reveal_in_finder,
            pick_directory,
            login_item_enabled,
            set_login_item,
            install_cli,
            mcp_snippet,
            check_update,
            install_update,
            app_version,
            home_dir,
        ])
        .setup(move |app| {
            // Costs a full interactive shell; do it before anything needs it.
            paths::warm_login_path();

            let handle = app.handle().clone();
            let sup = handle.state::<AppState>().sup.clone();

            tray::create(&handle, sup.clone())?;
            tray::refresh(&handle, &sup);

            // The socket is what lets the CLI and MCP server drive this app.
            if let Err(e) = ipc::serve(sup.clone()) {
                // Losing the race for the socket means another Cucina got
                // there first; step aside rather than run half-connected.
                eprintln!("cucina: couldn't open the control socket: {e}");
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    handle.exit(0);
                    return Ok(());
                }
            }

            // Fan supervisor events out to the UI and the menu bar.
            let weak: Weak<Supervisor> = Arc::downgrade(&sup);
            let event_handle = handle.clone();
            sup.subscribe(Box::new(move |ev| {
                match &ev {
                    // A second launch asked us to come forward.
                    Event::Show => show_main_window(&event_handle),
                    // Log traffic is the chatty one: skip it entirely while the
                    // window is hidden. The ring buffer still has it when the
                    // window comes back.
                    Event::Log { .. } => {
                        let visible = event_handle
                            .get_webview_window("main")
                            .and_then(|w| w.is_visible().ok())
                            .unwrap_or(false);
                        if visible {
                            let _ = event_handle.emit(EVENT, &ev);
                        }
                    }
                    // A task run is not a server, so it changes nothing the
                    // menu bar shows — forward it and skip the tray redraw.
                    Event::Run(_) | Event::Tasks { .. } => {
                        let _ = event_handle.emit(EVENT, &ev);
                    }
                    _ => {
                        let _ = event_handle.emit(EVENT, &ev);
                        if let Some(sup) = weak.upgrade() {
                            tray::refresh(&event_handle, &sup);
                            ensure_ticker(&event_handle, &sup);
                        }
                    }
                }
            }));

            sup.start_auto();
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window puts Cucina in the menu bar rather than
            // killing everything it's tending.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Cucina")
        .run(|app, event| match event {
            // Dock icon click with no visible window.
            tauri::RunEvent::Reopen { .. } => show_main_window(app),
            tauri::RunEvent::ExitRequested { .. } => {
                app.state::<AppState>().sup.shutdown();
            }
            _ => {}
        });
}
