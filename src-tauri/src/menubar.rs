use objc2::rc::Retained;
use objc2_app_kit::{NSStatusBar, NSStatusItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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
