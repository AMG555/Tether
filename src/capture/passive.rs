//! Passive backend: watches the clipboard, never the keyboard.
//!
//! This is the default on Linux because Wayland (and modern macOS/Win11 hardening) treat
//! global keylogging as malware — which it is, in the wrong hands. Rather than ask for
//! root or defeat the compositor, Savepoint offers a smaller guarantee honestly:
//! **anything you copy is saved forever, encrypted.**
//!
//! Implementation notes:
//! - No daemon, no polling storm: `wl-paste --watch` or `xclip -o -selection clipboard`
//!   is spawned through a shell and read line-buffered. If neither tool exists the source
//!   reports itself as inert rather than pretending to work.
//! - Clipboard contents go through the same `Policy` + `sanitize` path as keystrokes, so
//!   copying a password out of a manager is still refused if the app is blocked.
//! - `frontmost_app()` reads `_NET_WM_PID`/`xdotool` when available, else "clipboard".

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::engine::Source;
use crate::ring::{Event, EV_PASTE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    WlPaste,
    Xclip,
    None,
}

pub struct ClipboardWatch {
    tool: Tool,
}

impl ClipboardWatch {
    pub fn detect() -> Self {
        let has = |bin: &str| {
            Command::new("sh")
                .arg("-c")
                .arg(format!("command -v {bin} >/dev/null 2>&1"))
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        let tool = if std::env::var("WAYLAND_DISPLAY").is_ok() && has("wl-paste") {
            Tool::WlPaste
        } else if has("xclip") {
            Tool::Xclip
        } else if has("wl-paste") {
            Tool::WlPaste
        } else {
            Tool::None
        };
        Self { tool }
    }

    fn command(&self) -> Option<Command> {
        match self.tool {
            Tool::WlPaste => {
                let mut c = Command::new("wl-paste");
                // --watch re-emits on every clipboard change; --no-newline keeps bytes intact.
                c.arg("--watch").arg("--no-newline");
                Some(c)
            }
            Tool::Xclip => {
                // xclip has no watch mode; poll with a tiny loop instead of a busy spin.
                let mut c = Command::new("sh");
                c.arg("-c").arg(
                    "last=''; while :; do cur=$(xclip -o -selection clipboard 2>/dev/null); \
                     if [ \"$cur\" != \"$last\" ]; then printf '%s\\0' \"$cur\"; last=\"$cur\"; fi; \
                     sleep 1; done",
                );
                Some(c)
            }
            Tool::None => None,
        }
    }
}

impl Source for ClipboardWatch {
    fn name(&self) -> &'static str {
        match self.tool {
            Tool::WlPaste => "clipboard (wl-paste)",
            Tool::Xclip => "clipboard (xclip)",
            Tool::None => "clipboard (unavailable: install wl-clipboard or xclip)",
        }
    }

    fn run(&mut self, sink: &dyn Fn(Event), stop: &AtomicBool) {
        let Some(mut cmd) = self.command() else {
            // Stay alive but idle so the rest of the app (list/get) keeps working.
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            return;
        };
        let Ok(mut child) =
            cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null()).spawn()
        else {
            return;
        };
        let Some(stdout) = child.stdout.take() else { return };
        let mut reader = BufReader::new(stdout);
        let mut buf: Vec<u8> = Vec::new();

        while !stop.load(Ordering::Relaxed) {
            buf.clear();
            // wl-paste --watch separates emissions with newlines; NUL also accepted.
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Ok(_) => {
                    while buf.last() == Some(&b'\n') || buf.last() == Some(&b'\0') {
                        buf.pop();
                    }
                    if buf.is_empty() {
                        continue;
                    }
                    let text = String::from_utf8_lossy(&buf);
                    // Clipboard events are chunks, not keystrokes; chunk them to fit a slot
                    // rather than truncating silently.
                    for chunk in chunk_chars(&text, 60) {
                        let mut ev = Event::new(EV_PASTE, &frontmost_app());
                        ev.set_text(&chunk);
                        sink(ev);
                    }
                }
                Err(_) => break,
            }
        }
        let _ = child.kill();
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

/// Best-effort frontmost app on Linux. Deliberately optional: attribution is nice, but a
/// missing `xdotool` must never stop capture.
pub fn frontmost_app() -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg("xdotool getactivewindow getwindowpid 2>/dev/null || true")
        .output();
    if let Ok(o) = out {
        let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if let Ok(pid) = pid.parse::<u32>() {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            let comm = comm.trim();
            if !comm.is_empty() {
                return comm.to_string();
            }
        }
    }
    "clipboard".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_slot_width() {
        let s = "x".repeat(150);
        let parts = chunk_chars(&s, 60);
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.chars().count() <= 60));
        assert_eq!(parts.concat(), s);
    }

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk_chars("hi", 60), vec!["hi".to_string()]);
    }

    #[test]
    fn chunks_are_char_boundaries_for_unicode() {
        let s = "नमस्ते".repeat(20);
        for p in chunk_chars(&s, 7) {
            assert!(std::str::from_utf8(p.as_bytes()).is_ok());
        }
    }

    #[test]
    fn detection_never_panics() {
        let w = ClipboardWatch::detect();
        assert!(!w.name().is_empty());
    }
}
