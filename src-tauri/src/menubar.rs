use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::Retained;
use objc2_app_kit::{NSStatusBar, NSStatusItem};
use objc2_foundation::NSString;
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
}

fn position_key(autosave_name: &str) -> CFString {
    CFString::new(&format!("NSStatusItem Preferred Position {autosave_name}"))
}

fn read_position(app_id: CFStringRef, autosave_name: &str) -> Option<f64> {
    let key = position_key(autosave_name);
    let value = unsafe { CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), app_id) };
    if value.is_null() {
        return None;
    }
    unsafe { CFType::wrap_under_create_rule(value) }
        .downcast::<CFNumber>()
        .and_then(|n| n.to_f64())
}

/// Seed where macOS places our tray icon and the wall, before they get their
/// autosave names. A preferred position is the item's distance from the
/// right edge of the screen; a larger value sits further left. Placing both
/// just left of Spotlight yields `… [wall][Veil App] Spotlight ⋯ Clock`, so
/// expanding the wall hides everything except Veil App and the system items
/// to its right (Spotlight, Control Center, Clock).
///
/// Only seeded when missing or broken (wall not left of the icon), so a
/// user's own Cmd-drag arrangement survives relaunches.
fn seed_positions() {
    let own = unsafe { kCFPreferencesCurrentApplication };
    let icon = read_position(own, TRAY_AUTOSAVE);
    let wall = read_position(own, WALL_AUTOSAVE);
    if let (Some(icon), Some(wall)) = (icon, wall) {
        if wall > icon {
            return;
        }
    }
    let spotlight = CFString::from_static_string("com.apple.Spotlight");
    let base = read_position(spotlight.as_concrete_TypeRef(), "Item-0").unwrap_or(0.0);
    for (name, pos) in [(TRAY_AUTOSAVE, base + 1.0), (WALL_AUTOSAVE, base + 2.0)] {
        let key = position_key(name);
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
    let hidden = !HIDDEN.load(Ordering::SeqCst);
    HIDDEN.store(hidden, Ordering::SeqCst);
    if let Some(item) = WALL.lock().unwrap().as_ref() {
        item.0.setLength(if hidden { WALL_EXPANDED } else { WALL_COLLAPSED });
        log::info!(
            "DEBUG toggle hidden={hidden} len={} visible={} main={}",
            item.0.length(),
            item.0.isVisible(),
            objc2::MainThreadMarker::new().is_some()
        );
    } else {
        log::info!("DEBUG toggle: no wall");
    }
    hidden
}
