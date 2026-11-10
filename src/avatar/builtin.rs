//! Built-in avatars: procedural, rigged, animated humanoid and quadruped.
//!
//! These exist so the companion is never empty. They are described as **parameter sets**, not
//! baked meshes: a body plan plus proportions, palette and gait. That keeps them a few hundred
//! bytes in the repo instead of tens of megabytes, makes them infinitely customisable (any
//! proportion or colour is one field), and sidesteps the licensing mess of shipping character
//! art.
//!
//! The window renders these with three.js primitives, skinned to a small bone chain so the same
//! walk/idle/celebrate clips that drive a real VRM drive these too. The result reads as a stylised
//! character — deliberately, because a low-poly procedural figure that *tries* to look human
//! lands in the uncanny valley, and an honest stylisation does not.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Species {
    /// Upright biped: walks, waves, sits, shoulders drop on a break reminder.
    Human,
    /// Quadruped: trots, sits, tail wag on recoveries.
    Cat,
    /// Quadruped with floppy ears.
    Dog,
    /// Small round biped that hops rather than walks.
    Blob,
}

impl Species {
    pub fn label(&self) -> &'static str {
        match self {
            Species::Human => "human",
            Species::Cat => "cat",
            Species::Dog => "dog",
            Species::Blob => "blob",
        }
    }

    pub fn is_biped(&self) -> bool {
        matches!(self, Species::Human | Species::Blob)
    }

    pub fn legs(&self) -> u8 {
        if self.is_biped() {
            2
        } else {
            4
        }
    }

    pub fn default_name(&self) -> &'static str {
        match self {
            Species::Human => "Kai",
            Species::Cat => "Mochi",
            Species::Dog => "Biscuit",
            Species::Blob => "Pip",
        }
    }

    /// Which animation clips the runtime should expect for this body plan.
    pub fn clips(&self) -> Vec<&'static str> {
        match self {
            Species::Human => vec!["idle", "walk", "sit", "wave", "celebrate", "sleep"],
            Species::Cat | Species::Dog => vec!["idle", "walk", "sit", "tail_wag", "sleep"],
            Species::Blob => vec!["idle", "hop", "sit", "celebrate", "sleep"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    /// Total height in metres-ish units (0.3 = desk buddy, 1.7 = life-sized).
    pub height: f32,
    /// Head-to-body ratio. Bigger reads as cuter; smaller reads as more realistic.
    pub head_ratio: f32,
    /// Shoulder width relative to height.
    pub shoulder: f32,
    /// Limb thickness relative to height.
    pub limb: f32,
    /// 0 = smooth spheres, 1 = boxy. Affects the primitive used per bone.
    pub blockiness: f32,
}

impl Default for Body {
    fn default() -> Self {
        Self { height: 1.0, head_ratio: 0.18, shoulder: 0.22, limb: 0.06, blockiness: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    /// #rrggbb
    pub skin: String,
    pub hair: String,
    pub top: String,
    pub bottom: String,
    pub accent: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            skin: "#e8c6a8".into(),
            hair: "#3a2d22".into(),
            top: "#c2331a".into(),
            bottom: "#3f3a33".into(),
            accent: "#1f6b3a".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Builtin {
    pub species: Species,
    pub name: String,
    pub body: Body,
    pub palette: Palette,
    /// Animation playback speed multiplier.
    pub tempo: f32,
    /// Whether the avatar may wander across the desktop (config can still override).
    pub walks: bool,
    /// Optional accessories: a hat, glasses, a little tie. Cosmetic only.
    pub accessories: Vec<String>,
}

impl Default for Builtin {
    fn default() -> Self {
        Self {
            species: Species::Cat,
            name: "Mochi".into(),
            body: Body::default(),
            palette: Palette::default(),
            tempo: 1.0,
            walks: true,
            accessories: Vec::new(),
        }
    }
}

/// The curated starting set. Named after what they are so users can rename freely.
pub fn presets() -> Vec<Builtin> {
    vec![
        Builtin {
            species: Species::Human,
            name: "Kai".into(),
            body: Body {
                height: 1.7,
                head_ratio: 0.16,
                shoulder: 0.24,
                limb: 0.055,
                blockiness: 0.0,
            },
            palette: Palette {
                skin: "#e8b58a".into(),
                hair: "#1b1410".into(),
                top: "#6d28d9".into(),
                bottom: "#111827".into(),
                accent: "#22d3ee".into(),
            },
            tempo: 1.0,
            walks: true,
            accessories: vec!["glasses".into()],
        },
        Builtin {
            species: Species::Cat,
            name: "Mochi".into(),
            body: Body {
                height: 0.45,
                head_ratio: 0.34,
                shoulder: 0.2,
                limb: 0.07,
                blockiness: 0.0,
            },
            palette: Palette {
                skin: "#f4a259".into(),
                hair: "#f4a259".into(),
                top: "#fb923c".into(),
                bottom: "#fb923c".into(),
                accent: "#fde047".into(),
            },
            tempo: 1.15,
            walks: true,
            accessories: Vec::new(),
        },
        Builtin {
            species: Species::Dog,
            name: "Biscuit".into(),
            body: Body {
                height: 0.5,
                head_ratio: 0.31,
                shoulder: 0.22,
                limb: 0.08,
                blockiness: 0.15,
            },
            palette: Palette {
                skin: "#d9a066".into(),
                hair: "#8b5e34".into(),
                top: "#a16207".into(),
                bottom: "#a16207".into(),
                accent: "#f87171".into(),
            },
            tempo: 1.25,
            walks: true,
            accessories: vec!["collar".into()],
        },
        Builtin {
            species: Species::Blob,
            name: "Pip".into(),
            body: Body {
                height: 0.32,
                head_ratio: 0.5,
                shoulder: 0.3,
                limb: 0.05,
                blockiness: 0.0,
            },
            palette: Palette {
                skin: "#a78bfa".into(),
                hair: "#a78bfa".into(),
                top: "#c2331a".into(),
                bottom: "#7c3aed".into(),
                accent: "#1f6b3a".into(),
            },
            tempo: 0.9,
            walks: true,
            accessories: Vec::new(),
        },
    ]
}

impl Builtin {
    pub fn by_name(name: &str) -> Option<Builtin> {
        presets()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name) || p.species.label() == name.to_lowercase())
    }

    /// Clamps every field to something renderable. A config file is user-editable text; a
    /// height of 40 metres or a head ratio of 0 must not produce an avatar that fills the screen
    /// or a NaN matrix.
    pub fn sanitized(mut self) -> Self {
        self.body.height = self.body.height.clamp(0.15, 2.2);
        self.body.head_ratio = self.body.head_ratio.clamp(0.08, 0.6);
        self.body.shoulder = self.body.shoulder.clamp(0.08, 0.5);
        self.body.limb = self.body.limb.clamp(0.015, 0.2);
        self.body.blockiness = self.body.blockiness.clamp(0.0, 1.0);
        self.tempo = self.tempo.clamp(0.3, 3.0);
        if self.name.trim().is_empty() {
            self.name = self.species.default_name().to_string();
        }
        self.name = self.name.chars().take(32).collect();
        self
    }

    /// Height in pixels at a given on-screen scale (used for placement/hit-testing).
    pub fn screen_height(&self, px_per_unit: f32) -> f32 {
        self.body.height * px_per_unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_renderable() {
        for p in presets() {
            let s = p.clone().sanitized();
            assert!(s.body.height >= 0.15 && s.body.height <= 2.2, "{} height", s.name);
            assert!(s.body.head_ratio > 0.0);
            assert!(!s.species.clips().is_empty());
        }
    }

    #[test]
    fn presets_cover_humans_and_animals() {
        let ps = presets();
        assert!(ps.iter().any(|p| p.species == Species::Human));
        assert!(ps.iter().any(|p| p.species == Species::Cat));
        assert!(ps.iter().any(|p| p.species == Species::Dog));
    }

    #[test]
    fn lookup_is_case_insensitive_by_name_or_species() {
        assert_eq!(Builtin::by_name("mochi").unwrap().species, Species::Cat);
        assert_eq!(Builtin::by_name("HUMAN").unwrap().species, Species::Human);
        assert!(Builtin::by_name("nobody").is_none());
    }

    #[test]
    fn absurd_config_is_clamped_not_rendered() {
        let b = Builtin {
            body: Body {
                height: 40.0,
                head_ratio: 0.0,
                shoulder: -1.0,
                limb: 9.0,
                blockiness: 4.0,
            },
            tempo: 99.0,
            name: "x".repeat(100),
            ..Default::default()
        };
        let s = b.sanitized();
        assert_eq!(s.body.height, 2.2);
        assert_eq!(s.body.head_ratio, 0.08);
        assert_eq!(s.body.shoulder, 0.08);
        assert_eq!(s.body.limb, 0.2);
        assert_eq!(s.body.blockiness, 1.0);
        assert_eq!(s.tempo, 3.0);
        assert_eq!(s.name.chars().count(), 32);
    }

    #[test]
    fn empty_name_falls_back_to_species_default() {
        let b = Builtin { name: "   ".into(), species: Species::Dog, ..Default::default() };
        assert_eq!(b.sanitized().name, "Biscuit");
    }

    #[test]
    fn body_plans_agree_with_clip_lists() {
        for p in presets() {
            let clips = p.species.clips();
            assert!(clips.contains(&"idle"));
            assert!(clips.contains(&"sleep"));
            if p.species.is_biped() {
                assert_eq!(p.species.legs(), 2);
            } else {
                assert_eq!(p.species.legs(), 4);
            }
        }
    }

    #[test]
    fn screen_height_uses_the_body_plan() {
        let b = Builtin { body: Body { height: 1.5, ..Default::default() }, ..Default::default() };
        assert!((b.screen_height(100.0) - 150.0).abs() < 0.01);
    }
}
