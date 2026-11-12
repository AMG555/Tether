//! Reconstructing a draft from a stream of buffered events.
//!
//! The interesting part of Tether is *not* storing keystrokes — it is turning a pile of
//! `Char`/`Backspace`/`Paste` events back into the text the user was looking at, across a
//! crash, after the ring wrapped, and with edits that happened mid-string.
//!
//! Rules (ADR 0004):
//! - `Char` appends and advances the caret.
//! - `Backspace` deletes before the caret.
//! - `Paste` inserts the whole string at the caret (a paste is one event, not N chars —
//!   otherwise a 5k paste would evict the entire ring).
//! - `Focus`/`Blur` are boundaries: a new field means a new draft.
//! - The caret is *unknown* in v1 because low-level hooks don't report it. We therefore
//!   assume "append at end", which is correct for the overwhelmingly common case (typing
//!   into an empty or growing field) and degrade-safe for the rest: a mid-string edit
//!   yields a plausible draft, never lost input. `probe` mode fixes this properly by
//!   reading the field's real contents on macOS/Windows (see docs/limitations).

use crate::ring::{Event, EV_BACKSPACE, EV_BLUR, EV_CHAR, EV_FOCUS, EV_PASTE};

/// A reconstructed draft for one (app, field) session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub app: String,
    pub text: String,
    pub started_ts: u32,
    pub updated_ts: u32,
    pub events: u32,
    pub redactions: u32,
    /// True when the ring wrapped or a Paste was truncated, i.e. `text` may be a suffix
    /// of what the user actually typed. The UI must show this honestly.
    pub truncated: bool,
}

impl Draft {
    pub fn new(app: String, ts: u32) -> Self {
        Draft {
            app,
            text: String::new(),
            started_ts: ts,
            updated_ts: ts,
            events: 0,
            redactions: 0,
            truncated: false,
        }
    }
}

#[derive(Default)]
pub struct Builder {
    draft: Option<Draft>,
}

impl Builder {
    pub fn new() -> Self {
        Self { draft: None }
    }

    /// Feeds one event, returning a draft if a session boundary was reached.
    pub fn push(&mut self, ev: &Event) -> Option<Draft> {
        match ev.kind {
            EV_FOCUS => {
                let finished = self.flush();
                self.draft = Some(Draft {
                    app: ev.app_str().to_string(),
                    text: String::new(),
                    started_ts: ev.ts,
                    updated_ts: ev.ts,
                    events: 0,
                    redactions: 0,
                    truncated: false,
                });
                finished
            }
            EV_BLUR => self.flush(),
            EV_CHAR | EV_BACKSPACE | EV_PASTE => {
                let app = ev.app_str().to_string();
                // A different app means a boundary even without a Focus event (hooks miss
                // focus changes when a window is restored after a crash).
                let switch =
                    self.draft.as_ref().map(|d| !app.is_empty() && d.app != app).unwrap_or(false);
                let mut finished = None;
                if switch {
                    finished = self.flush();
                    self.draft = Some(Draft::new(app.clone(), ev.ts));
                }
                if self.draft.is_none() {
                    self.draft = Some(Draft::new(app, ev.ts));
                }
                self.apply(ev);
                if let Some(d) = self.draft.as_mut() {
                    d.updated_ts = ev.ts;
                    d.events = d.events.saturating_add(1);
                }
                finished
            }
            _ => None,
        }
    }

    fn apply(&mut self, ev: &Event) {
        let Some(d) = self.draft.as_mut() else { return };
        match ev.kind {
            EV_CHAR => {
                let t = ev.text_str();
                if t.contains('\u{2588}') {
                    d.redactions = d.redactions.saturating_add(1);
                }
                d.text.push_str(t);
            }
            EV_BACKSPACE => {
                // pop_one grapheme: removing a whole 'e' when it's the combining accent
                // would leave mojibake on screen, which is worse than the original bug.
                pop_one(&mut d.text);
            }
            EV_PASTE => {
                let t = ev.text_str();
                // A payload that exactly fills the slot may have been cut; flag it.
                if t.len() >= 60 {
                    d.truncated = true;
                }
                if t.contains('\u{2588}') {
                    d.redactions = d.redactions.saturating_add(1);
                }
                d.text.push_str(t);
            }
            _ => {}
        }
    }

    /// Ends the current session and returns it if it contains anything worth saving.
    pub fn flush(&mut self) -> Option<Draft> {
        let d = self.draft.take()?;
        if d.text.trim().is_empty() || d.events == 0 {
            return None;
        }
        Some(d)
    }

    /// Final draft without ending the session (used by the live "show me my buffer" view).
    pub fn peek(&self) -> Option<Draft> {
        self.draft.clone().filter(|d| !d.text.is_empty())
    }
}

