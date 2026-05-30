//! macOS Accessibility (AX) helpers: trust checks + reading the current
//! selection straight from the focused UI element (no clipboard, no keystroke).
//! Both reading `AXSelectedText` and simulating Cmd+C require the *same*
//! Accessibility permission — granting it once covers everything.

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use std::os::raw::c_void;

type AXUIElementRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
}

/// Whether Shiro currently has Accessibility permission. No prompt, no side effects.
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// The frontmost app's (localizedName, pid) via NSWorkspace — in-process and
/// instant, replacing an osascript spawn on the capture hot path.
pub fn frontmost_app() -> Option<(String, i32)> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::CStr;
    use std::os::raw::c_char;
    unsafe {
        let cls = Class::get("NSWorkspace")?;
        let ws: *mut Object = msg_send![cls, sharedWorkspace];
        if ws.is_null() {
            return None;
        }
        let app: *mut Object = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        let name_ns: *mut Object = msg_send![app, localizedName];
        let name = if name_ns.is_null() {
            String::new()
        } else {
            let c: *const c_char = msg_send![name_ns, UTF8String];
            if c.is_null() {
                String::new()
            } else {
                CStr::from_ptr(c).to_string_lossy().into_owned()
            }
        };
        Some((name, pid))
    }
}

/// Add Shiro to the Accessibility list and show the system permission dialog
/// (with an "Open System Settings" button). Returns the current trust state.
pub fn prompt_trust() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let dict = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef())
    }
}

/// Read the selected text from the system-wide focused element. Returns `None`
/// when nothing is selected or the focused app doesn't expose it (most browser
/// / Electron web content) — in which case the caller falls back to Cmd+C.
pub fn selected_text() -> Option<String> {
    if !is_trusted() {
        return None;
    }
    unsafe {
        let system_ref = AXUIElementCreateSystemWide();
        if system_ref.is_null() {
            return None;
        }
        // Wrap so CoreFoundation releases these for us on drop.
        let _system = CFType::wrap_under_create_rule(system_ref as CFTypeRef);

        let focused_attr = CFString::new("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(
            system_ref,
            focused_attr.as_concrete_TypeRef(),
            &mut focused,
        ) != 0
            || focused.is_null()
        {
            return None;
        }
        let focused_cf = CFType::wrap_under_create_rule(focused);

        let sel_attr = CFString::new("AXSelectedText");
        let mut val: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(
            focused_cf.as_concrete_TypeRef() as AXUIElementRef,
            sel_attr.as_concrete_TypeRef(),
            &mut val,
        ) != 0
            || val.is_null()
        {
            return None;
        }

        let s = CFString::wrap_under_create_rule(val as CFStringRef).to_string();
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}
