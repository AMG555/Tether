//! macOS backend: `CGEventTap` (keystrokes) + Accessibility `probe()` (exact field reads).
//!
//! macOS is the strictest of the three platforms, and correctly so:
//! - A `CGEventTap` at `kCGSessionEventTap` needs **Input Monitoring** (`IOHIDRequestAccess`)
//!   and, for keyboard events, **Accessibility** trust. Both are user-granted in System
//!   Settings; Tether never asks for root and never installs a privileged helper.
//! - If the tap cannot be created (permission denied), `run()` returns immediately and the
//!   caller falls back to clipboard mode — it does not silently pretend to be capturing.
//! - The `probe()` path uses `AXUIElementCopyAttributeValue` on the focused element, which
//!   *does* expose the caret and the full string, so mid-string edits are captured exactly.
//!   `AXSecureTextField` is recognised and always refused, belt-and-braces on top of Policy.
//!
//! Unsigned-helper note: CFMachPort callbacks must return the event or it is consumed for
//! the whole system. We always return it — Tether must be invisible to the user's typing.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use crate::engine::{FieldSnapshot, Source};
use crate::ring::{Event, EV_BACKSPACE, EV_CHAR};

type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;
type AXUIElementRef = *mut c_void;
type CFStringRef = *const c_void;

const CG_SESSION_EVENT_TAP: u32 = 1;
const CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const CG_TAP_OPTION_DEFAULT: u32 = 0;
const CG_EVENT_KEY_DOWN: u64 = 1 << 10;
const CG_KEYBOARD_EVENT_KEYCODE: i64 = 9;
const K_CG_HID_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const K_CG_HID_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const KEYCODE_DELETE: i64 = 51;
const KEYCODE_RETURN: i64 = 36;
const AX_SECURE_ROLE: &str = "AXSecureTextField";

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: Option<
            unsafe extern "C" fn(CGEventTapProxy, u64, CGEventRef, *mut c_void) -> CGEventRef,
        >,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: i64) -> i64;
    fn CGEventKeyboardGetUnicodeString(
        event: CGEventRef,
        max_len: usize,
        actual_len: *mut usize,
        buf: *mut u16,
    ) -> ();

    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyAttributeValue2() -> ();
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        alloc: CFAllocatorRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;
    fn CFStringCreateWithCString(alloc: CFAllocatorRef, s: *const i8, encoding: u32)
        -> CFStringRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFStringGetCString(s: CFStringRef, buf: *mut i8, size: isize, encoding: u32) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> u64;
    fn CFStringGetTypeID() -> u64;
    fn CFBooleanGetValue(b: CFTypeRef) -> u8;
    fn CFBooleanGetTypeID() -> u64;
    fn CFStringGetLength(s: CFStringRef) -> isize;
}

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_RUN_LOOP_DEFAULT_MODE: &[u8] = b"kCFRunLoopDefaultMode\0";

static TX: Mutex<Option<Sender<Event>>> = Mutex::new(None);
static PROBE_USES_AX: AtomicBool = AtomicBool::new(true);

pub struct MacTap;

impl Source for MacTap {
    fn name(&self) -> &'static str {
        if unsafe { AXIsProcessTrusted() } != 0 {
            "macOS CGEventTap + accessibility"
        } else {
            "macOS CGEventTap (permission not granted — run `tether doctor`)"
        }
    }

    /// Exact read of the focused field. This is the high-fidelity path: it returns the
    /// field's *current* text, so nothing depends on replaying keystrokes.
    fn probe(&mut self) -> Option<FieldSnapshot> {
        if !PROBE_USES_AX.load(Ordering::Relaxed) || unsafe { AXIsProcessTrusted() } == 0 {
            return None;
        }
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let focused = ax_attr(system, "AXFocusedUIElement")?;
            let role = ax_string(focused, "AXRole").unwrap_or_default();
            let app_el = ax_attr(system, "AXFocusedApplication");
            let app =
                app_el.and_then(|a| ax_string(a, "AXTitle")).unwrap_or_else(|| "macos".to_string());
            let title = app_el
                .and_then(|a| ax_attr(a, "AXFocusedWindow"))
                .and_then(|w| ax_string(w, "AXTitle"))
                .unwrap_or_default();
            let is_password = role.contains(AX_SECURE_ROLE)
                || ax_string(focused, "AXSubrole").map(|s| s == AX_SECURE_ROLE).unwrap_or(false);
            let value = ax_string(focused, "AXValue").unwrap_or_default();
            if role.is_empty() {
                return None;
            }
            Some(FieldSnapshot { app, window_title: title, role, value, is_password })
        }
    }

    fn run(&mut self, sink: &dyn Fn(Event), stop: &AtomicBool) {
        let (tx, rx) = channel::<Event>();
        *TX.lock().unwrap() = Some(tx);
        let consumer = std::thread::spawn(move || {
            while let Ok(ev) = rx.recv() {
                sink(ev);
            }
        });

        unsafe {
            let tap = CGEventTapCreate(
                CG_SESSION_EVENT_TAP,
                CG_HEAD_INSERT_EVENT_TAP,
                CG_TAP_OPTION_DEFAULT,
                CG_EVENT_KEY_DOWN,
                Some(tap_callback),
                std::ptr::null_mut(),
            );
            if tap.is_null() {
                eprintln!(
                    "tether: CGEventTap unavailable. Grant Accessibility + Input Monitoring \
                     to rescue in System Settings > Privacy & Security, then rerun. \
                     Falling back to clipboard-only capture."
                );
                *TX.lock().unwrap() = None;
                let _ = consumer.join();
                return;
            }
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let rl = CFRunLoopGetCurrent();
            let mode = CFStringCreateWithCString(
                std::ptr::null(),
                K_CF_RUN_LOOP_DEFAULT_MODE.as_ptr() as *const i8,
                CF_STRING_ENCODING_UTF8,
            );
            CFRunLoopAddSource(rl, source, mode);

            while !stop.load(Ordering::Relaxed) {
                // Returns after ~100 ms even with no events, so we can observe `stop`.
                CFRunLoopRunInMode(mode, 0.1, 1);
            }
            CFRelease(source as CFTypeRef);
            CFRelease(tap as CFTypeRef);
            CFRelease(mode as CFTypeRef);
        }
        *TX.lock().unwrap() = None;
        let _ = consumer.join();
    }
}

