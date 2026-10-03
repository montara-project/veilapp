//! Reading menu bar status items through the Accessibility API.
//!
//! On macOS 26 the system hosts every status item under the Control Center
//! process, so the only supported way to enumerate them with app attribution
//! is to read Control Center's extras menu bar via the Accessibility API.
//! Requires the Accessibility permission (System Settings → Privacy &
//! Security → Accessibility).

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType, CFTypeRef};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::window::{
    copy_window_info, kCGWindowBounds, kCGWindowLayer, kCGWindowListOptionOnScreenOnly,
    kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID, kCGNullWindowID,
};
use std::ffi::c_void;

type AXUIElementRef = *mut c_void;

/// `kAXErrorAPIDisabled` — returned when the Accessibility permission is missing.
const AX_ERROR_API_DISABLED: i32 = -25211;

unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, buf: *mut c_void) -> bool;
}

/// `kAXValueCGPointType`
const AX_VALUE_CG_POINT: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AxPoint {
    x: f64,
    y: f64,
}

pub struct MenuItem {
    pub title: String,
    pub description: String,
    /// On-screen x position of the icon, for matching the hosted window.
    pub x: f64,
}

impl MenuItem {
    /// Best human-readable label for the item.
    pub fn label(&self) -> &str {
        if self.description.is_empty() {
            &self.title
        } else {
            &self.description
        }
    }
}

/// Whether this process may use the Accessibility API.
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Like [`is_trusted`], but shows the system prompt on first use (opens
/// System Settings → Privacy & Security → Accessibility).
pub fn request_access() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) }
}

fn copy_attr(element: AXUIElementRef, attribute: &str) -> Option<CFType> {
    let attr = CFString::new(attribute);
    let mut out: CFTypeRef = std::ptr::null_mut();
    let err =
        unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut out) };
    if err == 0 && !out.is_null() {
        Some(unsafe { CFType::wrap_under_create_rule(out) })
    } else {
        if err != AX_ERROR_API_DISABLED {
            log::debug!("AX attribute {attribute} error {err}");
        }
        None
    }
}

fn string_attr(element: AXUIElementRef, attribute: &str) -> String {
    copy_attr(element, attribute)
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn position_attr(element: AXUIElementRef) -> f64 {
    copy_attr(element, "AXPosition")
        .and_then(|value| {
            let mut point = AxPoint::default();
            // SAFETY: `point` is a valid CGPoint buffer for the AXValue.
            let ok = unsafe {
                AXValueGetValue(value.as_CFTypeRef(), AX_VALUE_CG_POINT, std::ptr::from_mut(&mut point).cast())
            };
            ok.then_some(point.x)
        })
        .unwrap_or(0.0)
}

/// PID of the process hosting the menu bar items ("Control Center" on macOS 26).
fn host_pid() -> Option<i32> {
    host_windows().first().map(|w| w.pid)
}

pub struct HostWindow {
    pub window_id: u32,
    pub pid: i32,
    pub x: f64,
}

/// Every on-screen status-item window (level 25) with its window ID and x
/// position. On macOS 26 these are all owned by the Control Center process.
pub fn host_windows() -> Vec<HostWindow> {
    let Some(list) = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) else {
        return Vec::new();
    };
    let layer_key = unsafe { CFString::wrap_under_get_rule(kCGWindowLayer) };
    let pid_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerPID) };
    let name_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerName) };
    let number_key = unsafe { CFString::wrap_under_get_rule(kCGWindowNumber) };
    let bounds_key = unsafe { CFString::wrap_under_get_rule(kCGWindowBounds) };

    let mut out = Vec::new();
    for raw in list.get_all_values() {
        let dict =
            unsafe { CFDictionary::<CFString, CFType>::wrap_under_get_rule(raw as *const _) };
        let owner = dict
            .find(&name_key)
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let layer = dict
            .find(&layer_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i32())
            .unwrap_or(-1);
        if layer != 25 || owner != "Control Center" {
            continue;
        }
        let Some(pid) = dict
            .find(&pid_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i32())
        else {
            continue;
        };
        let window_id = dict
            .find(&number_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i32())
            .unwrap_or(0) as u32;
        let Some(bounds) = dict
            .find(&bounds_key)
            .and_then(|v| v.downcast::<CFDictionary>())
        else {
            continue;
        };
        let (keys, values) = bounds.get_keys_and_values();
        let axis = |name: &str| -> f64 {
            for (k, v) in keys.iter().zip(values.iter()) {
                // SAFETY: keys/values are CFString/CFNumber owned by `bounds`.
                let ks = unsafe { CFString::wrap_under_get_rule(*k as *const _) };
                if ks.to_string() == name {
                    let n = unsafe { CFNumber::wrap_under_get_rule(*v as *const _) };
                    return n.to_f64().unwrap_or(0.0);
                }
            }
            0.0
        };
        out.push(HostWindow {
            window_id,
            pid,
            x: axis("X"),
        });
    }
    out.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    out
}

/// All status items in the menu bar, in bar order.
///
/// Returns `None` when the Accessibility permission is not granted, so the
/// caller can fall back to a heuristic list.
pub fn menu_bar_items() -> Option<Vec<MenuItem>> {
    if !is_trusted() {
        return None;
    }
    let Some(pid) = host_pid() else {
        return Some(Vec::new());
    };
    let host = unsafe { AXUIElementCreateApplication(pid) };

    // The hosted items live in the extras menu bar.
    let Some(container) = copy_attr(host, "AXExtrasMenuBar").or_else(|| copy_attr(host, "AXMenuBar"))
    else {
        return Some(Vec::new());
    };
    let Some(children) = copy_attr(container.as_CFTypeRef() as AXUIElementRef, "AXChildren") else {
        return Some(Vec::new());
    };
    let Some(items) = children.downcast::<CFArray>() else {
        return Some(Vec::new());
    };

    let mut out = Vec::new();
    for child in items.iter() {
        let element = *child as AXUIElementRef;
        out.push(MenuItem {
            title: string_attr(element, "AXTitle"),
            description: string_attr(element, "AXDescription"),
            x: position_attr(element),
        });
    }
    Some(out)
}
