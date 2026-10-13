mod apps;
mod ax;
mod menubar;
mod settings;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{
    ActivationPolicy, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const PANEL_LABEL: &str = "panel";
const SETTINGS_LABEL: &str = "settings";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";
/// Passed by the login item so a launch at login can be told apart from the
/// user opening the app by hand.
const ARG_AUTOSTART: &str = "--autostart";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static LAST_AUTO_HIDE: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
fn list_apps() -> Vec<apps::AppInfo> {
    let mut list = apps::list_menu_bar_apps();
    let wall = menubar::wall_span();
    // Without Accessibility no icon can be located. Every card still offers
    // the choice, so that using it brings up the permission prompt.
    let trusted = ax::trusted(false);
    for info in &mut list {
        // Veil App's own icon is what brings the others back; it never hides.
        if info.pid == std::process::id() as i32 {
            continue;
        }
        info.veiled = if trusted {
            wall.and_then(|wall| app_veiled(info.pid, wall))
        } else {
            Some(true)
        };
    }
    list
}

/// Whether `pid`'s menu bar icon sits left of the wall's right edge, i.e.
/// hides with "Hide icons". `None` when the app has no icon.
fn app_veiled(pid: i32, (_, wall_right): (f64, f64)) -> Option<bool> {
    ax::status_item_x(pid).map(|x| x < wall_right)
}

#[tauri::command]
fn quit_app(pid: i32, force: bool) -> bool {
    apps::quit_app(pid, force)
}

/// How long a reopened app gets to show a window before we fall back to
/// clicking its menu bar icon, and how long the menu bar gets to lay its
/// icons back out after the wall collapses. Tune if slow apps get their icon
/// clicked on top of their window, or the click lands before the icon does.
const WINDOW_WAIT: Duration = Duration::from_millis(800);
const LAYOUT_WAIT: Duration = Duration::from_millis(300);

/// Emitted with the new state whenever the wall changes without the panel's
/// own Hide/Show button, so the button label stays right.
const EVENT_ICONS_HIDDEN: &str = "icons-hidden";

/// Wait up to `WINDOW_WAIT` for `pid` to show a normal window.
fn wait_for_window(pid: i32) -> bool {
    let deadline = std::time::Instant::now() + WINDOW_WAIT;
    while std::time::Instant::now() < deadline {
        if ax::has_window(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Expand or collapse the wall from a background thread and tell the panel.
fn set_icons_hidden(app: &tauri::AppHandle, hidden: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        menubar::set_hidden(hidden);
        let _ = handle.emit(EVENT_ICONS_HIDDEN, hidden);
    });
}

#[tauri::command]
fn activate_app(app: tauri::AppHandle, pid: i32) -> bool {
    if !apps::activate_app(pid) {
        return false;
    }
    // Clicking our own icon would just toggle the panel.
    if pid == std::process::id() as i32 {
        return true;
    }
    // Some apps ignore the reopen above and only open from their own menu
    // bar icon. If no window shows up, click that icon for the user.
    std::thread::spawn(move || {
        if wait_for_window(pid) || !ax::trusted(true) || !ax::has_status_item(pid) {
            return;
        }
        // The icon must be on the bar to be clicked, so bring the icons
        // back for the click.
        let was_hidden = menubar::is_hidden();
        if was_hidden {
            set_icons_hidden(&app, false);
            std::thread::sleep(LAYOUT_WAIT);
        }
        if !ax::click_status_item(pid) {
            eprintln!("no clickable menu bar icon for pid {pid}");
        }
        // Hide again once the app has a window of its own. A popover or
        // menu anchors to the icon instead, so then the icons stay shown.
        if was_hidden && wait_for_window(pid) {
            set_icons_hidden(&app, true);
        }
    });
    true
}

/// Run `f` on the main thread and wait for its result.
fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .ok()?;
    rx.recv().ok()
}

/// How far past the wall's edge an icon is dropped, in points: far enough
/// to land on the neighbouring icon rather than on the wall itself.
const DROP_MARGIN: f64 = 3.0;

/// Choose whether `pid`'s menu bar icon hides with "Hide icons" by
/// Cmd-dragging it across the wall. Returns where the icon ended up, which
/// is the old side when the drag did not take; `None` without the
/// Accessibility permission (macOS then asks for it) or without an icon.
///
/// Runs off the main thread: the drag is paced with sleeps.
#[tauri::command(async)]
fn set_app_veiled(app: tauri::AppHandle, pid: i32, veiled: bool) -> Option<bool> {
    if !ax::trusted(true) {
        return None;
    }
    // An icon behind the expanded wall is off screen and can't be dragged.
    let was_hidden = menubar::is_hidden();
    if was_hidden {
        on_main(&app, || menubar::set_hidden(false));
        std::thread::sleep(LAYOUT_WAIT);
    }
    let wall = || on_main(&app, menubar::wall_span).flatten();
    if let Some((wall_left, wall_right)) = wall() {
        let drop_x = if veiled {
            wall_left - DROP_MARGIN
        } else {
            wall_right + DROP_MARGIN
        };
        if ax::drag_status_item(pid, drop_x) {
            std::thread::sleep(LAYOUT_WAIT);
        }
    }
    // The wall moved if the icon crossed it, so measure again.
    let now = wall().and_then(|wall| app_veiled(pid, wall));
    if was_hidden {
        on_main(&app, || menubar::set_hidden(true));
    }
    now
}

#[tauri::command]
fn toggle_menu_bar_icons() -> bool {
    // Expand/collapse the wall: hides every icon left of Veil App, keeping
    // Veil App and the system items to its right (Wi-Fi, Battery,
    // Spotlight, Control Center, Clock).
    menubar::toggle()
}

#[tauri::command]
fn icons_hidden() -> bool {
    menubar::is_hidden()
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle, label: String) {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.hide();
    }
}

