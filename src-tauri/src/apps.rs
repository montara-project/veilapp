use base64::Engine as _;
use objc2::{msg_send, AnyThread};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBitmapImageFileType,
    NSBitmapImageRep, NSCompositingOperation, NSImage, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSDictionary, NSRect, NSSize};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;

use crate::ax_menubar;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub pid: i32,
    pub memory_bytes: u64,
    pub icon_png: String,
}

/// The panel list: real menu bar items when the Accessibility permission is
/// granted, Accessory-policy apps otherwise.
pub fn list_menu_bar_apps() -> Vec<AppInfo> {
    match ax_menubar::menu_bar_items() {
        Some(items) if !items.is_empty() => apps_from_items(items),
        _ => accessory_apps(),
    }
}

/// Map menu bar items to running apps by name and keep the matched apps in
/// menu bar order. Apple-owned entries (Wi-Fi, clock, Control Center…) have
/// no matching third-party app and are skipped.
fn apps_from_items(items: Vec<ax_menubar::MenuItem>) -> Vec<AppInfo> {
    let labels: Vec<String> = items.iter().map(|i| i.label().to_string()).collect();
    log_labels_once(&labels);

    let running = running_apps();
    let me = std::process::id() as i32;

    let mut apps: Vec<AppInfo> = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let label = item.label().trim().to_string();
        if label.is_empty() {
            continue;
        }
        let lower = label.to_lowercase();
        let matched = running.iter().find(|a| {
            let pid = a.processIdentifier();
            pid != me
                && !seen.contains(&pid)
                && !bundle_id_is_apple(a)
                && a.localizedName()
                    .map(|n| n.to_string().to_lowercase() == lower)
                    .unwrap_or(false)
        });
        let Some(app) = matched else { continue };
        let pid = app.processIdentifier();
        seen.insert(pid);
        apps.push(AppInfo {
            name: app.localizedName().map(|n| n.to_string()).unwrap_or(label),
            pid,
            memory_bytes: resident_memory(pid).unwrap_or(0),
            icon_png: app
                .icon()
                .and_then(|icon| icon_to_png_base64(&icon))
                .unwrap_or_default(),
        });
    }
    apps
}

/// Fallback when Accessibility is not granted: background/menu bar apps
/// (Accessory policy) have no Dock presence and live in the menu bar.
fn accessory_apps() -> Vec<AppInfo> {
    let me = std::process::id() as i32;
    let mut apps = Vec::new();
    for app in running_apps() {
        let pid = app.processIdentifier();
        if pid == me || app.activationPolicy() != NSApplicationActivationPolicy::Accessory {
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

/// Log the raw menu bar item labels only when they change, so the 5s polling
/// doesn't spam the log and permission/matching issues stay visible.
fn log_labels_once(labels: &[String]) {
    static LAST: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap();
    if *last != labels {
        log::info!("menu bar items: {labels:?}");
        *last = labels.to_vec();
    }
}

fn bundle_id_is_apple(app: &NSRunningApplication) -> bool {
    app.bundleIdentifier()
        .map(|b| b.to_string().starts_with("com.apple."))
        .unwrap_or(false)
}

/// Window IDs of the menu bar items that belong to third-party apps (the
/// panel list), matched by on-screen x position. `None` when the
/// Accessibility permission is missing. Our own icon is excluded.
pub fn menu_bar_item_window_ids() -> Option<Vec<u32>> {
    let items = ax_menubar::menu_bar_items()?;
    let windows = ax_menubar::host_windows();
    if windows.is_empty() {
        return Some(Vec::new());
    }
    let me = std::process::id() as i32;
    let running = running_apps();

    let mut ids = Vec::new();
    for item in items {
        let label = item.label().trim().to_lowercase();
        if label.is_empty() {
            continue;
        }
        let matched = running.iter().find(|a| {
            let pid = a.processIdentifier();
            pid != me
                && !bundle_id_is_apple(a)
                && a.localizedName()
                    .map(|n| n.to_string().to_lowercase() == label)
                    .unwrap_or(false)
        });
        let Some(_) = matched else { continue };
        if let Some(win) = windows.iter().find(|w| (w.x - item.x).abs() < 8.0) {
            ids.push(win.window_id);
        }
    }
    Some(ids)
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
    let running: Option<Retained<NSArray<NSRunningApplication>>> =
        unsafe { msg_send![&ws, runningApplications] };
    running.map(|r| r.to_vec()).unwrap_or_default()
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

    #[test]
    fn lists_menu_bar_apps_without_panicking() {
        let apps = list_menu_bar_apps();
        // Depends on what is running, but the call itself must always succeed.
        assert!(apps.len() < 1000);
    }
}
