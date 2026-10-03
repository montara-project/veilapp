use objc2::rc::Retained;
use objc2_app_kit::{NSStatusBar, NSStatusItem};
use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// A blank "wall" status item. Widening it pushes every menu bar icon to its
/// left out of the visible bar — the same public-API trick Hidden Bar uses.
/// macOS clips status items at the left edge of the status area, so anything
/// left of the wall simply disappears until the wall collapses again.
struct WallItem(Retained<NSStatusItem>);
// The wall is only ever touched from the main thread (setup + sync commands).
unsafe impl Send for WallItem {}
unsafe impl Sync for WallItem {}

static WALL: Mutex<Option<WallItem>> = Mutex::new(None);
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Narrow enough to stay visible so it can be Cmd-dragged into place.
const WALL_COLLAPSED: f64 = 8.0;
/// Wide enough to push the whole icon cluster off the bar.
const WALL_EXPANDED: f64 = 800.0;

/// Must be called on the main thread (AppKit requirement).
pub fn init() {
    let mut wall = WALL.lock().unwrap();
    if wall.is_some() {
        return;
    }
    let bar = NSStatusBar::systemStatusBar();
    *wall = Some(WallItem(bar.statusItemWithLength(WALL_COLLAPSED)));
}

/// Expand or collapse the wall. Returns `true` when icons are now hidden.
/// Must be called on the main thread.
pub fn toggle() -> bool {
    init();
    let hidden = !HIDDEN.load(Ordering::SeqCst);
    HIDDEN.store(hidden, Ordering::SeqCst);
    if let Some(item) = WALL.lock().unwrap().as_ref() {
        item.0.setLength(if hidden { WALL_EXPANDED } else { WALL_COLLAPSED });
    }
    hidden
}

// ---------------------------------------------------------------------------
// Per-icon hiding via the private SkyLight (CGS) framework.
//
// On macOS 26 every status-item window is hosted by the Control Center
// process, so hiding a SPECIFIC app's icon means touching that hosted window
// directly. CGSSetWindowAlpha on the main connection does exactly that —
// the same class of private API menu bar managers (Bartender, Ice) rely on.
// ---------------------------------------------------------------------------

type MainConnectionFn = unsafe extern "C" fn() -> u32;
type SetWindowAlphaFn = unsafe extern "C" fn(u32, u32, f64) -> i32;

struct Cgs {
    main_connection: MainConnectionFn,
    set_window_alpha: SetWindowAlphaFn,
}

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_NOW: c_int = 2;

fn cgs() -> Option<&'static Cgs> {
    static CGS: OnceLock<Option<Cgs>> = OnceLock::new();
    CGS
        .get_or_init(|| {
            unsafe {
                let path =
                    b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0";
                let handle = dlopen(path.as_ptr() as *const c_char, RTLD_NOW);
                if handle.is_null() {
                    return None;
                }
                let sym = |name: &[u8]| dlsym(handle, name.as_ptr() as *const c_char);
                let main_ptr = sym(b"CGSMainConnectionID\0");
                let alpha_ptr = sym(b"CGSSetWindowAlpha\0");
                if main_ptr.is_null() || alpha_ptr.is_null() {
                    return None;
                }
                let main_connection: MainConnectionFn = std::mem::transmute(main_ptr);
                let set_window_alpha: SetWindowAlphaFn = std::mem::transmute(alpha_ptr);
                Some(Cgs {
                    main_connection,
                    set_window_alpha,
                })
            }
        })
        .as_ref()
}

static ICONS_HIDDEN: AtomicBool = AtomicBool::new(false);
static HIDDEN_WINDOW_IDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Hide (alpha 0) or show (alpha 1) specific menu bar item windows.
/// Returns `true` when every CGS call succeeded.
pub fn set_icons_hidden(window_ids: &[u32], hidden: bool) -> bool {
    let Some(cgs) = cgs() else {
        return false;
    };
    let connection = unsafe { (cgs.main_connection)() };
    let alpha = if hidden { 0.0 } else { 1.0 };
    let mut ok = true;
    for &window_id in window_ids {
        let err = unsafe { (cgs.set_window_alpha)(connection, window_id, alpha) };
        if err != 0 {
            log::warn!("CGSSetWindowAlpha({window_id}) error {err}");
            ok = false;
        }
    }
    ICONS_HIDDEN.store(hidden, Ordering::SeqCst);
    *HIDDEN_WINDOW_IDS.lock().unwrap() = if hidden {
        window_ids.to_vec()
    } else {
        Vec::new()
    };
    ok
}

pub fn icons_hidden() -> bool {
    ICONS_HIDDEN.load(Ordering::SeqCst)
}

/// Window ids hidden by the current Hide state (may be stale after Control
/// Center re-lays-out; Show restores these plus the freshly computed ones).
pub fn hidden_window_ids() -> Vec<u32> {
    HIDDEN_WINDOW_IDS.lock().unwrap().clone()
}

/// Restore any icons we hid — called on app exit so the user's menu bar is
/// never left in a broken state.
pub fn restore_hidden_icons() {
    let ids = HIDDEN_WINDOW_IDS.lock().unwrap().clone();
    if !ids.is_empty() {
        set_icons_hidden(&ids, false);
    }
}
