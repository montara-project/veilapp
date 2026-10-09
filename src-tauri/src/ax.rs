//! Clicking another app's menu bar icon on the user's behalf.
//!
//! Apps that only show their window or popover from their own status item
//! click handler (Tauri/Electron tray apps, Alfred, …) ignore the reopen
//! event, so the only way to "open" them is to click that icon. Finding the
//! icon and posting the click both need the Accessibility permission.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::ptr;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;
const CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const CG_HID_EVENT_TAP: u32 = 0;
const CG_WINDOW_LIST_ON_SCREEN_ONLY: u32 = 1;
const CG_WINDOW_LIST_EXCLUDE_DESKTOP: u32 = 16;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, out: *mut c_void) -> bool;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    fn CGEventCreate(source: *const c_void) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        position: CGPoint,
        button: u32,
    ) -> CFTypeRef;
    fn CGEventPost(tap: u32, event: CFTypeRef);
    fn CGWarpMouseCursorPosition(position: CGPoint) -> i32;
}

/// Whether Veil App may use the Accessibility API. With `prompt`, macOS shows
/// its "grant access in System Settings" dialog when it may not.
pub fn trusted(prompt: bool) -> bool {
    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let options = CFDictionary::from_CFType_pairs(&[(
        key.as_CFType(),
        CFBoolean::from(prompt).as_CFType(),
    )]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Whether `pid` owns a normal window that is currently shown. Needs no
/// permission: owner and layer are public window-list fields.
pub fn has_window(pid: i32) -> bool {
    let list = unsafe {
        CGWindowListCopyWindowInfo(
            CG_WINDOW_LIST_ON_SCREEN_ONLY | CG_WINDOW_LIST_EXCLUDE_DESKTOP,
            0,
        )
    };
    if list.is_null() {
        return false;
    }
    let list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_create_rule(list) };
    let number = |window: &CFDictionary<CFString, CFType>, key: &'static str| {
        window
            .find(CFString::from_static_string(key))
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
    };
    list.iter().any(|window| {
        number(&window, "kCGWindowOwnerPID") == Some(i64::from(pid))
            && number(&window, "kCGWindowLayer") == Some(0)
    })
}

fn attribute(element: &CFType, name: &'static str) -> Option<CFType> {
    let name = CFString::from_static_string(name);
    let mut value: CFTypeRef = ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            &mut value,
        )
    };
    (err == 0 && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
}

fn point(element: &CFType, name: &'static str, value_type: u32) -> Option<CGPoint> {
    let value = attribute(element, name)?;
    let mut out = CGPoint::default();
    unsafe { AXValueGetValue(value.as_CFTypeRef(), value_type, (&raw mut out).cast()) }
        .then_some(out)
}

/// The first menu bar icon owned by `pid`, wherever the wall has put it.
fn status_item(pid: i32) -> Option<CFType> {
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    let app = unsafe { CFType::wrap_under_create_rule(app) };
    attribute(&app, "AXExtrasMenuBar")
        .and_then(|bar| attribute(&bar, "AXChildren"))
        .and_then(|children| children.downcast_into::<CFArray>())
        .and_then(|children| {
            children
                .get(0)
                .map(|item| unsafe { CFType::wrap_under_get_rule(*item) })
        })
}

pub fn has_status_item(pid: i32) -> bool {
    status_item(pid).is_some()
}

/// Left-click the first menu bar icon owned by `pid`. Returns `false` when
/// the app has no icon or it is not on a screen (e.g. still behind the wall).
///
/// A real mouse click rather than `AXPress`: many tray implementations
/// (Tauri's included) react to mouse events on the icon, not to the button
/// action that `AXPress` fires. The cursor is put back afterwards.
// ponytail: apps with several icons (Stats) get their first one clicked;
// pick by name if that ever matters.
pub fn click_status_item(pid: i32) -> bool {
    let Some(item) = status_item(pid) else {
        return false;
    };
    let (Some(origin), Some(size)) = (
        point(&item, "AXPosition", AX_VALUE_CG_POINT),
        point(&item, "AXSize", AX_VALUE_CG_SIZE),
    ) else {
        return false;
    };
    let target = CGPoint {
        x: origin.x + size.x / 2.0,
        y: origin.y + size.y / 2.0,
    };
    // Items pushed off the bar by the wall sit left of every display.
    if size.x <= 0.0 || !on_a_screen(target) {
        return false;
    }

    unsafe {
        let here = CGEventCreate(ptr::null());
        let cursor = (!here.is_null()).then(|| {
            let here = CFType::wrap_under_create_rule(here);
            CGEventGetLocation(here.as_CFTypeRef())
        });
        for mouse_type in [CG_EVENT_LEFT_MOUSE_DOWN, CG_EVENT_LEFT_MOUSE_UP] {
            let event = CGEventCreateMouseEvent(ptr::null(), mouse_type, target, 0);
            if event.is_null() {
                return false;
            }
            let event = CFType::wrap_under_create_rule(event);
            CGEventPost(CG_HID_EVENT_TAP, event.as_CFTypeRef());
        }
        if let Some(cursor) = cursor {
            CGWarpMouseCursorPosition(cursor);
        }
    }
    true
}

fn on_a_screen(p: CGPoint) -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGGetDisplaysWithPoint(
            point: CGPoint,
            max_displays: u32,
            displays: *mut u32,
            matching_display_count: *mut u32,
        ) -> i32;
    }
    let mut count = 0u32;
    let err = unsafe { CGGetDisplaysWithPoint(p, 0, ptr::null_mut(), &mut count) };
    err == 0 && count > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_and_trust_checks_do_not_panic() {
        // Reads only; never prompts and never clicks.
        let _ = trusted(false);
        assert!(!has_window(-1));
        assert!(!has_status_item(-1));
        assert!(!click_status_item(-1));
    }
}
