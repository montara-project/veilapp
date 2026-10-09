use base64::Engine as _;
use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{AnyThread, MainThreadMarker, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSCompositingOperation, NSImage, NSRunningApplication, NSWorkspace,
    NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{NSArray, NSDictionary, NSRect, NSSize};
use serde::Serialize;

#[derive(Serialize)]
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
    apps
}

pub fn quit_app(pid: i32, force: bool) -> bool {
    running_apps()
        .into_iter()
        .find(|app| app.processIdentifier() == pid)
        .is_some_and(|app| {
            if force {
                app.forceTerminate()
            } else {
                app.terminate()
            }
        })
}

/// Bring an app to the front the way clicking it in the Dock does.
///
/// `activateWithOptions` alone is not enough: since macOS 14 activation is
/// cooperative (`ActivateIgnoringOtherApps` is ignored) so the request is
/// dropped unless the active app — our panel — yields to the target first,
/// and it never reopens a window the user closed. Opening the app's bundle
/// sends the reopen event, which shows a window when none is open.
pub fn activate_app(pid: i32) -> bool {
    let Some(app) = running_apps()
        .into_iter()
        .find(|a| a.processIdentifier() == pid)
    else {
        return false;
    };
    if let Some(mtm) = MainThreadMarker::new() {
        let me = NSApplication::sharedApplication(mtm);
        // macOS 14+ only; older systems activate without yielding.
        if me.respondsToSelector(sel!(yieldActivationToApplication:)) {
            me.yieldActivationToApplication(&app);
        }
    }
    if let Some(url) = app.bundleURL() {
        let config = NSWorkspaceOpenConfiguration::new();
        config.setActivates(true);
        NSWorkspace::sharedWorkspace()
            .openApplicationAtURL_configuration_completionHandler(&url, &config, None);
        return true;
    }
    // Bundle-less processes can't be reopened; activate them directly.
    #[allow(deprecated)]
    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows)
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
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        Some(base64::engine::general_purpose::STANDARD.encode(png.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_apps_without_panicking() {
        let apps = list_menu_bar_apps();
        assert!(apps.len() < 1000);
    }
}
