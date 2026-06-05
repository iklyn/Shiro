//! macOS native input + clipboard helpers — **Accessibility permission only,
//! never Automation.**
//!
//! Text/URL capture used to shell out to `osascript … keystroke "c"`, which made
//! macOS pop a *separate* "Shiro wants to control System Events" Automation
//! consent the first time you captured in a browser/Electron app — a surprise
//! second prompt on top of the Accessibility one we already ask for. Posting the
//! keystrokes with **CGEvent** instead uses only the Accessibility permission, so
//! there is never a second prompt.
//!
//! The clipboard is read through **NSPasteboard**, polling the actual string
//! *contents* (not `changeCount`) — that's the bit that matters: an app bumps the
//! change count when it *declares* the pasteboard types, a beat before the data is
//! actually written, so reading on the count race-reads an empty string. Polling
//! the contents until they're non-empty is what makes this reliable.
#![cfg(target_os = "macos")]

use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

// ── CGEvent synthetic keystrokes (Accessibility permission only) ─────────────

/// Virtual key codes (ANSI layout positions; layout-independent for these keys).
pub const KEY_C: u16 = 8;
pub const KEY_L: u16 = 37;
pub const KEY_ESCAPE: u16 = 53;

const FLAG_COMMAND: u64 = 0x0010_0000; // kCGEventFlagMaskCommand
const HID_TAP: u32 = 0; // kCGHIDEventTap

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        keycode: u16,
        keydown: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

fn post_key(keycode: u16, flags: u64) {
    unsafe {
        for keydown in [true, false] {
            let ev = CGEventCreateKeyboardEvent(std::ptr::null_mut(), keycode, keydown);
            if ev.is_null() {
                continue;
            }
            if flags != 0 {
                CGEventSetFlags(ev, flags);
            }
            CGEventPost(HID_TAP, ev);
            CFRelease(ev as *const c_void);
        }
    }
}

/// Press ⌘+<key> (e.g. ⌘C, ⌘L). Accessibility permission only — no Automation.
pub fn send_cmd_key(keycode: u16) {
    post_key(keycode, FLAG_COMMAND);
}

/// Press a bare key (e.g. Esc).
pub fn send_key(keycode: u16) {
    post_key(keycode, 0);
}

// ── Objective-C / NSPasteboard plumbing ──────────────────────────────────────

/// RAII autorelease pool so the short-lived NSString/NSArray temporaries each
/// pasteboard call creates don't leak (capture polls the clipboard many times).
struct Pool(*mut Object);
impl Pool {
    fn new() -> Self {
        unsafe {
            match Class::get("NSAutoreleasePool") {
                Some(cls) => Pool(msg_send![cls, new]),
                None => Pool(std::ptr::null_mut()),
            }
        }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        unsafe {
            let _: () = msg_send![self.0, drain];
        }
    }
}

unsafe fn nsstring(s: &str) -> *mut Object {
    let Ok(c) = CString::new(s) else {
        return std::ptr::null_mut();
    };
    match Class::get("NSString") {
        Some(cls) => msg_send![cls, stringWithUTF8String: c.as_ptr()],
        None => std::ptr::null_mut(),
    }
}

unsafe fn rust_string(s: *mut Object) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let c: *const c_char = msg_send![s, UTF8String];
    if c.is_null() {
        return None;
    }
    Some(CStr::from_ptr(c).to_string_lossy().into_owned())
}

unsafe fn pasteboard() -> Option<*mut Object> {
    let cls = Class::get("NSPasteboard")?;
    let pb: *mut Object = msg_send![cls, generalPasteboard];
    if pb.is_null() {
        None
    } else {
        Some(pb)
    }
}

unsafe fn string_for_type(uti: &str) -> Option<String> {
    let pb = pasteboard()?;
    let ty = nsstring(uti);
    if ty.is_null() {
        return None;
    }
    let s: *mut Object = msg_send![pb, stringForType: ty];
    rust_string(s)
}

/// Raw bytes of a pasteboard flavour (some apps store HTML as data that
/// `stringForType:` refuses to coerce to a string).
unsafe fn data_for_type(uti: &str) -> Option<Vec<u8>> {
    let pb = pasteboard()?;
    let ty = nsstring(uti);
    if ty.is_null() {
        return None;
    }
    let data: *mut Object = msg_send![pb, dataForType: ty];
    if data.is_null() {
        return None;
    }
    let len: usize = msg_send![data, length];
    if len == 0 {
        return None;
    }
    let ptr: *const u8 = msg_send![data, bytes];
    if ptr.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts(ptr, len).to_vec())
}

// ── Public clipboard primitives ──────────────────────────────────────────────

/// Plain-text flavour of the clipboard, if any.
pub fn clipboard_text() -> Option<String> {
    let _pool = Pool::new();
    unsafe { string_for_type("public.utf8-plain-text") }
}

