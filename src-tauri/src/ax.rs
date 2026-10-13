//! Clicking and moving another app's menu bar icon on the user's behalf.
//!
//! Apps that only show their window or popover from their own status item
//! click handler (Tauri/Electron tray apps, Alfred, …) ignore the reopen
//! event, so the only way to "open" them is to click that icon. Finding the
//! icon and posting the click both need the Accessibility permission.
//!
//! The same goes for choosing which icons hide: an icon hides when it sits
//! left of the wall (see `menubar`), and only a Cmd-drag moves it across.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::ptr;
use std::time::Duration;

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
const CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
const CG_EVENT_FLAG_COMMAND: u64 = 1 << 20;
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
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventPost(tap: u32, event: CFTypeRef);
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, timeout_seconds: f32) -> i32;
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

const AX_TIMEOUT_SECS: f32 = 0.3;

/// The first menu bar icon owned by `pid`, wherever the wall has put it.
fn status_item(pid: i32) -> Option<CFType> {
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    let app = unsafe { CFType::wrap_under_create_rule(app) };
    // The panel asks every listed app on each refresh; a hung app must not
    // stall it for the default 6 s.
    unsafe { AXUIElementSetMessagingTimeout(app.as_CFTypeRef(), AX_TIMEOUT_SECS) };
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

/// Centre of the first menu bar icon owned by `pid`, on a screen or not.
// ponytail: apps with several icons (Stats) get their first one clicked or
// moved; pick by name if that ever matters.
fn status_item_center(pid: i32) -> Option<CGPoint> {
    let item = status_item(pid)?;
    let origin = point(&item, "AXPosition", AX_VALUE_CG_POINT)?;
    let size = point(&item, "AXSize", AX_VALUE_CG_SIZE)?;
    (size.x > 0.0).then_some(CGPoint {
        x: origin.x + size.x / 2.0,
        y: origin.y + size.y / 2.0,
    })
}

/// Horizontal centre of `pid`'s menu bar icon, in global screen points.
/// Icons pushed off the bar by the wall sit left of every display.
pub fn status_item_x(pid: i32) -> Option<f64> {
    status_item_center(pid).map(|center| center.x)
}

fn cursor_position() -> Option<CGPoint> {
    let here = unsafe { CGEventCreate(ptr::null()) };
    (!here.is_null()).then(|| unsafe {
        let here = CFType::wrap_under_create_rule(here);
        CGEventGetLocation(here.as_CFTypeRef())
    })
}

fn post_mouse(mouse_type: u32, at: CGPoint, flags: u64) -> bool {
    unsafe {
        let event = CGEventCreateMouseEvent(ptr::null(), mouse_type, at, 0);
        if event.is_null() {
            return false;
        }
        let event = CFType::wrap_under_create_rule(event);
        CGEventSetFlags(event.as_CFTypeRef(), flags);
        CGEventPost(CG_HID_EVENT_TAP, event.as_CFTypeRef());
    }
    true
}

/// Left-click the first menu bar icon owned by `pid`. Returns `false` when
/// the app has no icon or it is not on a screen (e.g. still behind the wall).
///
/// A real mouse click rather than `AXPress`: many tray implementations
/// (Tauri's included) react to mouse events on the icon, not to the button
/// action that `AXPress` fires. The cursor is put back afterwards.
pub fn click_status_item(pid: i32) -> bool {
    let Some(target) = status_item_center(pid).filter(|p| on_a_screen(*p)) else {
        return false;
    };
    let cursor = cursor_position();
    let clicked = post_mouse(CG_EVENT_LEFT_MOUSE_DOWN, target, 0)
        && post_mouse(CG_EVENT_LEFT_MOUSE_UP, target, 0);
    if let Some(cursor) = cursor {
        unsafe { CGWarpMouseCursorPosition(cursor) };
    }
    clicked
}

/// How a synthetic Cmd-drag is paced. The menu bar only picks an icon up
/// once the pointer has travelled while held, and drops it where the last
/// drag event left it. Tune if icons stay put or land one slot off.
const DRAG_STEPS: u32 = 12;
const DRAG_STEP_WAIT: Duration = Duration::from_millis(15);
const DRAG_HOLD_WAIT: Duration = Duration::from_millis(60);

/// Cmd-drag the first menu bar icon owned by `pid` along the bar and drop it
/// at `to_x`, the way a user rearranges icons by hand. Returns `false` when
/// the app has no icon or it is not on a screen. The owning app saves the new
/// spot itself, so the move survives relaunches. Blocks for the whole drag;
/// the cursor is put back afterwards.
pub fn drag_status_item(pid: i32, to_x: f64) -> bool {
    let Some(from) = status_item_center(pid).filter(|p| on_a_screen(*p)) else {
        return false;
    };
    let cursor = cursor_position();
    let mut dragged = post_mouse(CG_EVENT_LEFT_MOUSE_DOWN, from, CG_EVENT_FLAG_COMMAND);
    let mut at = from;
    std::thread::sleep(DRAG_HOLD_WAIT);
    for step in 1..=DRAG_STEPS {
        at.x = from.x + (to_x - from.x) * f64::from(step) / f64::from(DRAG_STEPS);
        dragged &= post_mouse(CG_EVENT_LEFT_MOUSE_DRAGGED, at, CG_EVENT_FLAG_COMMAND);
        std::thread::sleep(DRAG_STEP_WAIT);
    }
    std::thread::sleep(DRAG_HOLD_WAIT);
    // Always release, or the bar is left holding the icon.
    dragged &= post_mouse(CG_EVENT_LEFT_MOUSE_UP, at, CG_EVENT_FLAG_COMMAND);
    if let Some(cursor) = cursor {
        unsafe { CGWarpMouseCursorPosition(cursor) };
    }
    dragged
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
        assert!(status_item_x(-1).is_none());
        assert!(!drag_status_item(-1, 0.0));
    }
}
