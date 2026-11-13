//! User-facing runtime configuration (`config.toml`-shaped JSON in the data dir).
//!
//! Kept deliberately small and boring: a human can read and edit this file, and a bad file
//! must never stop capture from starting. Anything unparseable falls back to defaults with a
//! warning, because "your protection silently turned itself off" is the worst outcome here.

use serde::{Deserialize, Serialize};

use crate::pet::Pet;
use crate::remind::Schedule;

pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Reminder schedule for the desktop companion.
    pub remind: Schedule,
    /// Which installed pet to use. `None` = the built-in procedural blob.
    pub pet: Option<String>,
    /// Let the pet walk around the desktop. Off by default: some people find movement
    /// distracting, and a companion that never moves is still useful.
    pub pet_walk: bool,
    /// Show the pet always-on-top of other windows.
    pub pet_always_on_top: bool,
    /// Where the pet may roam, as a fraction of the work area, `(min_x, max_x)`.
    pub pet_bounds: (f32, f32),
    /// Commit interval for the capture loop, milliseconds.
    pub flush_ms: u64,
    /// Crash ring size in keystrokes.
    pub ring_slots: usize,
    /// Retention window for rescued texts and revisions, days. Defaults to 14 (2 weeks).
    pub retention_days: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            remind: Schedule::default(),
            pet: None,
            pet_walk: false,
            pet_always_on_top: false,
            pet_bounds: (0.05, 0.95),
            flush_ms: 1000,
            ring_slots: 4096,
            retention_days: 14,
        }
    }
}

impl Config {
    /// Loads from `<dir>/config.json`, creating a documented default on first run.
    /// Never fails: a broken config is reported and defaults are used.
    pub fn load_or_default(dir: &std::path::Path) -> (Self, Option<String>) {
        let path = dir.join(CONFIG_FILE);
        if !path.exists() {
            let cfg = Config::default();
            let body = serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".into());
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&path, body);
            return (cfg, None);
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    // Clamp the dangerous knobs rather than trusting them.
                    cfg.flush_ms = cfg.flush_ms.clamp(200, 60_000);
                    cfg.ring_slots = cfg.ring_slots.clamp(64, 65536);
                    cfg.retention_days = cfg.retention_days.clamp(1, 365);
                    let (lo, hi) = cfg.pet_bounds;
                    cfg.pet_bounds = (lo.clamp(0.0, 1.0), hi.clamp(0.0, 1.0));
                    (cfg, None)
                }
                Err(e) => (
                    Config::default(),
                    Some(format!("{} is not valid config ({e}); using defaults", path.display())),
                ),
            },
            Err(e) => (
                Config::default(),
                Some(format!("could not read {}: {e}; using defaults", path.display())),
            ),
        }
    }

    pub fn save(&self, dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let body = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(dir.join(CONFIG_FILE), body)
    }

    /// Resolves the configured pet against what is actually installed.
    pub fn resolve_pet(&self, pets_dir: &std::path::Path) -> Option<Pet> {
        let wanted = self.pet.as_ref()?;
        let (pets, _) = crate::pet::installed(pets_dir);
        pets.into_iter()
            .find(|p| crate::pet::slug(&p.name) == crate::pet::slug(wanted) || &p.name == wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_creates_a_readable_default() {
        let d = tempfile::tempdir().unwrap();
        let (cfg, warn) = Config::load_or_default(d.path());
        assert!(warn.is_none());
        assert_eq!(cfg.flush_ms, 1000);
        let written = std::fs::read_to_string(d.path().join(CONFIG_FILE)).unwrap();
        assert!(written.contains("water_every_min"), "the file must be self-documenting");
    }

    #[test]
    fn broken_config_falls_back_with_a_warning() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(CONFIG_FILE), "{ this is not json").unwrap();
        let (cfg, warn) = Config::load_or_default(d.path());
        assert_eq!(cfg.flush_ms, 1000, "defaults still apply");
        assert!(warn.unwrap().contains("not valid config"));
    }

    #[test]
    fn dangerous_values_are_clamped() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join(CONFIG_FILE),
            r#"{"flush_ms": 1, "ring_slots": 999999999, "pet_bounds": [-5.0, 9.0], "retention_days": 0}"#,
        )
        .unwrap();
        let (cfg, _) = Config::load_or_default(d.path());
        assert_eq!(cfg.flush_ms, 200, "a 1ms flush would hammer the disk");
        assert_eq!(cfg.ring_slots, 65536);
        assert_eq!(cfg.pet_bounds, (0.0, 1.0));
        assert_eq!(cfg.retention_days, 1, "clamped to minimum 1 day");
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(CONFIG_FILE), r#"{"pet": "Mochi"}"#).unwrap();
        let (cfg, warn) = Config::load_or_default(d.path());
        assert!(warn.is_none());
        assert_eq!(cfg.pet.as_deref(), Some("Mochi"));
        assert_eq!(cfg.remind.water_every_min, 45, "untouched fields keep defaults");
    }

    #[test]
    fn pet_resolution_matches_by_slug_or_name() {
        let d = tempfile::tempdir().unwrap();
        let pets = d.path().join("pets");
        crate::pet::scaffold(&pets.join("mochi"), "Mochi", 32, "#c2331a", "#1f6b3a").unwrap();
        let cfg = Config { pet: Some("mochi".into()), ..Default::default() };
        assert_eq!(cfg.resolve_pet(&pets).map(|p| p.name), Some("Mochi".to_string()));
        let cfg2 = Config { pet: Some("Mochi".into()), ..Default::default() };
        assert!(cfg2.resolve_pet(&pets).is_some());
        let cfg3 = Config { pet: Some("Nobody".into()), ..Default::default() };
        assert!(cfg3.resolve_pet(&pets).is_none());
    }
}
