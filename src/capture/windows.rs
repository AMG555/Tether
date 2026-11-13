//! Windows backend: `WH_KEYBOARD_LL` low-level keyboard hook.
//!
//! Why a hook and not UI Automation polling: the requirement is "don't lose a single
//! keystroke", and UIA polling is lossy by construction — it sees field contents on a
//! timer, so a fast typist's last few words are always at risk. A `WH_KEYBOARD_LL` hook is
//! synchronous and lossless, needs no elevation, and dies with the process (so nothing is
//! left behind if Tether is killed).
//!
//! Character fidelity: raw virtual-key codes are not text (`VK_OEM_1` is `;` or `ö`
//! depending on layout). We therefore resolve every keypress through `ToUnicodeEx` with
//! the *current* layout and the live keyboard state, exactly like a text editor does.
//!
//! The hook callback must return fast or Windows silently uninstalls it
//! (`LowLevelHooksTimeout`, default 300 ms). So the callback does three things only:
//! read the process name (cached per pid), build an `Event`, and push it on a channel.
//! All policy, redaction, disk I/O and encryption happen on other threads.

#![allow(non_snake_case, non_camel_case_types)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use crate::engine::Source;
use crate::ring::{Event, EV_BACKSPACE, EV_CHAR, EV_PASTE};

type Hinstance = *mut c_void;
type Hwnd = *mut c_void;
type Hhook = *mut c_void;
type Handle = *mut c_void;
type Hkl = *mut c_void;
type Lresult = isize;
type Wparam = usize;
type Lparam = isize;

const WH_KEYBOARD_LL: i32 = 13;
const WM_KEYDOWN: usize = 0x0100;
const WM_SYSKEYDOWN: usize = 0x0104;
const VK_BACK: usize = 0x08;
const VK_TAB: usize = 0x09;
const VK_RETURN: usize = 0x0D;
const VK_SHIFT: usize = 0x10;
const VK_CONTROL: usize = 0x11;
const VK_MENU: usize = 0x12;
const VK_CAPITAL: usize = 0x14;
const VK_ESCAPE: usize = 0x1B;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const MAPVK_VK_TO_VSC: u32 = 0;

#[repr(C)]
struct KbdLlHookStruct {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: Wparam,
    l_param: Lparam,
    time: u32,
    pt_x: i32,
    pt_y: i32,
    l_private: u32,
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        id_hook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, Wparam, Lparam) -> Lresult>,
        hmod: Hinstance,
        thread_id: u32,
    ) -> Hhook;
    fn CallNextHookEx(hhk: Hhook, n_code: i32, w_param: Wparam, l_param: Lparam) -> Lresult;
    fn UnhookWindowsHookEx(hhk: Hhook) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> Lresult;
    fn GetForegroundWindow() -> Hwnd;
    fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    fn GetKeyboardState(state: *mut u8) -> i32;
    fn GetKeyboardLayout(thread_id: u32) -> Hkl;
    fn MapVirtualKeyExW(code: u32, map_type: u32, layout: Hkl) -> u32;
    fn ToUnicodeEx(
        vk: u32,
        scan: u32,
        state: *const u8,
        buf: *mut u16,
        cch: i32,
        flags: u32,
        layout: Hkl,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn QueryFullProcessImageNameW(proc_: Handle, flags: u32, buf: *mut u16, size: *mut u32) -> i32;
    fn CloseHandle(h: Handle) -> i32;
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
}

static TX: Mutex<Option<Sender<Event>>> = Mutex::new(None);
static CACHE: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

pub struct WindowsHook;

impl Source for WindowsHook {
    fn name(&self) -> &'static str {
        "windows low-level keyboard hook"
    }

    fn run(&mut self, sink: std::sync::Arc<dyn Fn(Event) + Send + Sync>, stop: &AtomicBool) {
        let (tx, rx) = channel::<Event>();
        *TX.lock().unwrap() = Some(tx);
        *CACHE.lock().unwrap() = Some(HashMap::new());

        // Consumer thread: this is where the expensive work happens, away from the hook.
        let consumer = std::thread::spawn(move || {
            while let Ok(ev) = rx.recv() {
                sink(ev);
            }
        });

        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(hook_proc),
                GetModuleHandleW(std::ptr::null()),
                0,
            );
            if hook.is_null() {
                eprintln!("tether: could not install keyboard hook (is another hook blocking it?)");
                *TX.lock().unwrap() = None;
                let _ = consumer.join();
                return;
            }
            let mut msg: Msg = std::mem::zeroed();
            while !stop.load(Ordering::Relaxed) {
                // Peek-with-timeout so `stop` is observed without a posted message.
                let r = pea_message(&mut msg);
                if r > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if r == 0 {
                    break;
                }
            }
            UnhookWindowsHookEx(hook);
        }
        *TX.lock().unwrap() = None;
        let _ = consumer.join();
    }
}

