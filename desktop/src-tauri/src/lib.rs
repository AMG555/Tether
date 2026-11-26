mod commands;
mod diagnostics;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::Manager as _;

// ── Windows clipboard poller ─────────────────────────────────────────────────
// `WH_KEYBOARD_LL` only captures keystrokes; Ctrl+V paste is a clipboard
// operation and never fires the hook with the pasted text — only with the bare
// "V" key while Ctrl is held, which the hook drops.  We therefore run a
// lightweight background poller that tracks `GetClipboardSequenceNumber()` and,
// on change, reads `CF_UNICODETEXT` from the clipboard and injects the text as
// `EV_PASTE` events into the same engine sink the hook uses.
//
// Design choices:
// - 150 ms poll interval: fast enough to catch paste-then-crash, slow enough to
//   not chew CPU.  The clipboard sequence number is a simple counter read, so
//   the cost of the check itself is a single Win32 call (~1 µs).
// - On first run we snapshot the initial sequence number so we do NOT ingest
//   whatever happened to be in the clipboard before Tether started.
// - Text longer than 60 chars is split into 60-char chunks so each chunk fits
//   inside the ring's 64-byte text slot without truncation (same logic as
//   passive.rs).
// - We only record CF_UNICODETEXT.  Binary clipboard data (images, HTML) is
//   ignored — it has no crash-recovery value as text.
// - We skip clipboard changes where the text is identical to the previous paste
//   so that focus-switching apps that re-assert the clipboard every few seconds
//   don't duplicate the saved text.
#[cfg(windows)]
mod clipboard_poller {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tether::ring::{Event, EV_PASTE};

    type Handle = *mut c_void;

    const CF_UNICODETEXT: u32 = 13;

    #[link(name = "user32")]
    extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
        fn OpenClipboard(hwnd: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(hmem: Handle) -> Handle;
        fn GlobalUnlock(hmem: Handle) -> i32;
        fn GlobalSize(hmem: Handle) -> usize;
    }

