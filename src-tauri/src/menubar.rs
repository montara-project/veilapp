use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2_app_kit::{NSStatusBar, NSStatusItem};
use objc2_foundation::{NSRect, NSString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// A blank "wall" status item. Widening it pushes every menu bar icon to its
/// left out of the visible bar — the same public-API trick Hidden Bar and Ice
/// use. macOS clips status items at the left edge of the status area, so
/// anything left of the wall simply disappears until the wall collapses.
///
/// Hiding other apps' status item windows directly (e.g. CGSSetWindowAlpha)
/// is silently ignored by the window server for windows our connection does
/// not own, so the wall is the only reliable mechanism.
struct WallItem(Retained<NSStatusItem>);
// The wall is only ever touched from the main thread (setup + sync commands).
unsafe impl Send for WallItem {}
unsafe impl Sync for WallItem {}

static WALL: Mutex<Option<WallItem>> = Mutex::new(None);
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Narrow enough to stay visible so it can be Cmd-dragged into place.
const WALL_COLLAPSED: f64 = 8.0;
/// Wide enough to push the whole icon cluster off the widest bar.
const WALL_EXPANDED: f64 = 10_000.0;

const TRAY_AUTOSAVE: &str = "veilapp-icon";
const WALL_AUTOSAVE: &str = "veilapp-wall";

unsafe extern "C" {
    static kCFPreferencesCurrentApplication: CFStringRef;
    fn CFPreferencesCopyAppValue(key: CFStringRef, application_id: CFStringRef) -> CFTypeRef;
    fn CFPreferencesSetAppValue(key: CFStringRef, value: CFTypeRef, application_id: CFStringRef);
    fn CFPreferencesAppSynchronize(application_id: CFStringRef) -> bool;
    static kCFPreferencesCurrentUser: CFStringRef;
    static kCFPreferencesAnyHost: CFStringRef;
    fn CFPreferencesCopyKeyList(
        application_id: CFStringRef,
        user: CFStringRef,
        host: CFStringRef,
    ) -> CFArrayRef;
}

/// System items that must stay visible while icons are hidden, as
/// (preferences domain, status item autosave name). Clock and Control
/// Center are pinned to the right edge by macOS and always stay visible.
const KEEP_ITEMS: [(&str, &str); 3] = [
    ("com.apple.Spotlight", "Item-0"),
    ("com.apple.controlcenter", "WiFi"),
    ("com.apple.controlcenter", "Battery"),
];

fn copy_pref(app_id: CFStringRef, key: &str) -> Option<CFType> {
    let key = CFString::new(key);
    let value = unsafe { CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), app_id) };
    (!value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
}

fn position_key(autosave_name: &str) -> String {
    format!("NSStatusItem Preferred Position {autosave_name}")
}

fn read_position(app_id: CFStringRef, autosave_name: &str) -> Option<f64> {
    copy_pref(app_id, &position_key(autosave_name))?
        .downcast::<CFNumber>()
        .and_then(|n| n.to_f64())
}

/// `false` only when the user turned the item off in the menu bar settings.
fn is_visible(app_id: CFStringRef, autosave_name: &str) -> bool {
    let Some(value) = copy_pref(app_id, &format!("NSStatusItem Visible {autosave_name}")) else {
        return true;
    };
    value
        .downcast::<CFBoolean>()
        .map(bool::from)
        .or_else(|| value.downcast::<CFNumber>().and_then(|n| n.to_i64()).map(|n| n != 0))
        .unwrap_or(true)
}

/// Preferred position of the leftmost visible keep-list item (0 when none).
fn leftmost_keep_position() -> f64 {
    KEEP_ITEMS
        .iter()
        .filter_map(|(domain, name)| {
            let domain = CFString::new(domain);
            let domain = domain.as_concrete_TypeRef();
            is_visible(domain, name)
                .then(|| read_position(domain, name))
                .flatten()
        })
        .fold(0.0, f64::max)
}

