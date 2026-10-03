//! Accessibility permission helpers (System Settings → Privacy & Security →
//! Accessibility).

use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
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