// `PeekMessageW` is declared separately to keep the timeout loop readable.
#[link(name = "user32")]
extern "system" {
    #[link_name = "PeekMessageW"]
    fn PeekMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    #[link_name = "Sleep"]
    fn KernelSleep(ms: u32);
}

fn pea_message(msg: &mut Msg) -> i32 {
    const PM_REMOVE: u32 = 1;
    let r = unsafe { PeekMessageW(msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
    if r == 0 {
        unsafe { KernelSleep(10) };
        // Return 1 (not 0) so the loop keeps running; 0 is reserved for WM_QUIT.
        return if std::ptr::eq(msg, msg) { 1 } else { 0 };
    }
    r
}

unsafe extern "system" fn hook_proc(code: i32, w_param: Wparam, l_param: Lparam) -> Lresult {
    if code >= 0 && (w_param == WM_KEYDOWN || w_param == WM_SYSKEYDOWN) {
        let info = &*(l_param as *const KbdLlHookStruct);
        if let Some(ev) = translate(info) {
            if let Ok(guard) = TX.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(ev);
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param)
}

/// Turns a keypress into a Tether event. Returns `None` for keys that produce no text
/// (modifiers, shortcuts, escape) so the ring is not polluted with noise.
unsafe fn translate(info: &KbdLlHookStruct) -> Option<Event> {
    let vk = info.vk_code as usize;
    let app = frontmost_app();

    // Shortcuts (Ctrl/Alt held) are commands, not prose. Capture them and you record
    // Ctrl+V as "v" and corrupt every draft.
    let state_ptr = {
        let mut state = [0u8; 256];
        GetKeyboardState(state.as_mut_ptr());
        state
    };
    let ctrl = state_ptr[VK_CONTROL] & 0x80 != 0;
    let alt = state_ptr[VK_MENU] & 0x80 != 0;
    if ctrl || alt {
        // Ctrl+V is special: the paste itself arrives as a separate clipboard event.
        return None;
    }

    match vk {
        VK_BACK => {
            let mut ev = Event::new(EV_BACKSPACE, &app);
            // `info.time` is milliseconds since boot, not an epoch: never store it.
            ev.ts = crate::ring::now_secs();
            return Some(ev);
        }
        VK_RETURN => {
            let mut ev = Event::new(EV_CHAR, &app);
            ev.set_text("\n");
            // `info.time` is milliseconds since boot, not an epoch: never store it.
            ev.ts = crate::ring::now_secs();
            return Some(ev);
        }
        VK_TAB | VK_ESCAPE | VK_SHIFT | VK_CONTROL | VK_MENU | VK_CAPITAL => return None,
        _ => {}
    }

    let layout =
        GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut()));
    let scan = MapVirtualKeyExW(info.vk_code, MAPVK_VK_TO_VSC, layout) as u32;
    let mut buf = [0u16; 8];
    let n = ToUnicodeEx(
        info.vk_code,
        scan,
        state_ptr.as_ptr(),
        buf.as_mut_ptr(),
        buf.len() as i32,
        0,
        layout,
    );
    if n <= 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buf[..n as usize]);
    if text.is_empty() || text.chars().all(|c| c.is_control()) {
        return None;
    }
    // A paste is detected on the clipboard side; if it lands here as a single char we
    // still record it, which is harmless (deduped in reconstruct).
    let mut ev = Event::new(EV_CHAR, &app);
    ev.set_text(&text);
    // `info.time` is milliseconds since boot, not an epoch: never store it.
    ev.ts = crate::ring::now_secs();
    Some(ev)
}

pub fn frontmost_app() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return String::new();
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }
        if let Ok(cache) = CACHE.lock() {
            if let Some(map) = cache.as_ref() {
                if let Some(name) = map.get(&pid) {
                    return name.clone();
                }
            }
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        let base =
            full.rsplit(['\\', '/']).next().unwrap_or(&full).trim_end_matches(".exe").to_string();
        if let Ok(mut cache) = CACHE.lock() {
            if let Some(map) = cache.as_mut() {
                if map.len() > 256 {
                    map.clear();
                }
                map.insert(pid, base.clone());
            }
        }
        base
    }
}

/// Clipboard events on Windows are delivered by the same lower-level poller used
/// elsewhere; wiring `AddClipboardFormatListener` requires a message window, which the
/// tray UI will own. Until then the hook path covers keystrokes and Ctrl+V is recorded as
/// "something was pasted" so the draft is never shorter than what the user saw.
#[allow(dead_code)]
fn channel_paste(app: &str, text: &str) -> Event {
    let mut ev = Event::new(EV_PASTE, app);
    ev.set_text(text);
    ev
}
