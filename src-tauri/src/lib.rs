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
    apps::list_menu_bar_apps()
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
    // Expand/collapse the wall: hides every icon left of Veil App, keeping
    // Veil App and the system items to its right (Wi-Fi, Battery,
    // Spotlight, Control Center, Clock).
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
            let tray = TrayIconBuilder::new()
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
                    let mut x = (tray_x + tray_w / 2.0 - f64::from(size.width) / 2.0).round() as i32;
                    // Keep the panel fully on screen when the tray icon sits
                    // near the right edge, with an 8pt margin.
                    if let Some(m) = &monitor {
                        let margin = (8.0 * scale).round() as i32;
                        let right = m.position().x + m.size().width as i32 - margin;
                        x = x.min(right - size.width as i32).max(m.position().x + margin);
                    }
                    let y = (tray_y + tray_h + 4.0 * scale).round() as i32;
                    let _ = panel.set_position(PhysicalPosition::new(x, y));
                    let _ = panel.show();
                    let _ = panel.set_focus();
                })
                .build(app)?;

            // Seeded autosave positions put the wall directly left of the
            // tray icon, so the tray stays visible when the wall expands.
            tray.with_inner_tray_icon(|inner| {
                if let Some(item) = inner.ns_status_item() {
                    menubar::init(&item);
                }
            })?;
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
        .expect("error while running tauri application");
}