/// The wall's preferred position, see `seed_positions`.
fn wall_position() -> Option<f64> {
    read_position(unsafe { kCFPreferencesCurrentApplication }, WALL_AUTOSAVE)
}

/// Whether `domain` has a status item turned on that sits right of the wall
/// at preferred position `wall`. Read from preferred positions (a smaller one
/// sits further right), so it works without the Accessibility permission.
/// Items named in `skip` are left out.
fn has_item_right_of(wall: f64, domain: &str, skip: &[&str]) -> bool {
    let domain = CFString::new(domain);
    let domain = domain.as_concrete_TypeRef();
    let keys = unsafe {
        CFPreferencesCopyKeyList(domain, kCFPreferencesCurrentUser, kCFPreferencesAnyHost)
    };
    if keys.is_null() {
        return false;
    }
    let keys: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(keys) };
    let prefix = position_key("");
    keys.iter()
        .filter_map(|key| key.to_string().strip_prefix(&prefix).map(str::to_owned))
        .filter(|name| !skip.contains(&name.as_str()))
        .any(|name| {
            is_visible(domain, &name) && read_position(domain, &name).is_some_and(|pos| pos < wall)
        })
}

/// Seed where macOS places our tray icon and the wall, before they get their
/// autosave names. A preferred position is the item's distance from the
/// right edge of the screen; a larger value sits further left, and items are
/// laid out in that order. Placing both just left of the leftmost keep-list
/// item yields `… [wall][Veil App] Wi-Fi ⋯ Battery ⋯ Spotlight ⋯ Clock`, so
/// expanding the wall hides everything except Veil App and the items to its
/// right. Third-party icons sitting between keep-list items stay visible;
/// Cmd-drag them left of the wall to hide them too.
///
/// Only seeded when missing or broken (wall not left of the icon, or not
/// left of every keep-list item), so a user's own Cmd-drag arrangement
/// survives relaunches.
fn seed_positions() {
    let own = unsafe { kCFPreferencesCurrentApplication };
    let keep = leftmost_keep_position();
    let icon = read_position(own, TRAY_AUTOSAVE);
    let wall = read_position(own, WALL_AUTOSAVE);
    if let (Some(icon), Some(wall)) = (icon, wall) {
        if wall > icon && wall > keep {
            return;
        }
    }
    for (name, pos) in [(TRAY_AUTOSAVE, keep + 1.0), (WALL_AUTOSAVE, keep + 2.0)] {
        let key = CFString::new(&position_key(name));
        let value = CFNumber::from(pos);
        unsafe { CFPreferencesSetAppValue(key.as_concrete_TypeRef(), value.as_CFTypeRef(), own) };
    }
    unsafe { CFPreferencesAppSynchronize(own) };
}

/// Pin our tray icon right of the wall and create the wall.
/// Must be called on the main thread (AppKit requirement), after the tray
/// icon exists.
pub fn init(tray: &NSStatusItem) {
    let mut wall = WALL.lock().unwrap();
    if wall.is_some() {
        return;
    }
    seed_positions();
    tray.setAutosaveName(Some(&NSString::from_str(TRAY_AUTOSAVE)));
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(WALL_COLLAPSED);
    item.setAutosaveName(Some(&NSString::from_str(WALL_AUTOSAVE)));
    *wall = Some(WallItem(item));
}

/// Expand or collapse the wall. Returns `true` when icons are now hidden.
/// Must be called on the main thread.
pub fn toggle() -> bool {
    let hidden = !is_hidden();
    set_hidden(hidden);
    hidden
}

pub fn is_hidden() -> bool {
    HIDDEN.load(Ordering::SeqCst)
}

/// Must be called on the main thread.
pub fn set_hidden(hidden: bool) {
    HIDDEN.store(hidden, Ordering::SeqCst);
    if native::available() {
        native::set_hidden(hidden);
        return;
    }
    if let Some(item) = WALL.lock().unwrap().as_ref() {
        item.0.setLength(if hidden { WALL_EXPANDED } else { WALL_COLLAPSED });
    }
}