/// HTML flavour of the clipboard, if any. Browsers vary in how they label it, so
/// try the modern UTI, the legacy Carbon type, and a raw-bytes fallback. This is
/// what carries rich-text formatting (the frontend turns it into Markdown).
pub fn clipboard_html() -> Option<String> {
    let _pool = Pool::new();
    unsafe {
        for ty in ["public.html", "Apple HTML pasteboard type"] {
            if let Some(s) = string_for_type(ty) {
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
            if let Some(bytes) = data_for_type(ty) {
                let s = String::from_utf8_lossy(&bytes).into_owned();
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

/// Empty the clipboard (so a following ⌘C can be detected by content polling).
pub fn clear_clipboard() {
    let _pool = Pool::new();
    unsafe {
        if let Some(pb) = pasteboard() {
            let _: i64 = msg_send![pb, clearContents];
        }
    }
}

/// Replace the clipboard with plain text (used to restore what we cleared).
pub fn set_clipboard_text(text: &str) {
    let _pool = Pool::new();
    unsafe {
        if let Some(pb) = pasteboard() {
            let _: i64 = msg_send![pb, clearContents];
            let ns = nsstring(text);
            let ty = nsstring("public.utf8-plain-text");
            if !ns.is_null() && !ty.is_null() {
                let _: bool = msg_send![pb, setString: ns forType: ty];
            }
        }
    }
}

/// Does the clipboard currently hold an image flavour (PNG/TIFF)?
pub fn clipboard_has_image() -> bool {
    let _pool = Pool::new();
    unsafe {
        let Some(pb) = pasteboard() else {
            return false;
        };
        let png = nsstring("public.png");
        let tiff = nsstring("public.tiff");
        if png.is_null() || tiff.is_null() {
            return false;
        }
        let Some(arr_cls) = Class::get("NSArray") else {
            return false;
        };
        let objs = [png, tiff];
        let arr: *mut Object =
            msg_send![arr_cls, arrayWithObjects: objs.as_ptr() count: objs.len()];
        if arr.is_null() {
            return false;
        }
        let avail: *mut Object = msg_send![pb, availableTypeFromArray: arr];
        !avail.is_null()
    }
}

/// Read an image off the clipboard as PNG bytes, if any (e.g. after "Copy Image"
/// in a browser, or copying from Preview/Photos). Returns None when there's no
/// image flavour. Uses NSImage → TIFF → NSBitmapImageRep → PNG. AppleScript
/// can't read image bytes cleanly, which is why this stays native.
pub fn read_image_png() -> Option<Vec<u8>> {
    let _pool = Pool::new();
    unsafe {
        let pb = pasteboard()?;
        let img_cls = Class::get("NSImage")?;
        let img: *mut Object = msg_send![img_cls, alloc];
        let img: *mut Object = msg_send![img, initWithPasteboard: pb];
        if img.is_null() {
            return None;
        }
        // Copy the bytes out before releasing the image / draining the pool.
        let bytes = (|| {
            let tiff: *mut Object = msg_send![img, TIFFRepresentation];
            if tiff.is_null() {
                return None;
            }
            let rep_cls = Class::get("NSBitmapImageRep")?;
            let rep: *mut Object = msg_send![rep_cls, imageRepWithData: tiff];
            if rep.is_null() {
                return None;
            }
            let dict_cls = Class::get("NSDictionary")?;
            let props: *mut Object = msg_send![dict_cls, dictionary];
            // NSBitmapImageFileTypePNG = 4.
            let png: *mut Object = msg_send![rep, representationUsingType: 4u64 properties: props];
            if png.is_null() {
                return None;
            }
            let len: usize = msg_send![png, length];
            if len == 0 {
                return None;
            }
            let ptr: *const u8 = msg_send![png, bytes];
            if ptr.is_null() {
                return None;
            }
            Some(std::slice::from_raw_parts(ptr, len).to_vec())
        })();
        let _: () = msg_send![img, release];
        bytes
    }
}

// ── Compound capture helpers ─────────────────────────────────────────────────

/// Grab the frontmost browser's URL: ⌘L selects the address bar, ⌘C copies it,
/// Esc returns to the page. Keystrokes via CGEvent (Accessibility only). Polls
/// the clipboard contents so we don't read before the copy lands, then restores
/// whatever was on the clipboard.
pub fn browser_url() -> Option<String> {
    use std::time::Duration;

    let saved = clipboard_text();
    send_cmd_key(KEY_L);
    // Let the address bar take focus and select its text before copying.
    std::thread::sleep(Duration::from_millis(120));
    clear_clipboard();
    send_cmd_key(KEY_C);

    let mut url = None;
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        if let Some(s) = clipboard_text() {
            let t = s.trim().to_string();
            if !t.is_empty() {
                url = Some(t);
                break;
            }
        }
    }

    send_key(KEY_ESCAPE);
    if let Some(s) = saved {
        set_clipboard_text(&s);
    }
    url.filter(|u| u.starts_with("http"))
}