/// Removes one user-perceived character.
///
/// Order matters: trailing combining marks are stripped *first*, then the base character.
/// Popping the base first would leave a floating accent and the user's next keystroke would
/// render on top of the wrong letter — the kind of corruption a recovery tool must never cause.
pub fn pop_one(s: &mut String) {
    while let Some(c) = s.chars().next_back() {
        if is_combining(c) {
            s.truncate(s.len() - c.len_utf8());
        } else {
            break;
        }
    }
    if let Some(c) = s.chars().next_back() {
        s.truncate(s.len() - c.len_utf8());
    }
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Rebuilds all drafts from a drained event list.
pub fn reconstruct(events: &[Event]) -> Vec<Draft> {
    let mut b = Builder::new();
    let mut out = Vec::new();
    for ev in events {
        if let Some(d) = b.push(ev) {
            out.push(d);
        }
    }
    if let Some(d) = b.flush() {
        out.push(d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(app: &str, s: &str) -> Event {
        let mut e = Event::new(EV_CHAR, app);
        e.set_text(s);
        e
    }
    fn typed(app: &str, s: &str) -> Vec<Event> {
        s.chars().map(|c| ch(app, &c.to_string())).collect()
    }

    #[test]
    fn types_a_sentence() {
        let mut ev = typed("Code", "hello world");
        for (i, e) in ev.iter_mut().enumerate() {
            e.ts = i as u32;
        }
        let d = reconstruct(&ev);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].text, "hello world");
        assert_eq!(d[0].events, 11);
    }

    #[test]
    fn backspace_undoes() {
        let mut ev = typed("t", "hello");
        let mut bs = Event::new(EV_BACKSPACE, "t");
        bs.ts = 99;
        ev.push(bs);
        ev.push(ch("t", "!"));
        assert_eq!(reconstruct(&ev)[0].text, "hell!");
    }

    #[test]
    fn backspace_on_empty_is_safe() {
        let mut e = Event::new(EV_BACKSPACE, "t");
        e.ts = 1;
        assert!(reconstruct(&[e]).is_empty(), "nothing worth saving");
    }

    #[test]
    fn backspace_removes_whole_combining_char() {
        let mut ev = vec![ch("t", "e")];
        let mut accent = ch("t", "\u{0301}"); // combining acute
        accent.ts = 2;
        ev.push(accent);
        let mut bs = Event::new(EV_BACKSPACE, "t");
        bs.ts = 3;
        ev.push(bs);
        assert_eq!(reconstruct(&ev).len(), 0, "e+accent deleted as one grapheme");
    }

    #[test]
    fn focus_boundary_splits_drafts() {
        let mut ev = vec![Event::new(EV_FOCUS, "Code")];
        ev.extend(typed("Code", "first"));
        ev.push(Event::new(EV_BLUR, "Code"));
        ev.push(Event::new(EV_FOCUS, "Chrome"));
        ev.extend(typed("Chrome", "second"));
        let d = reconstruct(&ev);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].text, "first");
        assert_eq!(d[1].text, "second");
        assert_eq!(d[1].app, "Chrome");
    }

    #[test]
    fn app_switch_without_focus_still_splits() {
        let mut ev = typed("Code", "abc");
        ev.extend(typed("Slack", "xyz"));
        let d = reconstruct(&ev);
        assert_eq!(d.len(), 2, "half a prompt in Slack must not merge into the editor draft");
        assert_eq!(d[0].text, "abc");
        assert_eq!(d[1].text, "xyz");
    }

    #[test]
    fn paste_is_one_event_not_many() {
        let mut p = Event::new(EV_PASTE, "t");
        p.set_text("a long pasted paragraph");
        let d = reconstruct(&[p]);
        assert_eq!(d[0].text, "a long pasted paragraph");
        assert_eq!(d[0].events, 1, "paste must not consume N ring slots");
        assert!(!d[0].truncated);
    }

    #[test]
    fn truncated_paste_is_flagged() {
        let mut p = Event::new(EV_PASTE, "t");
        p.set_text(&"x".repeat(64));
        assert!(reconstruct(&[p])[0].truncated, "silently dropping text is not allowed");
    }

    #[test]
    fn whitespace_only_draft_is_not_saved() {
        assert!(reconstruct(&typed("t", "   \n  ")).is_empty());
    }

    #[test]
    fn redactions_are_counted() {
        let mut ev = typed("t", "key ");
        ev.extend(typed("t", "\u{2588}\u{2588}\u{2588}"));
        assert!(reconstruct(&ev)[0].redactions >= 3);
    }

    #[test]
    fn unicode_roundtrip() {
        let text = "नमस्ते 世界 🎉";
        assert_eq!(reconstruct(&typed("t", text))[0].text, text);
    }
}