/// macOS 27 ejects an expanded wall from the menu bar instead of letting it
/// push icons off, so expanding it hides nothing. There, hiding goes through
/// the allow-list behind the assessment (exam) mode in the private
/// MenuBarClientCore framework: every status item whose app is not on the
/// list hides until the assertion is invalidated or this process exits. The
/// collapsed wall stays as the boundary: apps whose icon sits right of it
/// are allowed, as are all system items (macOS still hides some of its own,
/// e.g. Spotlight, and offers no way to keep them).
///
/// The framework is resolved at runtime, so a macOS release that drops it
/// falls back to the wall rather than failing to launch.
mod native {
    use super::{HIDDEN, has_item_right_of, wall_position, wall_span};
    use block2::RcBlock;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::{msg_send, sel};
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::{NSArray, NSNumber, NSProcessInfo, NSString};
    use std::ffi::{CStr, c_char, c_int, c_void};
    use std::sync::{Mutex, OnceLock};

    const FRAMEWORK: &CStr =
        c"/System/Library/PrivateFrameworks/MenuBarClientCore.framework/MenuBarClientCore";
    const CONFIGURATION: &CStr = c"MBAssessmentModeConfiguration";
    const ASSERTION: &CStr = c"MBAssessmentModeAssertion";
    const RTLD_NOW: c_int = 0x2;
    const RTLD_LOCAL: c_int = 0x4;
    /// System item ids to keep; the same full range Hidden Bar passes.
    const SYSTEM_ITEMS: std::ops::Range<i32> = 0..64;

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    }

    struct Assertion(Retained<AnyObject>);
    // Only stored and invalidated; the framework's completion handler may run
    // off the main thread.
    unsafe impl Send for Assertion {}

    static HELD: Mutex<Option<Assertion>> = Mutex::new(None);

    fn init_sel() -> Sel {
        sel!(initWithAllowedSystemItems:allowedBundleIdentifiers:)
    }
    fn activate_sel() -> Sel {
        sel!(activateWithConfiguration:completionHandler:)
    }

    /// macOS 27 or later with the framework, both classes and every selector
    /// used below present.
    pub fn available() -> bool {
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            if NSProcessInfo::processInfo().operatingSystemVersion().majorVersion < 27
                || unsafe { dlopen(FRAMEWORK.as_ptr(), RTLD_NOW | RTLD_LOCAL) }.is_null()
            {
                return false;
            }
            let (Some(config), Some(assertion)) =
                (AnyClass::get(CONFIGURATION), AnyClass::get(ASSERTION))
            else {
                return false;
            };
            config.instance_method(init_sel()).is_some()
                && assertion.instance_method(activate_sel()).is_some()
                && assertion.instance_method(sel!(invalidate)).is_some()
        })
    }

    fn invalidate(assertion: Assertion) {
        let _: () = unsafe { msg_send![&*assertion.0, invalidate] };
    }

    const CONTROL_CENTER: &str = "com.apple.controlcenter";
    /// Control Center extras that `SYSTEM_ITEMS` already keep (as
    /// autosave names). Every other one (Focus, Now Playing...) only stays
    /// through the `com.apple.controlcenter` bundle.
    const COVERED_EXTRAS: [&str; 8] = [
        "WiFi",
        "Battery",
        "BentoBox",
        "Clock",
        "ScreenMirroring",
        "Bluetooth",
        "Display",
        "Sound",
    ];

    /// Bundle ids that stay visible: Veil App, Control Center when one of its
    /// extras the system item ids miss sits right of the wall, and every app
    /// whose icon sits right of the wall. The last needs Accessibility;
    /// without it those apps hide too.
    ///
    /// Not keepable at all, so not listed: Spotlight (no Spotlight process
    /// draws it on macOS 27; tried with its bundle and system ids 0..4096),
    /// and a build without a bundle id (`tauri dev`), since macOS matches the
    /// list by bundle id, so a dev run hides its own icon too.
    fn kept_bundle_ids() -> Vec<String> {
        let own = NSRunningApplication::currentApplication()
            .bundleIdentifier()
            .map(|b| b.to_string());
        let control_center = wall_position()
            .is_some_and(|wall| has_item_right_of(wall, CONTROL_CENTER, &COVERED_EXTRAS))
            .then(|| CONTROL_CENTER.to_string());
        let right_of_wall = wall_span()
            .map(|(_, right)| crate::apps::bundle_ids_with_icon_from(right))
            .unwrap_or_default();
        own.into_iter().chain(control_center).chain(right_of_wall).collect()
    }

    pub fn set_hidden(hidden: bool) {
        if !hidden {
            if let Some(held) = HELD.lock().unwrap().take() {
                invalidate(held);
            }
            return;
        }
        let items: Vec<_> = SYSTEM_ITEMS.map(NSNumber::new_i32).collect();
        let items = NSArray::from_retained_slice(&items);
        let bundles: Vec<_> = kept_bundle_ids().iter().map(|b| NSString::from_str(b)).collect();
        let bundles = NSArray::from_retained_slice(&bundles);

        // `available()` checked both classes exist.
        let config_class = AnyClass::get(CONFIGURATION).unwrap();
        let assertion_class = AnyClass::get(ASSERTION).unwrap();
        let config: Allocated<AnyObject> = unsafe { msg_send![config_class, alloc] };
        let config: Option<Retained<AnyObject>> = unsafe {
            msg_send![config, initWithAllowedSystemItems: &*items, allowedBundleIdentifiers: &*bundles]
        };
        let assertion: Option<Retained<AnyObject>> = unsafe { msg_send![assertion_class, new] };
        let (Some(config), Some(assertion)) = (config, assertion) else {
            eprintln!("menu bar: could not create the hide list");
            HIDDEN.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        };

        let pending = assertion.clone();
        let done = RcBlock::new(move |error: *mut AnyObject| {
            let assertion = Assertion(pending.clone());
            if !error.is_null() {
                // Fail open: nothing is hidden, so say so.
                // ponytail: the panel label stays on "Show icons" until the
                // next click or reopen; emit an event here if that bites.
                eprintln!("menu bar: macOS refused the hide list");
                HIDDEN.store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
            // Shown again while macOS was still answering.
            if !HIDDEN.load(std::sync::atomic::Ordering::SeqCst) {
                return invalidate(assertion);
            }
            // A newer list replaces the one held before it.
            if let Some(old) = HELD.lock().unwrap().replace(assertion) {
                invalidate(old);
            }
        });
        let _: () = unsafe {
            msg_send![&*assertion, activateWithConfiguration: &*config, completionHandler: &*done]
        };
    }
}

/// The wall's left and right edges in global screen points. An icon left of
/// the right edge is one the wall hides; see `seed_positions` for the layout.
/// Must be called on the main thread.
pub fn wall_span() -> Option<(f64, f64)> {
    let wall = WALL.lock().unwrap();
    let item = &wall.as_ref()?.0;
    // Plain selectors, as the typed `button`/`window` accessors would pull
    // four more objc2-app-kit features in for one frame.
    let button: Option<Retained<NSObject>> = unsafe { msg_send![&**item, button] };
    let window: Option<Retained<NSObject>> = unsafe { msg_send![&*button?, window] };
    let frame: NSRect = unsafe { msg_send![&*window?, frame] };
    Some((frame.origin.x, frame.origin.x + frame.size.width))
}

#[cfg(test)]
mod tests {
    use objc2_foundation::NSProcessInfo;

    /// Fails when a macOS release renames or drops the private API, so the
    /// fallback to the wall (which hides nothing there) gets noticed.
    #[test]
    fn native_hiding_resolves_on_macos_27() {
        let major = NSProcessInfo::processInfo().operatingSystemVersion().majorVersion;
        assert_eq!(super::native::available(), major >= 27);
    }
}
