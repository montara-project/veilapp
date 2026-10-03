mod apps;
mod menubar;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
};

const PANEL_LABEL: &str = "panel";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static LAST_AUTO_HIDE: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
fn list_apps() -> Vec<apps::AppInfo> {
    apps::list_regular_apps()
}

#[tauri::command]
fn quit_app(pid: i32, force: bool) -> bool {
    apps::quit_app(pid, force)
}

#[tauri::command]
fn activate_app(pid: i32) -> bool {
    apps::activate_app(pid)
}

#[tauri::command]
fn toggle_menu_bar_icons() -> bool {
    menubar::toggle()
}

#[tauri::command]
fn hide_panel(app: tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        let _ = panel.hide();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        // Menu bar app: no Dock icon, no main window.
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let panel =
                WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("index.html".into()))
                    .title("Veil App")
                    .inner_size(400.0, 580.0)
                    .decorations(false)
                    .transparent(true)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .visible(false)
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
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("default window icon").clone())
                .icon_as_template(true)
                .tooltip("Veil App")
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

                    // All values are in physical pixels, so no scale factor is
                    // needed: center the panel under the tray icon.
                    let (tray_w, tray_h) = match rect.size {
                        tauri::Size::Physical(s) => (f64::from(s.width), f64::from(s.height)),
                        tauri::Size::Logical(s) => (s.width, s.height),
                    };
                    let size = panel.outer_size().unwrap_or_default();
                    let x =
                        (position.x + tray_w / 2.0 - f64::from(size.width) / 2.0).round() as i32;
                    let y = (position.y + tray_h + 6.0).round() as i32;
                    let _ = panel.set_position(PhysicalPosition::new(x, y));
                    let _ = panel.show();
                    let _ = panel.set_focus();
                })
                .build(app)?;

            // Create the wall AFTER the tray: new status items enter at the
            // left end, so the tray icon ends up right of the wall and stays
            // visible when the wall expands. Users can still Cmd-drag both.
            menubar::init();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            quit_app,
            activate_app,
            toggle_menu_bar_icons,
            hide_panel
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
