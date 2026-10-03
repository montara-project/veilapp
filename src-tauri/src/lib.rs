mod apps;
mod ax_menubar;
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
    // Self-healing: while icons are hidden, re-assert alpha on the current
    // third-party windows — Control Center recreates hosted windows with
    // fresh ids as it re-lays-out the bar.
    if ax_menubar::is_trusted() && menubar::icons_hidden() {
        if let Some(ids) = apps::third_party_window_ids() {
            menubar::set_icons_hidden(&ids, true);
        }
    }
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
    // Hide every hosted third-party menu bar icon (per the keep-list: our
    // own tray window, Battery, Spotlight, Wi-Fi). Without Accessibility,
    // fall back to the wall spacer.
    if !ax_menubar::is_trusted() {
        return menubar::toggle();
    }
    let hidden = !menubar::icons_hidden();
    if hidden {
        let ids = apps::third_party_window_ids().unwrap_or_default();
        menubar::set_icons_hidden(&ids, true);
    } else {
        // Control Center recreates hosted windows with fresh ids as it
        // re-lays-out, so restore both the recorded ids and whatever is
        // third-party right now.
        let mut ids = menubar::hidden_window_ids();
        ids.extend(apps::third_party_window_ids().unwrap_or_default());
        ids.sort_unstable();
        ids.dedup();
        menubar::set_icons_hidden(&ids, false);
    }
    hidden
}

#[tauri::command]
fn hide_panel(app: tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        let _ = panel.hide();
    }
}

#[tauri::command]
fn accessibility_granted() -> bool {
    ax_menubar::is_trusted()
}

/// Asks macOS for the Accessibility permission; shows the system prompt that
/// deep-links into System Settings on first use.
#[tauri::command]
fn request_accessibility() -> bool {
    ax_menubar::request_access()
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

            // Snapshot the hosted status windows before our tray exists, so
            // per-icon hiding can exclude the tray's own window.
            ax_menubar::snapshot_before_tray();

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

                    // Anchor to the ICON's rect (physical pixels), not the
                    // cursor position — clicking the top or bottom of the
                    // icon must not change the gap. 4pt below the icon puts
                    // the glass right under the menu bar on any scale factor.
                    let scale = panel
                        .monitor_from_point(position.x, position.y)
                        .ok()
                        .flatten()
                        .map(|m| m.scale_factor())
                        .unwrap_or(2.0);
                    let (tray_x, tray_y) = match rect.position {
                        tauri::Position::Physical(p) => (f64::from(p.x), f64::from(p.y)),
                        tauri::Position::Logical(p) => (p.x, p.y),
                    };
                    let (tray_w, tray_h) = match rect.size {
                        tauri::Size::Physical(s) => (f64::from(s.width), f64::from(s.height)),
                        tauri::Size::Logical(s) => (s.width, s.height),
                    };
                    let size = panel.outer_size().unwrap_or_default();
                    let x = (tray_x + tray_w / 2.0 - f64::from(size.width) / 2.0).round() as i32;
                    let y = (tray_y + tray_h + 4.0 * scale).round() as i32;
                    let _ = panel.set_position(PhysicalPosition::new(x, y));
                    let _ = panel.show();
                    let _ = panel.set_focus();
                })
                .build(app)?;

            // Create the wall AFTER the tray so the tray icon ends up right of
            // the wall and stays visible when the wall expands. On macOS 26
            // the exact ordering is decided by the system; users can still
            // Cmd-drag both items into place if needed.
            menubar::init();

            // Recover from a previous crashed session: any window we
            // alpha-hid must start out visible again (fresh hidden-state).
            ax_menubar::restore_all_hosted_windows();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            quit_app,
            activate_app,
            toggle_menu_bar_icons,
            hide_panel,
            accessibility_granted,
            request_accessibility
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                // Never leave the user's menu bar with invisible icons.
                menubar::restore_hidden_icons();
            }
        });
}
