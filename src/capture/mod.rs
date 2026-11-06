//! Platform capture backends, behind one trait.
//!
//! | platform | mechanism | permission needed | fidelity |
//! |---|---|---|---|
//! | Windows | `WH_KEYBOARD_LL` low-level hook | none (userland) | exact keystrokes |
//! | macOS | `CGEventTap` (session tap) | Accessibility + Input Monitoring | exact keystrokes |
//! | Linux X11 | passive: clipboard watch | none | clipboard-level |
//! | Linux Wayland | passive: clipboard watch | none | clipboard-level |
//!
//! Wayland deliberately forbids global keyboard hooks — that is a *good* security property
//! and Savepoint does not try to defeat it. On those systems Savepoint degrades to watching
//! the clipboard, which still rescues the most common disaster (you copied the half-prompt,
//! then crashed; or you never copied and the terminal buffer is gone).
//!
//! Every backend obeys the same contract: deliver events fast, never block, never panic on
//! hostile OS input, and drop anything `Policy` rejects before it reaches the ring.

use std::sync::atomic::AtomicBool;

use crate::ring::Event;

pub mod passive;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

/// What the capture layer is physically able to do on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Real keystroke events.
    Keystrokes,
    /// Clipboard changes only (no keystrokes available on this platform/permission set).
    Clipboard,
    /// Nothing available; commands like `list`/`get` still work.
    None,
}

impl Mode {
    pub fn describe(&self) -> &'static str {
        match self {
            Mode::Keystrokes => "keystrokes + clipboard",
            Mode::Clipboard => "clipboard only (global hooks unavailable here)",
            Mode::None => "inactive",
        }
    }
}

/// Returns the best backend available on this machine.
pub fn default_source() -> Box<dyn crate::engine::Source> {
    #[cfg(windows)]
    {
        return Box::new(windows::WindowsHook);
    }
    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::MacTap);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Box::new(passive::ClipboardWatch::detect());
    }
    #[allow(unreachable_code)]
    Box::new(passive::ClipboardWatch::detect())
}

/// Force a mode, for `savepoint run --mode clipboard` and for tests.
pub fn source_for(mode: Mode) -> Box<dyn crate::engine::Source> {
    match mode {
        Mode::Keystrokes => {
            #[cfg(windows)]
            {
                Box::new(windows::WindowsHook)
            }
            #[cfg(target_os = "macos")]
            {
                Box::new(macos::MacTap)
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                Box::new(passive::ClipboardWatch::detect())
            }
        }
        Mode::Clipboard => Box::new(passive::ClipboardWatch::detect()),
        Mode::None => Box::new(NullSource),
    }
}

/// A backend that produces nothing. Used by tests and by `--mode none`.
pub struct NullSource;

impl crate::engine::Source for NullSource {
    fn name(&self) -> &'static str {
        "null"
    }
    fn run(&mut self, _sink: &dyn Fn(Event), stop: &AtomicBool) {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// Best-effort name of the frontmost application, without capturing its contents.
/// Used to attribute clipboard events to an app so drafts stay separate per app.
pub fn frontmost_app() -> String {
    #[cfg(windows)]
    {
        windows::frontmost_app()
    }
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_app()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        passive::frontmost_app()
    }
    #[cfg(not(any(windows, unix)))]
    {
        "unknown".to_string()
    }
}