unsafe extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    _type: u64,
    event: CGEventRef,
    _info: *mut c_void,
) -> CGEventRef {
    // Taps are disabled by the OS if they stall; re-enable defensively.
    let keycode = CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_KEYCODE);
    let app = frontmost_app();
    let ev = if keycode == KEYCODE_DELETE {
        Some(Event::new(EV_BACKSPACE, &app))
    } else if keycode == KEYCODE_RETURN {
        let mut e = Event::new(EV_CHAR, &app);
        e.set_text("\n");
        Some(e)
    } else {
        let mut buf = [0u16; 16];
        let mut len: usize = 0;
        CGEventKeyboardGetUnicodeString(event, buf.len(), &mut len, buf.as_mut_ptr());
        if len == 0 {
            None
        } else {
            let text = String::from_utf16_lossy(&buf[..len]);
            // Control characters other than newline are shortcuts, not prose.
            if text.chars().all(|c| c.is_control() && c != '\n') {
                None
            } else {
                let mut e = Event::new(EV_CHAR, &app);
                e.set_text(&text);
                Some(e)
            }
        }
    };
    if let Some(e) = ev {
        if let Ok(g) = TX.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(e);
            }
        }
    }
    // Never swallow the event: the user's typing must reach the real app untouched.
    event
}

unsafe fn ax_attr(el: AXUIElementRef, name: &str) -> Option<AXUIElementRef> {
    let attr = cfstr(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyAttributeValue(el, attr, &mut out);
    CFRelease(attr as CFTypeRef);
    if err != 0 || out.is_null() {
        None
    } else {
        Some(out as AXUIElementRef)
    }
}

unsafe fn ax_string(el: AXUIElementRef, name: &str) -> Option<String> {
    let v = ax_attr(el, name)?;
    let s = cfstring_to_rust(v as CFStringRef);
    CFRelease(v as CFTypeRef);
    s
}

unsafe fn cfstr(s: &str) -> CFStringRef {
    let c = std::ffi::CString::new(s).ok()?;
    CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), CF_STRING_ENCODING_UTF8)
}

unsafe fn cfstring_to_rust(s: CFStringRef) -> Option<String> {
    if s.is_null() || CFGetTypeID(s as CFTypeRef) != CFStringGetTypeID() {
        return None;
    }
    let len = CFStringGetLength(s) as usize;
    let mut buf = vec![0i8; len * 4 + 1];
    if CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, CF_STRING_ENCODING_UTF8) == 0 {
        return None;
    }
    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

/// Frontmost app name via the accessibility API (no subprocess, no Apple Events prompt).
pub fn frontmost_app() -> String {
    unsafe {
        if AXIsProcessTrusted() == 0 {
            return "macos".to_string();
        }
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return "macos".to_string();
        }
        ax_attr(system, "AXFocusedApplication")
            .and_then(|a| ax_string(a, "AXTitle"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "macos".to_string())
    }
}

/// `tether doctor` calls this to report precise permission state.
pub fn permission_status() -> (bool, bool) {
    let ax = unsafe { AXIsProcessTrusted() != 0 };
    // Input Monitoring has no reliable public query; a tap probe is the honest test.
    unsafe {
        let tap = CGEventTapCreate(
            CG_SESSION_EVENT_TAP,
            CG_HEAD_INSERT_EVENT_TAP,
            CG_TAP_OPTION_DEFAULT,
            CG_EVENT_KEY_DOWN,
            Some(tap_callback),
            std::ptr::null_mut(),
        );
        let tap_ok = !tap.is_null();
        if tap_ok {
            CFRelease(tap as CFTypeRef);
        }
        (ax, tap_ok)
    }
}