    /// Reads the current CF_UNICODETEXT clipboard contents.
    /// Returns `None` if the clipboard is empty, unavailable, or non-text.
    pub fn read_clipboard_text() -> Option<String> {
        // Yield briefly (40 ms) so the copying application can finish populating
        // and close its clipboard handle. This avoids clipboard lock contention
        // that can cause third-party apps to crash or report "Cannot open clipboard".
        std::thread::sleep(std::time::Duration::from_millis(40));

        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
                return None;
            }
            // Retry OpenClipboard up to 3 times in case the source app is still releasing it
            let mut opened = false;
            for _ in 0..3 {
                if OpenClipboard(std::ptr::null_mut()) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            if !opened {
                return None;
            }
            let hmem = GetClipboardData(CF_UNICODETEXT);
            if hmem.is_null() {
                CloseClipboard();
                return None;
            }
            let raw = GlobalLock(hmem);
            if raw.is_null() {
                CloseClipboard();
                return None;
            }
            // The GlobalSize includes the NUL terminator(s).
            let size_bytes = GlobalSize(hmem);
            let max_wchars = size_bytes / 2;
            // Cast to u16 pointer and find the NUL terminator for the actual length.
            let ptr = raw as *const u16;
            let mut len = 0usize;
            while len < max_wchars && *ptr.add(len) != 0 {
                len += 1;
            }
            let text = if len > 0 {
                Some(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)))
            } else {
                None
            };
            GlobalUnlock(hmem);
            CloseClipboard();
            text
        }
    }

    /// Splits `s` into chunks of at most `n` Unicode scalar values so each
    /// chunk fits in the ring's text slot.
    fn chunk_chars(s: &str, n: usize) -> Vec<String> {
        if s.chars().count() <= n {
            return vec![s.to_string()];
        }
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in s.chars() {
            cur.push(c);
            if cur.chars().count() >= n {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    /// Runs the clipboard poller until `stop` is set.
    /// Any clipboard text change is delivered to `sink` as one or more
    /// `EV_PASTE` events (chunked to fit the slot payload).
    pub fn run(
        sink: Arc<dyn Fn(Event) + Send + Sync>,
        stop: &AtomicBool,
        app_name_fn: impl Fn() -> String,
    ) {
        // Snapshot the sequence number at startup so we don't ingest pre-existing
        // clipboard contents as if the user just pasted them.
        let mut last_seq = unsafe { GetClipboardSequenceNumber() };
        let mut last_text: Option<String> = None;

        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let seq = unsafe { GetClipboardSequenceNumber() };
            if seq == last_seq {
                continue;
            }
            last_seq = seq;

            if let Some(text) = read_clipboard_text() {
                // Skip if the text is exactly what we already recorded
                // (some apps re-assert the same content on focus change).
                if last_text.as_deref() == Some(&text) {
                    continue;
                }
                last_text = Some(text.clone());
                let app = app_name_fn();
                for chunk in chunk_chars(&text, 60) {
                    if chunk.is_empty() {
                        continue;
                    }
                    let mut ev = Event::new(EV_PASTE, &app);
                    ev.set_text(&chunk);
                    sink(ev);
                }
            }
        }
    }
}

// ── macOS clipboard poller ───────────────────────────────────────────────────
// Same problem as Windows: CGEventTap fires for Cmd+V with keycode for "v",
// and `CGEventKeyboardGetUnicodeString` returns the character "v" (not the
// pasted content). We therefore poll `[NSPasteboard generalPasteboard].changeCount`
// every 150 ms and read the string value when it changes — identical strategy
// to the Windows poller, different OS API.
//
// We call into Objective-C runtime directly (no extra crate) using the same
// `extern "C"` approach as the CGEventTap code already does.
#[cfg(target_os = "macos")]
mod clipboard_poller_mac {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tether::ring::{Event, EV_PASTE};

    type Id = *mut c_void;

    // Minimal Objective-C runtime symbols — already available on macOS, no extra linking.
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {
        fn objc_getClass(name: *const i8) -> Id;
        fn sel_registerName(name: *const i8) -> Id;
        fn objc_msgSend(receiver: Id, sel: Id, ...) -> Id;
    }

    /// Returns the current NSPasteboard changeCount (a monotonic integer that
    /// increments every time clipboard contents change).
    fn pasteboard_change_count() -> i64 {
        unsafe {
            let cls_name = c"NSPasteboard";
            let sel_gen = c"generalPasteboard";
            let sel_cc = c"changeCount";
            let cls = objc_getClass(cls_name.as_ptr());
            if cls.is_null() {
                return 0;
            }
            let pb = objc_msgSend(cls, sel_registerName(sel_gen.as_ptr()));
            if pb.is_null() {
                return 0;
            }
            // changeCount returns NSInteger (i64 on 64-bit).
            // We transmute through a function pointer to get the right ABI.
            type MsgSendI64 = unsafe extern "C" fn(Id, Id) -> i64;
            let f: MsgSendI64 = std::mem::transmute(objc_msgSend as usize);
            f(pb, sel_registerName(sel_cc.as_ptr()))
        }
    }

    /// Reads the current plaintext string from the general pasteboard.
    /// Returns `None` if the clipboard contains no text.
    pub fn read_clipboard_text() -> Option<String> {
        unsafe {
            let cls_name = c"NSPasteboard";
            let sel_gen = c"generalPasteboard";
            let sel_str = c"stringForType:";
            let sel_pt = c"NSPasteboardTypeString"; // type name as string class
                                                    // Build the NSString for the pasteboard type.
            let nss_cls = objc_getClass(c"NSString".as_ptr());
            if nss_cls.is_null() {
                return None;
            }
            // [NSString stringWithUTF8String:] — selector
            let sel_swu = sel_registerName(c"stringWithUTF8String:".as_ptr());
            type MsgSendPtrStr = unsafe extern "C" fn(Id, Id, *const i8) -> Id;
            let make_str: MsgSendPtrStr = std::mem::transmute(objc_msgSend as usize);
            let type_nsstr = make_str(nss_cls, sel_swu, sel_pt.as_ptr() as *const i8);
            if type_nsstr.is_null() {
                return None;
            }

            let cls = objc_getClass(cls_name.as_ptr());
            let pb = objc_msgSend(cls, sel_registerName(sel_gen.as_ptr()));
            if pb.is_null() {
                return None;
            }
            type MsgSendPtrId = unsafe extern "C" fn(Id, Id, Id) -> Id;
            let get_str: MsgSendPtrId = std::mem::transmute(objc_msgSend as usize);
            let ns_str = get_str(pb, sel_registerName(sel_str.as_ptr()), type_nsstr);
            if ns_str.is_null() {
                return None;
            }
            // Convert NSString → &CStr → Rust String
            let sel_utf = sel_registerName(c"UTF8String".as_ptr());
            type MsgSendConstChar = unsafe extern "C" fn(Id, Id) -> *const i8;
            let utf8: MsgSendConstChar = std::mem::transmute(objc_msgSend as usize);
            let ptr = utf8(ns_str, sel_utf);
            if ptr.is_null() {
                return None;
            }
            Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }

    fn chunk_chars(s: &str, n: usize) -> Vec<String> {
        if s.chars().count() <= n {
            return vec![s.to_string()];
        }
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in s.chars() {
            cur.push(c);
            if cur.chars().count() >= n {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    pub fn run(
        sink: Arc<dyn Fn(Event) + Send + Sync>,
        stop: &AtomicBool,
        app_name_fn: impl Fn() -> String,
    ) {
        let mut last_count = pasteboard_change_count();
        let mut last_text: Option<String> = None;

        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let count = pasteboard_change_count();
            if count == last_count {
                continue;
            }
            last_count = count;

            if let Some(text) = read_clipboard_text() {
                if last_text.as_deref() == Some(&text) {
                    continue;
                }
                last_text = Some(text.clone());
                let app = app_name_fn();
                for chunk in chunk_chars(&text, 60) {
                    if chunk.is_empty() {
                        continue;
                    }
                    let mut ev = Event::new(EV_PASTE, &app);
                    ev.set_text(&chunk);
                    sink(ev);
                }
            }
        }
    }
}

/// Shared capture state, held for the lifetime of the Tauri process.
///
/// Design: the Engine lives here (not in commands.rs) so the capture loop
/// and the UI command handlers share the same in-process vault and ring.
/// Commands that only need vault access open their own handle via
/// `open_local_vault()` to avoid holding the engine lock across IPC calls.
pub struct AppState {
    /// The running capture engine. `None` only during the brief window
    /// between app start and the first `start_capture` call if capture
    /// failed to initialise (e.g. vault key missing on first run).
    pub engine: Option<Arc<tether::engine::Engine>>,
    /// Flipped to `true` to stop the capture and flush threads cleanly.
    pub stop: Arc<AtomicBool>,
    /// Whether the keyboard hook thread is currently running.
    pub capturing: Arc<AtomicBool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            engine: None,
            stop: Arc::new(AtomicBool::new(false)),
            capturing: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn run() -> Result<(), tauri::Error> {
    // Prevent WebView2 from flashing a solid white backing surface during cold
    // boot or window moves on transparent overlays (Alpha = 0). The main window
    // paints an opaque CSS background immediately so it is unaffected.
    #[cfg(target_os = "windows")]
    if std::env::var_os("WEBVIEW2_DEFAULT_BACKGROUND_COLOR").is_none() {
        std::env::set_var("WEBVIEW2_DEFAULT_BACKGROUND_COLOR", "0");
    }

    // Configure WebView2 browser arguments to minimise RAM, GPU memory overhead,
    // and idle CPU consumption across all child webview processes.
    #[cfg(target_os = "windows")]
    if std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").is_none() {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,TranslateUI,OptimizationHints \
             --enable-low-end-device-mode \
             --disable-renderer-backgrounding \
             --disable-background-networking",
        );
    }

    tauri::Builder::default()
        .manage(std::sync::Mutex::new(AppState::new()))
        // Intercept the X-button: hide the main window instead of closing it.
        // The process keeps running so the in-process capture stays active.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Start the capture engine on the main Tauri setup thread.
            // Any failure here is non-fatal: the UI still loads and shows the
            // vault; it will display a "capture not running" status to the user.
            let dir = tether::default_data_dir();
            let key_file = dir.join("key.txt");

            // ── First-run auto-bootstrap ──────────────────────────────────────
            // If the vault key does not exist yet, create it now — no user action
            // required. This ensures capture starts immediately on first launch
            // without requiring a separate `tether init` step.
            if !key_file.exists() {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    eprintln!("tether: cannot create data dir on first run: {e}");
                } else {
                    match tether::crypto::SecretKey::random() {
                        Ok(key) => {
                            let hex = key.to_hex();
                            if let Err(e) = std::fs::write(&key_file, &hex) {
                                eprintln!("tether: cannot write key on first run: {e}");
                            } else {
                                let _ = tether::vault::set_private(&key_file);
                            }
                        }
                        Err(e) => eprintln!("tether: cannot generate key on first run: {e}"),
                    }
                }
            }

            // ── Auto-install startup entry ────────────────────────────────────
            // The whole value proposition of Tether is "already running when the
            // crash happens". Silently install the per-user login/startup item on
            // first run so the user is protected from the next boot onwards. The
            // setting checkbox in Settings reflects the actual installed state,
            // and the user can remove it at any time.
            if let Ok(already) = commands::get_autostart_status() {
                if !already {
                    if let Ok(current_exe) = std::env::current_exe() {
                        let _ = tether::setup::install_autostart(&current_exe);
                    }
                }
            }

            let started = if key_file.exists() {
                match commands::open_local_vault() {
                    Ok(vault) => {
                        match tether::engine::Engine::new(
                            vault,
                            4096,
                            tether::policy::Policy::default(),
                        ) {
                            Ok(engine) => {
                                // Replay anything in the crash buffer before accepting new input.
                                let _ = engine.recover();
                                // Load retention preference.
                                let (cfg, _) = tether::config::Config::load_or_default(&dir);
                                engine.set_retention_days(cfg.retention_days);

                                let engine = Arc::new(engine);
                                let stop = Arc::new(AtomicBool::new(false));
                                let capturing = Arc::new(AtomicBool::new(false));

                                // Flush thread: commits keystrokes to the vault every second.
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    std::thread::spawn(move || {
                                        e.run_commit_loop(&s, Duration::from_secs(1));
                                    });
                                }

                                // Capture thread: installs the OS keyboard hook.
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    let c = Arc::clone(&capturing);
                                    std::thread::spawn(move || {
                                        c.store(true, Ordering::Relaxed);
                                        let mut source = tether::capture::default_source();
                                        let sink: Arc<dyn Fn(tether::ring::Event) + Send + Sync> =
                                            Arc::new(move |ev| e.ingest(ev));
                                        source.run(sink, &s);
                                        c.store(false, Ordering::Relaxed);
                                    });
                                }

                                // Clipboard poller thread (Windows only): captures Ctrl+V
                                // paste text that the keyboard hook cannot see.
                                #[cfg(windows)]
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    std::thread::spawn(move || {
                                        let sink: Arc<dyn Fn(tether::ring::Event) + Send + Sync> =
                                            Arc::new(move |ev| e.ingest(ev));
                                        clipboard_poller::run(
                                            sink,
                                            &s,
                                            tether::capture::frontmost_app,
                                        );
                                    });
                                }

                                // Clipboard poller thread (macOS): captures Cmd+V paste text
                                // that CGEventTap sees only as the character "v".
                                #[cfg(target_os = "macos")]
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    std::thread::spawn(move || {
                                        let sink: Arc<dyn Fn(tether::ring::Event) + Send + Sync> =
                                            Arc::new(move |ev| e.ingest(ev));
                                        clipboard_poller_mac::run(
                                            sink,
                                            &s,
                                            tether::capture::frontmost_app,
                                        );
                                    });
                                }

                                // Store state for command handlers.
                                let state = app.state::<std::sync::Mutex<AppState>>();
                                if let Ok(mut guard) = state.lock() {
                                    guard.engine = Some(engine);
                                    guard.stop = stop;
                                    guard.capturing = capturing;
                                }
                                true
                            }
                            Err(_) => false,
                        }
                    }
                    Err(_) => false,
                }
            } else {
                // Key file still missing (creation failed above) — capture cannot start.
                eprintln!("tether: capture not started — vault key could not be created");
                false
            };
            if !started {
                eprintln!("tether: capture not started — check data directory permissions");
            }

            // Automatically display the companion overlay if active companion is visible.
            // This ensures the companion shows up immediately on system restart/login
            // without having to wait for the main window to open or load.
            let active = commands::get_active_companion();
            if active.visible {
                let _ = commands::show_companion_overlay(app.handle().clone());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_vault_status,
            commands::list_drafts,
            commands::get_draft,
            commands::get_revisions,
            commands::delete_draft,
            commands::prune_vault,
            commands::set_retention_days,
            commands::get_autostart_status,
            commands::set_autostart,
            commands::list_companions,
            commands::save_custom_companion,
            commands::delete_custom_companion,
            commands::install_catalog_companion,
            commands::uninstall_autostart,
            commands::purge_vault_data,
            commands::uninstall_app,
            commands::get_companion_clip,
            commands::get_system_status,
            commands::show_main_window,
            commands::show_companion_overlay,
            commands::hide_companion_overlay,
            commands::set_companion_passthrough,
            commands::set_hit_zone_region,
            commands::start_companion_drag,
            commands::get_capture_status,
            commands::get_active_companion,
            commands::set_active_companion,
            commands::check_for_updates,
            commands::install_update,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // Tauri 2 exits the process when the last window closes by default.
            // Prevent that so the in-process capture keeps running in the background.
            // The only intentional exit paths are:
            //   • Settings → Uninstall (calls std::process::exit directly)
            //   • OS kill / SIGTERM
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
    Ok(())
}