/// Show and focus the settings window. Created once at setup and reused, so
/// reopening keeps the toggles in their current state.
fn open_settings(app: &tauri::AppHandle) {
    if let Some(settings) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = settings.show();
        let _ = settings.unminimize();
        // Activates the app on macOS (activateIgnoringOtherApps), which an
        // accessory-policy app needs for the window to take keyboard focus.
        let _ = settings.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![ARG_AUTOSTART]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Menu bar app: no Dock icon, no main window.
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);
            settings::init(app.handle());

            let panel =
                WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("index.html".into()))
                    .title("Veil App")
                    .inner_size(480.0, 580.0)
                    .decorations(false)
                    .transparent(true)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .visible(false)
                    // Native macOS popover material — Liquid Glass on macOS 26,
                    // NSVisualEffectView popover material on older systems.
                    .effects(
                        tauri::window::EffectsBuilder::new()
                            .effect(tauri::window::Effect::LiquidGlassRegular)
                            .effect(tauri::window::Effect::Popover)
                            .state(tauri::window::EffectState::Active)
                            .radius(16.0)
                            .build(),
                    )
                    // The square window shadow reads as a border around the
                    // rounded glass material; the glass has its own edge
                    // lighting, so no window shadow.
                    .shadow(false)
                    .build()?;

            let panel_for_focus = panel.clone();
            panel.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = panel_for_focus.hide();
                    LAST_AUTO_HIDE.store(now_ms(), Ordering::Relaxed);
                }
            });

            let handle = app.handle().clone();

            // A regular decorated window, built hidden once and reused.
            // Closing it (red traffic light, Esc) only hides it so its state
            // survives and reopening is instant.
            let settings_window = WebviewWindowBuilder::new(
                app,
                SETTINGS_LABEL,
                WebviewUrl::App("settings.html".into()),
            )
            .title("Veil App Settings")
            .inner_size(560.0, 460.0)
            .minimizable(false)
            .maximizable(false)
            .resizable(false)
            .visible(false)
            .build()?;
            let settings_for_close = settings_window.clone();
            settings_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = settings_for_close.hide();
                }
            });

            let menu = MenuBuilder::new(app)
                .text(MENU_SETTINGS, "Settings…")
                .separator()
                .text(MENU_QUIT, "Quit Veil App")
                .build()?;

            let tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("default window icon")
                        .clone(),
                )
                .icon_as_template(true)
                .tooltip("Veil App")
                // Right-click opens the native context menu; left click keeps
                // toggling the panel below.
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(move |_tray, event| {
                    let TrayIconEvent::Click {
                        position,
                        rect,
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    else {
                        return;
                    };
                    let Some(panel) = handle.get_webview_window(PANEL_LABEL) else {
                        return;
                    };

                    if panel.is_visible().unwrap_or(false) {
                        let _ = panel.hide();
                        return;
                    }
                    // The very click that raised this event also stole focus
                    // from the panel and auto-hid it; don't reopen instantly.
                    if now_ms().saturating_sub(LAST_AUTO_HIDE.load(Ordering::Relaxed)) < 350 {
                        return;
                    }

                    // Anchor to the ICON's rect (physical pixels), not the
                    // cursor position — clicking the top or bottom of the
                    // icon must not change the gap. 4pt below the icon puts
                    // the glass right under the menu bar on any scale factor.
                    let monitor = panel
                        .monitor_from_point(position.x, position.y)
                        .ok()
                        .flatten();
                    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(2.0);
                    let (tray_x, tray_y) = match rect.position {
                        tauri::Position::Physical(p) => (f64::from(p.x), f64::from(p.y)),
                        tauri::Position::Logical(p) => (p.x, p.y),
                    };
                    let (tray_w, tray_h) = match rect.size {
                        tauri::Size::Physical(s) => (f64::from(s.width), f64::from(s.height)),
                        tauri::Size::Logical(s) => (s.width, s.height),
                    };
                    let size = panel.outer_size().unwrap_or_default();
                    let mut x =
                        (tray_x + tray_w / 2.0 - f64::from(size.width) / 2.0).round() as i32;
                    // Keep the panel fully on screen when the tray icon sits
                    // near the right edge, with an 8pt margin.
                    if let Some(m) = &monitor {
                        let margin = (8.0 * scale).round() as i32;
                        let right = m.position().x + m.size().width as i32 - margin;
                        x = x
                            .min(right - size.width as i32)
                            .max(m.position().x + margin);
                    }
                    let y = (tray_y + tray_h + 4.0 * scale).round() as i32;
                    let _ = panel.set_position(PhysicalPosition::new(x, y));
                    let _ = panel.show();
                    let _ = panel.set_focus();
                })
                .on_menu_event(|app, event| match event.id().as_ref() {
                    MENU_SETTINGS => open_settings(app),
                    MENU_QUIT => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Seeded autosave positions put the wall directly left of the
            // tray icon, so the tray stays visible when the wall expands.
            tray.with_inner_tray_icon(|inner| {
                if let Some(item) = inner.ns_status_item() {
                    menubar::init(&item);
                }
            })?;

            // Launched at login: start with the icons already hidden.
            if std::env::args().any(|arg| arg == ARG_AUTOSTART) {
                menubar::set_hidden(true);
            }
            // A login item enabled before `ARG_AUTOSTART` existed lacks the
            // argument; re-enabling rewrites it. Release builds only, so a
            // dev run never points the login item at the debug binary.
            if !cfg!(debug_assertions) {
                use tauri_plugin_autostart::ManagerExt;
                let autostart = app.autolaunch();
                if autostart.is_enabled().unwrap_or(false) {
                    let _ = autostart.enable();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            quit_app,
            activate_app,
            set_app_veiled,
            toggle_menu_bar_icons,
            icons_hidden,
            hide_window,
            settings::get_settings,
            settings::set_show_memory_usage
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
