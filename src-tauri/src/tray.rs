//! The menu bar: what's on the heat, at a glance, with start/stop in one click.
//!
//! Rebuilt only when a status actually changes — never on a timer, and never
//! for log output.

use cucina_core::model::{Origin, State};
use cucina_core::proto::ServerView;
use cucina_core::Supervisor;
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "cucina";

const IDLE_ICON: &[u8] = include_bytes!("../icons/tray-idle.png");
const ACTIVE_ICON: &[u8] = include_bytes!("../icons/tray-active.png");

fn mark(state: State) -> &'static str {
    match state {
        State::Running => "●",
        State::Starting => "◐",
        State::Crashed => "✕",
        State::Stopped => "○",
    }
}

/// One line per server: "● api · :3000 · 12m". Kept terse so the menu stays
/// scannable rather than becoming a table.
fn label(view: &ServerView) -> String {
    let s = &view.status;
    let mut parts = vec![format!("{}  {}", mark(s.state), view.server.name)];
    match s.state {
        State::Running | State::Starting => {
            if let Some(port) = s.port {
                parts.push(format!(":{port}"));
            }
            if let Some(started) = s.started_at {
                let ms = cucina_core::model::now_ms().saturating_sub(started);
                parts.push(uptime(ms));
            }
            if let Some(Origin::Agent { client }) = &s.origin {
                let who = if client.is_empty() { "agent" } else { client };
                parts.push(format!("⌁ {who}"));
            }
        }
        State::Crashed => {
            parts.push(match s.exit_code {
                Some(c) => format!("exit {c}"),
                None => "crashed".into(),
            });
        }
        State::Stopped => {}
    }
    parts.join("   ")
}

fn uptime(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h{:02}", secs / 3600, (secs % 3600) / 60)
    }
}

/// Clicking a row toggles it — the shortest path to what you came for.
fn toggle_item<R: Runtime>(app: &AppHandle<R>, view: &ServerView) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        format!("toggle:{}", view.server.id),
        label(view),
        true,
        None::<&str>,
    )
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, views: &[ServerView]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    if views.is_empty() {
        let empty = MenuItem::with_id(
            app,
            "noop",
            "Nothing in the kitchen yet",
            false,
            None::<&str>,
        )?;
        menu.append(&empty)?;
    } else {
        // Ungrouped servers sit at the top level; a project's services collapse
        // into one submenu you can start or stop as a unit.
        for view in views.iter().filter(|v| v.server.group.is_empty()) {
            menu.append(&toggle_item(app, view)?)?;
        }

        let mut names: Vec<&str> = Vec::new();
        for view in views {
            let group = view.server.group.as_str();
            if !group.is_empty() && !names.contains(&group) {
                names.push(group);
            }
        }

        for group in names {
            let members: Vec<&ServerView> =
                views.iter().filter(|v| v.server.group == group).collect();
            let live = members.iter().filter(|v| v.status.state.is_live()).count();
            // The project emoji went with the redesign — there is no longer
            // anywhere in the app to set one, so a leftover glyph here would
            // be unchangeable. The field stays on the record; nothing reads it.
            let titled = group.to_string();
            let heading = if live > 0 {
                format!("{titled}   {live}/{} on", members.len())
            } else {
                titled
            };

            let submenu = Submenu::with_id(app, format!("group:{group}"), heading, true)?;
            for view in &members {
                submenu.append(&toggle_item(app, view)?)?;
            }
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            submenu.append(&MenuItem::with_id(
                app,
                format!("group-start:{group}"),
                format!("Start all of {group}"),
                live < members.len(),
                None::<&str>,
            )?)?;
            submenu.append(&MenuItem::with_id(
                app,
                format!("group-stop:{group}"),
                format!("Stop all of {group}"),
                live > 0,
                None::<&str>,
            )?)?;
            menu.append(&submenu)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let any_live = views.iter().any(|v| v.status.state.is_live());
    if any_live {
        let stop_all = MenuItem::with_id(app, "stop-all", "Stop everything", true, None::<&str>)?;
        menu.append(&stop_all)?;
    }
    let open = MenuItem::with_id(app, "open", "Open Cucina", true, Some("Cmd+O"))?;
    menu.append(&open)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Cucina", true, Some("Cmd+Q"))?;
    menu.append(&quit)?;

    Ok(menu)
}

pub fn create<R: Runtime>(app: &AppHandle<R>, sup: Arc<Supervisor>) -> tauri::Result<TrayIcon<R>> {
    let views = views(&sup);
    let menu = build_menu(app, &views)?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(IDLE_ICON)?)
        // Template mode lets macOS invert the mark for light and dark menu bars.
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| on_menu_event(app, event.id().as_ref()))
        .build(app)?;

    Ok(tray)
}

pub fn views(sup: &Arc<Supervisor>) -> Vec<ServerView> {
    let statuses = sup.statuses();
    sup.servers()
        .into_iter()
        .zip(statuses)
        .map(|(server, status)| ServerView { server, status })
        .collect()
}

/// Redraw the menu and the icon. Cheap, but only called on real state changes.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, sup: &Arc<Supervisor>) {
    let views = views(sup);
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    if let Ok(menu) = build_menu(app, &views) {
        let _ = tray.set_menu(Some(menu));
    }

    let live = views.iter().filter(|v| v.status.state.is_live()).count();
    let icon = if live > 0 { ACTIVE_ICON } else { IDLE_ICON };
    if let Ok(image) = Image::from_bytes(icon) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(true);
    }
    // A bare count next to the mark, and nothing at all when idle. Clearing
    // has to be an empty string: passing None reads as "leave it alone" on
    // macOS, which strands the last count in the menu bar after everything
    // stops.
    let _ = tray.set_title(Some(if live > 0 {
        live.to_string()
    } else {
        String::new()
    }));
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(sup) = app.try_state::<crate::AppState>().map(|s| s.sup.clone()) else {
        return;
    };
    match id {
        "quit" => {
            sup.shutdown();
            app.exit(0);
        }
        "open" => crate::show_main_window(app),
        "stop-all" => {
            for view in views(&sup) {
                if view.status.state.is_live() {
                    let _ = sup.stop(&view.server.id);
                }
            }
        }
        other => {
            if let Some(server_id) = other.strip_prefix("toggle:") {
                let live = sup
                    .statuses()
                    .into_iter()
                    .find(|s| s.id == server_id)
                    .is_some_and(|s| s.state.is_live());
                let _ = if live {
                    sup.stop(server_id)
                } else {
                    sup.start(server_id, Origin::User)
                };
            } else if let Some(group) = other.strip_prefix("group-start:") {
                for view in views(&sup) {
                    if view.server.group == group && !view.status.state.is_live() {
                        let _ = sup.start(&view.server.id, Origin::User);
                    }
                }
            } else if let Some(group) = other.strip_prefix("group-stop:") {
                for view in views(&sup) {
                    if view.server.group == group && view.status.state.is_live() {
                        let _ = sup.stop(&view.server.id);
                    }
                }
            }
        }
    }
}
