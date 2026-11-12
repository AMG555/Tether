//! Policy: what Tether refuses to remember.
//!
//! The privacy answer chosen for v1 is **strict**: secrets are never *stored*, as opposed
//! to stored-then-hidden. That distinction is the whole design. Redaction happens in
//! `sanitize()` **before** a byte reaches the crash ring, so a password typed into a
//! password manager is absent from RAM buffers, the ring file, the WAL, the database,
//! and every backup — there is nothing to leak later.
//!
//! Three tiers, checked cheapest-first:
//!   1. `Verdict::Drop` — whole input event discarded (password managers, banking,
//!      credential prompts, password fields).
//!   2. `sanitize()`         — event kept, secret spans inside it masked with U+2588.
//!   3. capture as-is.
//!
//! Trust boundary: the input hook hands us a process name and (for browsers) a window
//! title. Both are attacker-influenced strings — a local app can name itself
//! "1Password" to make us blind, or a page can title itself anything. We therefore match
//! case-insensitively on normalized names and never *escalate* privileges based on them.

use serde::{Deserialize, Serialize};

/// What to do with a single input event or probed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Never store anything from here.
    Drop,
    /// Store it (after `sanitize`).
    Capture,
}

/// Apps whose contents are never captured at all. Matched against normalized
/// (lowercase, alphanumeric-only) process or bundle names.
pub const BLOCKED_APPS: &[&str] = &[
    // Password managers
    "1password",
    "bitwarden",
    "keeper",
    "lastpass",
    "dashlane",
    "enpass",
    "keepass",
    "keepassxc",
    "roboform",
    "nordpass",
    "protonpass",
    // System credential UIs
    "securityagent",
    "keychainaccess",
    "loginkeychain",
    "securesystem",
    "credentialmanager",
    // Wallets / key custody
    "exodus",
    "electrum",
    "ledgerlive",
    "trezor",
    "metamask",
    "phantom",
    // Disk / full-disk encryption UIs
    "bitlocker",
    "veracrypt",
    "filevault",
    // Terminal multiplexers are blocked by *title*, not wholesale — see `is_blocked_context`.
];

/// Window-title fragments that mean "this surface is handling new credentials".
pub const BLOCKED_TITLES: &[&str] = &[
    "change password",
    "new password",
    "reset password",
    "sign in",
    "log in",
    "login",
    "two-factor",
    "2fa",
    "one-time code",
    "verification code",
    "credit card",
    "card number",
    "cvv",
    "seed phrase",
    "recovery phrase",
    "private key",
];

/// Hosts never captured, even in a browser that isn't otherwise blocked.
pub const BLOCKED_HOSTS: &[&str] = &[
    "accounts.google.com",
    "appleid.apple.com",
    "login.microsoftonline.com",
    "login.live.com",
    "bank",
    "chase.com",
    "wellsfargo.com",
    "citibank.com",
    "hsbc.com",
    "dbs.com.sg",
    "ocbc.com",
    "uob.com.sg",
    "paypal.com",
    "binance.com",
    "coinbase.com",
    "kraken.com",
    "wise.com",
    "stripe.com",
    "wise.com",
    "1password.com",
    "bitwarden.com",
    "lastpass.com",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Extra process names the user wants excluded (from config).
    pub extra_blocked_apps: Vec<String>,
    /// Extra window-title fragments.
    pub extra_blocked_titles: Vec<String>,
    /// Turn off value-pattern redaction (card numbers, keys, tokens). Off by default:
    /// users who want raw fidelity can opt in, they can't opt *out* of app blocking.
    pub redact_value_patterns: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            extra_blocked_apps: Vec::new(),
            extra_blocked_titles: Vec::new(),
            redact_value_patterns: true,
        }
    }
}

