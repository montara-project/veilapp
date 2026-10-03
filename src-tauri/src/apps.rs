use base64::Engine as _;
use objc2::{msg_send, AnyThread};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBitmapImageFileType,
    NSBitmapImageRep, NSCompositingOperation, NSImage, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSDictionary, NSRect, NSSize};
use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub pid: i32,
    pub memory_bytes: u64,
    pub icon_png: String,
}

pub fn list_regular_apps() -> Vec<AppInfo> {
    let ws = NSWorkspace::sharedWorkspace();
    // objc2-app-kit 0.3.2 doesn't generate the `runningApplications` property,
    // so we call the selector directly.
    let running: Option<Retained<NSArray<NSRunningApplication>>> =
        unsafe { msg_send![&ws, runningApplications] };
    let Some(running) = running else {
        return Vec::new();
    };

    let mut apps = Vec::new();
    for app in running.to_vec() {
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        let pid = app.processIdentifier();
        let name = app
            .localizedName()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        let memory_bytes = resident_memory(pid).unwrap_or(0);
        let icon_png = app
            .icon()
            .and_then(|icon| icon_to_png_base64(&icon))
            .unwrap_or_default();
        apps.push(AppInfo {
            name,
            pid,
            memory_bytes,
            icon_png,
        });
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
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
