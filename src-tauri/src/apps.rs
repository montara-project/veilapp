use base64::Engine as _;
use objc2::{msg_send, AnyThread};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSCompositingOperation, NSImage, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSDictionary, NSRect, NSSize};
use serde::Serialize;

use crate::ax_menubar;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub pid: i32,
    pub memory_bytes: u64,
    pub icon_png: String,
}

/// The panel list: all third-party running apps (including Veil App itself).
///
/// On macOS 26 every status item is hosted anonymously under Control Center —
/// window lists and the Accessibility API (Control Center's tree AND each
/// app's own tree) expose no per-icon app names. The menu bar apps the user
/// cares about are third-party apps, so this is the closest honest list.
pub fn list_menu_bar_apps() -> Vec<AppInfo> {
    let mut apps = Vec::new();
    for app in running_apps() {
        let pid = app.processIdentifier();
        if bundle_id_is_apple(&app) {
            continue;
        }
        let name = app
            .localizedName()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        apps.push(AppInfo {
            name,
            pid,
            memory_bytes: resident_memory(pid).unwrap_or(0),
            icon_png: app
                .icon()
                .and_then(|icon| icon_to_png_base64(&icon))
                .unwrap_or_default(),
        });
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// System items that must stay visible: battery, Wi-Fi, and — via the
/// sandwich rule below — Spotlight. Clock and Control Center are hidden,
/// matching the requested keep-list. Adjust here to change it.
const KEEP_IDENTIFIERS: [&str; 2] = [
    "com.apple.menuextra.battery",
    "com.apple.menuextra.wifi",
];

/// Which hosted menu bar windows to hide for the Hide/Show toggle.
///
/// On macOS 26 only the system items are named in AX; third-party items are
/// anonymous stubs. So: keep our own tray window, the AX-named battery/Wi-Fi
/// windows, and the Spotlight window (the anonymous one sandwiched between
/// battery and Wi-Fi — system items keep their relative order). Everything
/// else gets hidden.
fn compute_windows_to_hide(
    windows: &[ax_menubar::HostWindow],
    items: &[ax_menubar::MenuItem],
    own_tray: Option<u32>,
) -> Vec<u32> {
    let mut keep_x: Vec<f64> = Vec::new();
    let mut battery_win: Option<f64> = None;
    let mut wifi_win: Option<f64> = None;
    for item in items {
        if !KEEP_IDENTIFIERS.contains(&item.identifier.as_str()) || item.x <= 0.0 {
            continue;
        }
        if let Some(w) = windows.iter().find(|w| (w.x - item.x).abs() <= 10.0) {
            keep_x.push(w.x);
            if item.identifier == KEEP_IDENTIFIERS[0] {
                battery_win = Some(w.x);
            } else {
                wifi_win = Some(w.x);
            }
        }
    }
    if let (Some(battery), Some(wifi)) = (battery_win, wifi_win) {
        if let Some(spotlight) = windows.iter().find(|w| w.x > battery && w.x < wifi) {
            keep_x.push(spotlight.x);
        }
    }

    windows
        .iter()
        .filter(|w| own_tray != Some(w.window_id))
        .filter(|w| !keep_x.iter().any(|kx| (kx - w.x).abs() <= 10.0))
        .map(|w| w.window_id)
        .collect()
}

/// Window IDs to hide for the Hide/Show toggle. `None` when the
/// Accessibility permission is missing.
pub fn third_party_window_ids() -> Option<Vec<u32>> {
    let windows = ax_menubar::host_windows();
    if windows.is_empty() {
        return Some(Vec::new());
    }
    let items = ax_menubar::menu_bar_items()?;
    let own = ax_menubar::detect_own_tray_window(&windows);
    Some(compute_windows_to_hide(&windows, &items, own))
}

pub fn quit_app(pid: i32, force: bool) -> bool {
    let running = running_apps();
    for app in running {
        if app.processIdentifier() == pid {
            return if force {
                app.forceTerminate()
            } else {
                app.terminate()
            };
        }
    }
    false
}

pub fn activate_app(pid: i32) -> bool {
    let running = running_apps();
    for app in running {
        if app.processIdentifier() == pid {
            #[allow(deprecated)]
            let options = NSApplicationActivationOptions::ActivateAllWindows
                | NSApplicationActivationOptions::ActivateIgnoringOtherApps;
            return app.activateWithOptions(options);
        }
    }
    false
}

fn running_apps() -> Vec<Retained<NSRunningApplication>> {
    let ws = NSWorkspace::sharedWorkspace();
    // objc2-app-kit 0.3.2 doesn't generate the `runningApplications` property,
    // so we call the selector directly.
    let running: Option<Retained<NSArray<NSRunningApplication>>> =
        unsafe { msg_send![&ws, runningApplications] };
    running.map(|r| r.to_vec()).unwrap_or_default()
}

fn bundle_id_is_apple(app: &NSRunningApplication) -> bool {
    app.bundleIdentifier()
        .map(|b| b.to_string().starts_with("com.apple."))
        .unwrap_or(false)
}

/// Physical memory footprint of a process (what Activity Monitor shows as
/// "Memory"), via libproc.
fn resident_memory(pid: i32) -> Option<u64> {
    let usage: libproc::pid_rusage::RUsageInfoV3 = libproc::pid_rusage::pidrusage(pid).ok()?;
    Some(usage.ri_phys_footprint)
}

const ICON_SIZE: f64 = 64.0;

/// Render the app icon at a fixed small size and return it as base64 PNG.
fn icon_to_png_base64(icon: &NSImage) -> Option<String> {
    #[allow(deprecated)]
    {
        let scaled = NSImage::initWithSize(NSImage::alloc(), NSSize::new(ICON_SIZE, ICON_SIZE));
        scaled.lockFocus();
        icon.drawInRect_fromRect_operation_fraction(
            NSRect {
                origin: objc2_foundation::NSPoint { x: 0.0, y: 0.0 },
                size: NSSize::new(ICON_SIZE, ICON_SIZE),
            },
            NSRect::ZERO,
            NSCompositingOperation::Copy,
            1.0,
        );
        scaled.unlockFocus();

        let tiff = scaled.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)?;
        // SAFETY: `rep` is a valid bitmap image rep; the call only reads it.
        let png = unsafe {
            rep.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }?;
        Some(base64::engine::general_purpose::STANDARD.encode(png.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(identifier: &str, x: f64) -> ax_menubar::MenuItem {
        ax_menubar::MenuItem {
            title: String::new(),
            description: String::new(),
            identifier: identifier.to_string(),
            x,
        }
    }

    fn window(x: f64) -> ax_menubar::HostWindow {
        ax_menubar::HostWindow {
            window_id: x as u32,
            pid: 8113,
            x,
        }
    }

    /// Reproduction from real measurements on macOS 26 (hosted window list
    /// and AX positions): the toggle must keep Veil App itself, battery,
    /// Spotlight (the anonymous window between battery and Wi-Fi) and Wi-Fi
    /// visible, and hide every third-party icon plus Clock and Control
    /// Center.
    #[test]
    fn hides_everything_except_veilapp_battery_search_wifi() {
        let windows: Vec<ax_menubar::HostWindow> = [
            892.0, 926.0, 964.0, 1002.0, 1040.0, 1078.0, 1125.0, 1201.0, 1233.0, 1271.0, 1313.0,
        ]
        .into_iter()
        .map(window)
        .collect();
        let items = vec![
            item("com.apple.menuextra.battery", 1133.0),
            item("com.apple.menuextra.clock", 1321.0),
            item("com.apple.menuextra.wifi", 1241.0),
            item("com.apple.menuextra.controlcenter", 1279.0),
        ];

        let hidden = compute_windows_to_hide(&windows, &items, Some(892));

        assert_eq!(
            hidden,
            vec![926, 964, 1002, 1040, 1078, 1271, 1313],
            "third-party icons + Clock + Control Center hidden; veilapp/battery/search/wifi kept"
        );
    }

    #[test]
    fn lists_apps_without_panicking() {
        let apps = list_menu_bar_apps();
        assert!(apps.len() < 1000);
    }
}