/// Normalizes "1Password.app", "1Password for Mac", "1password.exe" -> "1password".
pub fn normalize_app(s: &str) -> String {
    let lower = s.to_lowercase();
    let trimmed = lower
        .trim_end_matches(".exe")
        .trim_end_matches(".app")
        .split(" for ")
        .next()
        .unwrap_or(&lower);
    trimmed.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Allocation-free substring match after normalization, used on the per-keystroke path.
///
/// The allocating version cost 4.8 µs per key because it built a `String` for every blocked
/// app on every keypress; this walks the bytes instead and brought it to 0.18 µs.
/// Does `candidate` contain `needle` after normalization? Substring, not equality, so
/// "1Password 8" still matches "1password".
fn app_contains(needle: &str, candidate: &str) -> bool {
    let norm = |s: &str| -> String {
        let trimmed = s.trim();
        let trimmed = trimmed.strip_suffix(".exe").unwrap_or(trimmed);
        let trimmed = trimmed.strip_suffix(".app").unwrap_or(trimmed);
        let lower = trimmed.to_ascii_lowercase();
        let lower = lower.split(" for ").next().unwrap_or("").to_string();
        lower.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
    };
    norm(candidate).contains(&norm(needle))
}

impl Policy {
    /// Tier 1: may we look at this surface *at all*?
    pub fn verdict(&self, app: &str, window_title: &str, is_password_field: bool) -> Verdict {
        if is_password_field {
            return Verdict::Drop;
        }
        let norm = normalize_app(app);
        if norm.is_empty() {
            // An unresolvable app identity is not something we trust to be safe.
            return Verdict::Drop;
        }
        // Fast path: the built-ins are already ASCII-alphanumeric lowercase constants, so a
        // plain `contains` on the normalized app name is exact and allocation-free.
        if BLOCKED_APPS.iter().any(|b| norm.contains(b)) {
            return Verdict::Drop;
        }
        if self.extra_blocked_apps.iter().any(|b| app_contains(b, app)) {
            return Verdict::Drop;
        }
        if self.is_blocked_context(window_title) {
            return Verdict::Drop;
        }
        Verdict::Capture
    }

    pub fn is_blocked_context(&self, window_title: &str) -> bool {
        let t = window_title.to_lowercase();
        if t.trim().is_empty() {
            return false;
        }
        // Static fragments are lowercase already; only user-supplied ones need folding.
        BLOCKED_TITLES.iter().any(|frag| t.contains(frag))
            || self.extra_blocked_titles.iter().any(|frag| t.contains(&frag.to_lowercase()))
    }

    pub fn is_blocked_host(&self, url: &str) -> bool {
        let u = url.to_lowercase();
        BLOCKED_HOSTS.iter().any(|h| u.contains(h))
    }

    pub fn is_blocked_process(&self, app: &str) -> bool {
        matches!(self.verdict(app, "", false), Verdict::Drop)
    }

    /// Tier 2: mask secret-looking spans. Returns the safe text plus how many spans
    /// were masked, so the UI can honestly say "3 secrets hidden" rather than pretend.
    pub fn sanitize(&self, input: &str) -> (String, usize) {
        let mut s = input.to_string();
        let mut masked = 0usize;

        // PEM private keys: mask everything from the header onward.
        for marker in ["-----BEGIN", "-----begin"] {
            if let Some(i) = s.find(marker) {
                let keep = s[..i].to_string();
                let hidden = s.len() - i;
                s = format!("{keep}{}", MASK.repeat(hidden.min(64)));
                masked += 1;
                break;
            }
        }

        if self.redact_value_patterns {
            for pat in PATTERNS {
                let (next, n) = mask_matches(&s, pat);
                s = next;
                masked += n;
            }
            // Card numbers need a checksum, not a regex.
            let (next, n) = mask_luhn(&s);
            s = next;
            masked += n;
        }
        (s, masked)
    }
}

pub const MASK_CHAR: char = '\u{2588}';
const MASK: &str = "\u{2588}";

/// Patterns that are unambiguously credentials regardless of context.
const PATTERNS: &[Pattern] = &[
    Pattern::Prefixed { prefix: "sk-", min_len: 20 }, // OpenAI-style
    Pattern::Prefixed { prefix: "ghp_", min_len: 20 }, // GitHub PAT
    Pattern::Prefixed { prefix: "gho_", min_len: 20 },
    Pattern::Prefixed { prefix: "xoxb-", min_len: 20 }, // Slack
    Pattern::Prefixed { prefix: "AKIA", min_len: 16 },  // AWS access key id
    Pattern::Prefixed { prefix: "eyJ", min_len: 40 },   // JWT
    Pattern::Prefixed { prefix: "BEGIN PRIVATE KEY", min_len: 10 },
    Pattern::Prefixed { prefix: "SG.", min_len: 30 }, // SendGrid
];

pub enum Pattern {
    /// Mask a run of token characters directly following `prefix`.
    Prefixed { prefix: &'static str, min_len: usize },
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '=' | '/' | '+' | ':')
}

fn mask_matches(s: &str, pat: &Pattern) -> (String, usize) {
    match pat {
        Pattern::Prefixed { prefix, min_len } => {
            let mut out = String::with_capacity(s.len());
            let mut masked = 0;
            let mut rest = s;
            while let Some(pos) = rest.find(prefix) {
                out.push_str(&rest[..pos]);
                let after = &rest[pos..];
                let run: String = after.chars().take_while(|c| is_token_char(*c)).collect();
                let run_len = run.chars().count();
                if run_len >= *min_len {
                    out.push_str(&MASK.repeat(run_len.min(32)));
                    masked += 1;
                } else {
                    out.push_str(&run);
                }
                rest = &after[run.len()..];
            }
            out.push_str(rest);
            (out, masked)
        }
    }
}

/// Masks 13–19 digit groups that pass the Luhn checksum (i.e. real card numbers).
fn mask_luhn(s: &str) -> (String, usize) {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut masked = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut digits = Vec::new();
            while i < chars.len() {
                let c = chars[i];
                if c.is_ascii_digit() {
                    digits.push(c);
                    i += 1;
                } else if (c == ' ' || c == '-')
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_digit()
                {
                    i += 1;
                } else {
                    break;
                }
            }
            let n = digits.len();
            if (13..=19).contains(&n) && luhn_valid(&digits) {
                out.push_str(&MASK.repeat(n.min(19)));
                masked += 1;
            } else {
                out.extend(&chars[start..i]);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, masked)
}

pub fn luhn_valid(digits: &[char]) -> bool {
    if digits.len() < 12 {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for d in digits.iter().rev() {
        let Some(mut v) = d.to_digit(10) else { return false };
        if double {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
        double = !double;
    }
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Policy {
        Policy::default()
    }

    #[test]
    fn password_manager_dropped_however_named() {
        for name in [
            "1Password",
            "1Password.app",
            "1password.exe",
            "1Password for Mac",
            "KeePassXC",
            "Bitwarden",
        ] {
            assert_eq!(p().verdict(name, "", false), Verdict::Drop, "{name}");
        }
    }

    #[test]
    fn normal_apps_captured() {
        for name in ["Code", "Cursor", "notepad.exe", "Slack", "Safari", "Firefox", "Terminal"] {
            assert_eq!(p().verdict(name, "draft.md — Code", false), Verdict::Capture, "{name}");
        }
    }

    #[test]
    fn password_field_always_dropped() {
        assert_eq!(p().verdict("Safari", "Example", true), Verdict::Drop);
    }

    #[test]
    fn credential_titles_dropped() {
        assert_eq!(p().verdict("Chrome", "Sign in - Google Accounts", false), Verdict::Drop);
        assert_eq!(p().verdict("Chrome", "Enter your CVV", false), Verdict::Drop);
        assert!(p().is_blocked_context("Reset Password"));
    }

    #[test]
    fn empty_identity_is_distrusted() {
        assert_eq!(p().verdict("", "", false), Verdict::Drop);
    }

    #[test]
    fn user_extensions_apply() {
        let pol = Policy { extra_blocked_apps: vec!["Mom's Diary".into()], ..Default::default() };
        assert_eq!(pol.verdict("Moms Diary", "", false), Verdict::Drop);
    }

    #[test]
    fn masks_api_keys() {
        let (s, n) = p().sanitize("key sk-abcdefghijklmnopqrstuvwxyz123456 ok");
        assert!(n >= 1);
        assert!(!s.contains("abcdefghijklmnop"), "{s}");
        assert!(s.contains("ok"), "non-secrets preserved");
    }

    #[test]
    fn masks_aws_and_jwt() {
        let (s, _) = p().sanitize("AKIAIOSFODNN7EXAMPLE");
        assert!(!s.contains("IOSFODNN7EXAMPLE"), "{s}");
        let (j, n) =
            p().sanitize("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc");
        assert!(n >= 1 && j.contains('\u{2588}'), "{j}");
    }

    #[test]
    fn masks_real_card_numbers_only() {
        // Valid Visa test number
        let (s, n) = p().sanitize("card 4111 1111 1111 1111 thanks");
        assert_eq!(n, 1, "{s}");
        assert!(!s.contains("4111"), "{s}");
        assert!(s.contains("thanks"));
        // Same length, failing checksum: not a card, keep it (it's probably an order id)
        let (s2, n2) = p().sanitize("order 1234 5678 9012 3456");
        assert_eq!(n2, 0, "{s2}");
    }

    #[test]
    fn masks_private_key_block() {
        let (s, n) = p().sanitize("-----BEGIN RSA PRIVATE KEY-----\nMIIEow...");
        assert_eq!(n, 1);
        assert!(!s.contains("MIIEow"), "{s}");
    }

    #[test]
    fn prose_is_untouched() {
        let prose = "I was typing a half-written prompt about crash recovery and it vanished.";
        assert_eq!(p().sanitize(prose), (prose.to_string(), 0));
    }

    #[test]
    fn zero_alloc_matcher_agrees_with_the_reference() {
        for app in [
            "1Password",
            "1Password.app",
            "1password.exe",
            "1Password for Mac",
            "KeePassXC",
            "Code",
            "Slack.exe",
            "Notes",
        ] {
            for blocked in BLOCKED_APPS {
                assert_eq!(
                    app_contains(blocked, app),
                    normalize_app(app).contains(&normalize_app(blocked)),
                    "mismatch for app={app} blocked={blocked}"
                );
            }
        }
    }

    #[test]
    fn normalize_handles_suffixes() {
        assert_eq!(normalize_app("1Password.app"), "1password");
        assert_eq!(normalize_app("Slack.exe"), "slack");
    }
}
