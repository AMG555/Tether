//! The design system is a constraint, so it is enforced like one.
//!
//! "Decide it once, enforce it everywhere." A visual style that lives only in a CSS file
//! drifts within two commits — someone pastes a gradient, a `border-radius: 14px` and a
//! `backdrop-filter`, and the product looks like every other AI-generated page again.
//!
//! These tests read the shipped HTML and fail on the specific fingerprints of that look.

use std::path::{Path, PathBuf};

fn shipped_pages() -> Vec<PathBuf> {
    ["gui/clip-studio.html", "gui/clip-preview.html", "assets/pet-studio.html"]
        .iter()
        .map(|p| Path::new(env!("CARGO_MANIFEST_DIR")).join(p))
        .filter(|p| p.exists())
        .collect()
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Every page that carries the product's visual identity.
#[test]
fn the_design_system_exists_and_is_wired_in() {
    let theme = Path::new(env!("CARGO_MANIFEST_DIR")).join("gui/theme.css");
    let css = read(&theme);
    for token in ["--paper:", "--signal:", "--mono:", "--radius:"] {
        assert!(css.contains(token), "theme is missing {token}");
    }
    // The palette must be printed paper + one signal colour, not a dark violet app.
    assert!(css.contains("#f2efe6"), "the paper tone is part of the identity");
    assert!(!css.contains("#a78bfa"), "the generic violet accent must not come back");
    assert!(!css.contains("#8b5cf6"), "the generic violet accent must not come back");
}

#[test]
fn no_page_ships_the_generic_ai_look() {
    for page in shipped_pages() {
        let html = read(&page);
        let name = page.file_name().unwrap().to_string_lossy();

        // 1. Gradient backgrounds are the single strongest tell. The one permitted use is the
        //    fine grid on the desktop stage, which is a rule pattern, not a colour wash.
        assert!(
            !html.contains("linear-gradient(180deg")
                && !html.contains("radial-gradient")
                && !html.contains("linear-gradient(135deg"),
            "{name} contains a decorative gradient"
        );

        // 2. Blurred glass panels.
        assert!(!html.contains("backdrop-filter"), "{name} uses a frosted-glass panel");

        // 3. The AI-app violet family.
        for hue in ["#a78bfa", "#8b5cf6", "#7c3aed", "#6d28d9", "#241b3a"] {
            assert!(!html.to_lowercase().contains(hue), "{name} still uses {hue}");
        }

        // 4. Pill radii. Anything over 3px starts to read as a chat-bubble product.
        for bad in [
            "border-radius:12px",
            "border-radius: 12px",
            "border-radius:14px",
            "border-radius: 14px",
            "border-radius:16px",
            "border-radius: 16px",
            "border-radius:999px",
            "border-radius: 999px",
            "border-radius:24px",
        ] {
            assert!(!html.contains(bad), "{name} has a pill/soft radius: {bad}");
        }

        // 5. The pages must actually load the system rather than re-inventing it inline.
        assert!(
            html.contains("theme.css") || html.contains("--paper:"),
            "{name} does not use the shared theme"
        );
    }
}

#[test]
fn shipped_pages_make_no_network_requests() {
    // The visual layer must stay as offline as the binary.
    for page in shipped_pages() {
        let html = read(&page);
        let name = page.file_name().unwrap().to_string_lossy();
        for scheme in ["https://", "http://", "//fonts.", "cdn."] {
            assert!(!html.contains(scheme), "{name} references the network ({scheme})");
        }
    }
}

#[test]
fn the_theme_itself_has_no_gradients_or_glows() {
    let css = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("gui/theme.css"));
    // The only gradient allowed is the repeating grid on the stage.
    let decorative: Vec<&str> = css
        .lines()
        .filter(|l| l.contains("gradient("))
        .filter(|l| !l.contains("repeating-linear-gradient"))
        .collect();
    assert!(decorative.is_empty(), "decorative gradients in theme.css: {decorative:?}");
    assert!(!css.contains("box-shadow: 0 0 40px"), "no glows");
    assert!(!css.contains("backdrop-filter"), "no frosted glass");
}

/// The capture script is the only thing standing between the README and screenshots that lie.
#[test]
fn the_screenshot_capture_script_exists_and_covers_the_shipped_pages() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = read(&root.join("shots/capture.py"));
    for page in ["clip-studio.html", "clip-preview.html"] {
        assert!(
            script.contains(page),
            "shots/capture.py no longer captures {page} — a changed page must not keep its old shot"
        );
    }
    // It must drive the real binary, not a mock.
    assert!(script.contains("target") && script.contains("tether"), "capture must use the binary");
}
