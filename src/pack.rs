//! Mutating an installed clip pack **at any time** — because characters are built over days.
//!
//! The realistic workflow for a companion is not a single install. It is:
//!
//! - today, account A makes `idle` and `walk`;
//! - tomorrow, account B makes `remind` and `water`;
//! - next week, the author replaces `walk` because the loop was bad.
//!
//! So a pack is not a folder you install once. It is a small database of named clips that you
//! add to, replace in, and remove from, at any time, without ever leaving the pet in a state
//! that will not load. That is the whole job of this module.
//!
//! Two rules make that safe:
//!
//! 1. **Every mutation is checked before it is committed.** A new clip is header-audited
//!    ([`crate::avatar::clips::sniff`]) and copied to a temporary name inside the pack; only if
//!    the resulting manifest still validates does it get renamed into place. A rejected clip
//!    leaves the pet exactly as it was.
//! 2. **You cannot remove the one clip the pet needs to load.** `idle` is refused with a message
//!    that says why, instead of leaving a folder that fails at startup. Everything else, `walk`
//!    included, can go: a pack built over days should never be a trap.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::pet::{self, Pet};

/// The outcome of a mutation, in terms the caller can print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub state: String,
    pub file: String,
    /// Replaced an existing clip rather than adding a new one.
    pub replaced: bool,
}

/// A mutation in progress. Rolls the pack back if it is dropped without committing.
///
/// Validating *before* the commit is what makes a rejected clip a no-op, but validation has to
/// see the new file under its final name. So the order is: back up, write, validate, and let the
/// guard undo everything on any error path — including a panic.
struct Tx<'a> {
    dir: &'a Path,
    manifest_before: String,
    /// (final name, backup name) of a clip that is being replaced.
    clip_backup: Option<(PathBuf, PathBuf)>,
    committed: bool,
}

impl<'a> Tx<'a> {
    fn begin(dir: &'a Path) -> Result<Self, String> {
        let manifest_before =
            std::fs::read_to_string(dir.join("pet.json")).map_err(|e| e.to_string())?;
        Ok(Self { dir, manifest_before, clip_backup: None, committed: false })
    }

    fn commit(&mut self) {
        self.committed = true;
        if let Some((_, backup)) = self.clip_backup.take() {
            let _ = std::fs::remove_file(backup);
        }
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let _ = std::fs::write(self.dir.join("pet.json"), &self.manifest_before);
        if let Some((final_path, backup)) = self.clip_backup.take() {
            let _ = std::fs::rename(&backup, &final_path);
        }
    }
}

/// Adds or replaces one clip in an installed pet.
///
/// `state` is validated against the known animation states; `src` is any file the user has.
/// The clip is renamed to `<state>.<ext>` inside the pack, so a user can add
/// `/home/me/Downloads/veo_export_8.webp` without thinking about file names.
pub fn add_clip(pet_dir: &Path, state: &str, src: &Path, _now: u64) -> Result<Changed, String> {
    if !crate::avatar::clips::is_clip_state(state) {
        return Err(format!(
            "{state:?} is not an animation state. Use one of: {}",
            crate::avatar::clips::CLIP_STATES
                .iter()
                .map(|(k, _)| *k)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !src.exists() {
        return Err(format!("{} does not exist", src.display()));
    }

    // 1. Audit before touching the pack. A rejected clip is a complete no-op.
    let info = crate::avatar::clips::sniff(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if !info.container.is_animated() {
        return Err(format!(
            "{} is a {} — {state} needs an animation with frames to play",
            src.display(),
            info.container.label()
        ));
    }
    if state == "walk" && info.loops != 0 {
        return Err(format!(
            "{} does not loop (loop count {}). A walk cycle must loop forever, or the pet \
             freezes mid-stride.",
            src.display(),
            info.loops
        ));
    }

    std::fs::create_dir_all(pet_dir).map_err(|e| e.to_string())?;
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "webp".into());
    let file = format!("{state}.{ext}");

    let mut tx = Tx::begin(pet_dir)?;
    let mut manifest = read_manifest(pet_dir)?;
    let previous = manifest.clips.insert(state.to_string(), file.clone());
    let replaced = previous.is_some();

    // Move any existing clip aside first, so the guard can put it back on any failure path.
    let final_path = pet_dir.join(&file);
    if final_path.exists() {
        let backup = pet_dir.join(format!(".{file}.backup"));
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&final_path, &backup).map_err(|e| e.to_string())?;
        tx.clip_backup = Some((final_path.clone(), backup));
    }
    std::fs::copy(src, &final_path).map_err(|e| format!("copying into the pack: {e}"))?;

    // 2. Validate the folder exactly as it would be on disk after the change.
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(pet_dir.join("pet.json"), &json).map_err(|e| e.to_string())?;

    if let Err(errs) = crate::pet::validate(&manifest, pet_dir) {
        return Err(errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n  "));
    }
    tx.commit();

    // 3. A replaced clip leaves the old file behind; remove it unless two states share it.
    if let Some(old) = previous {
        if old != file && !manifest.clips.values().any(|v| *v == old) {
            let _ = std::fs::remove_file(pet_dir.join(&old));
        }
    }

    Ok(Changed { state: state.to_string(), file, replaced })
}

/// Removes one clip. `idle` is refused, because a pack without it will not load; `walk` is not,
/// because a stationary pet is a valid pet.
pub fn remove_clip(pet_dir: &Path, state: &str) -> Result<Changed, String> {
    let mut manifest = read_manifest(pet_dir)?;
    let Some(file) = manifest.clips.get(state).cloned() else {
        return Err(format!("this pet has no {state} clip"));
    };
    // Only `idle` is load-bearing. `walk` is removable: a pet that stands still is a valid pet,
    // and someone rebuilding their whole set should not have to fight the tool for it.
    if state == "idle" {
        return Err("idle is required — a pack without it cannot load. Replace it instead \
             (tether ask --set-clip idle FILE). To change how the pet looks at rest, that is \
             the clip to overwrite."
            .to_string());
    }
    manifest.clips.remove(state);
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(pet_dir.join("pet.json"), json).map_err(|e| e.to_string())?;

    // Only delete the file if no other state points at it.
    if !manifest.clips.values().any(|v| *v == file) {
        let _ = std::fs::remove_file(pet_dir.join(&file));
    }
    Ok(Changed { state: state.to_string(), file, replaced: false })
}

/// Lists what a pack has, including anything broken, without modifying it.
pub fn inventory(pet_dir: &Path) -> Result<Vec<(String, Option<ClipLine>)>, String> {
    let manifest = read_manifest(pet_dir)?;
    Ok(crate::avatar::clips::CLIP_STATES
        .iter()
        .map(|(state, _)| {
            let line = manifest.clips.get(*state).and_then(|f| {
                let path = pet_dir.join(f);
                crate::avatar::clips::sniff(&path)
                    .ok()
                    .map(|info| ClipLine { file: f.clone(), info })
            });
            ((*state).to_string(), line)
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct ClipLine {
    pub file: String,
    pub info: crate::avatar::clips::ClipInfo,
}

fn read_manifest(pet_dir: &Path) -> Result<Pet, String> {
    let path = pet_dir.join("pet.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))
}

/// Where installed pets live under a data directory.
pub fn pets_dir(data: &Path) -> PathBuf {
    data.join("pets")
}

/// Resolves a pet name (or `default`) to an installed folder.
pub fn resolve_pet(data: &Path, name: Option<&str>) -> Result<PathBuf, String> {
    let dir = pets_dir(data);
    if let Some(name) = name {
        let p = dir.join(pet::slug(name));
        if p.exists() {
            return Ok(p);
        }
        // Allow a path too, for editing a pack before installing it.
        let direct = PathBuf::from(name);
        if direct.join("pet.json").exists() {
            return Ok(direct);
        }
        return Err(format!("no installed pet called {name:?} (looked in {})", dir.display()));
    }
    // No name: if there is exactly one pet, that is obviously the one.
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("pet.json").exists())
        .collect();
    found.sort();
    match found.len() {
        0 => Err("no pets installed yet — `tether ask --new NAME` makes one".into()),
        1 => Ok(found.remove(0)),
        _ => Err(format!(
            "{} pets installed — say which one: {}",
            found.len() + 1,
            found
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// A sorted view of a manifest's clips, for printing.
pub fn manifest_clips(pet_dir: &Path) -> BTreeMap<String, String> {
    read_manifest(pet_dir).map(|p| p.clips).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn animated_webp(w: u32, h: u32, frames: u32, ms: u32, loops: u16) -> Vec<u8> {
        let mut chunks = Vec::new();
        let mut vp8x = vec![0x02 | 0x10];
        vp8x.extend_from_slice(&[0, 0, 0]);
        vp8x.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        chunks.extend_from_slice(b"VP8X");
        chunks.extend_from_slice(&(vp8x.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&vp8x);
        let mut anim = vec![0, 0, 0, 0];
        anim.extend_from_slice(&loops.to_le_bytes());
        chunks.extend_from_slice(b"ANIM");
        chunks.extend_from_slice(&(anim.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&anim);
        for _ in 0..frames {
            let mut f = vec![0u8; 12];
            f.extend_from_slice(&ms.to_le_bytes()[..3]);
            f.push(0);
            chunks.extend_from_slice(b"ANMF");
            chunks.extend_from_slice(&(f.len() as u32).to_le_bytes());
            chunks.extend_from_slice(&f);
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
        out.extend_from_slice(b"WEBP");
        out.extend_from_slice(&chunks);
        out
    }

    /// A pack with the two required clips.
    fn a_pack() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("pets/amma");
        std::fs::create_dir_all(&dir).unwrap();
        for state in ["idle", "walk"] {
            std::fs::write(dir.join(format!("{state}.webp")), animated_webp(264, 280, 12, 58, 0))
                .unwrap();
        }
        let manifest = r#"{"name":"Amma","clips":{"idle":"idle.webp","walk":"walk.webp"}}"#;
        std::fs::write(dir.join("pet.json"), manifest).unwrap();
        (d, dir)
    }

    fn clip_file(tag: &str) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(format!("{tag}.webp"));
        std::fs::write(&p, animated_webp(264, 280, 30, 58, 0)).unwrap();
        (d, p)
    }

    #[test]
    fn a_new_clip_can_be_added_days_later() {
        let (_d, dir) = a_pack();
        let (_s, src) = clip_file("water");
        let c = add_clip(&dir, "water", &src, 0).unwrap();
        assert_eq!(c.state, "water");
        assert!(!c.replaced);
        // Renamed to the state, so the user never has to think about filenames.
        assert!(dir.join("water.webp").exists());
        let pet = pet::load(&dir).unwrap();
        assert_eq!(pet.clips.get("water").map(String::as_str), Some("water.webp"));
    }

    #[test]
    fn adding_the_same_state_twice_replaces_it_and_removes_the_old_file() {
        let (_d, dir) = a_pack();
        let (_s1, first) = clip_file("take1");
        add_clip(&dir, "water", &first, 0).unwrap();
        let (_s2, second) = clip_file("take2");
        let c = add_clip(&dir, "water", &second, 0).unwrap();
        assert!(c.replaced, "second add is a replacement");
        // Only one water clip remains on disk: no orphaned megabytes.
        let waters: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("water."))
            .collect();
        assert_eq!(waters.len(), 1, "old clip should be gone");
    }

    #[test]
    fn an_optional_clip_can_be_removed_but_idle_cannot() {
        let (_d, dir) = a_pack();
        let (_s, src) = clip_file("water");
        add_clip(&dir, "water", &src, 0).unwrap();

        // idle is the only clip a pack cannot live without.
        assert!(remove_clip(&dir, "idle").unwrap_err().contains("required"));
        // walk is removable: the pet just stops walking.
        assert!(remove_clip(&dir, "walk").is_ok());
        assert!(pet::load(&dir).is_ok(), "a stationary pet still loads");

        let c = remove_clip(&dir, "water").unwrap();
        assert_eq!(c.state, "water");
        assert!(!dir.join("water.webp").exists());
        assert!(pet::load(&dir).is_ok(), "the pet still loads after a removal");
    }

    #[test]
    fn removing_something_absent_says_so() {
        let (_d, dir) = a_pack();
        assert!(remove_clip(&dir, "sleep").unwrap_err().contains("no sleep clip"));
    }

    #[test]
    fn a_bad_clip_is_refused_without_touching_the_pack() {
        let (_d, dir) = a_pack();
        let before = std::fs::read_to_string(dir.join("pet.json")).unwrap();

        // Not an image at all.
        let junk = dir.join("junk.webp");
        std::fs::write(&junk, b"not an image").unwrap();
        assert!(add_clip(&dir, "water", &junk, 0).is_err());

        // A still image where an animation is required.
        let still = dir.join("still.webp");
        std::fs::write(&still, animated_webp(64, 64, 0, 0, 0)).unwrap();
        assert!(add_clip(&dir, "water", &still, 0).unwrap_err().contains("needs an animation"));

        // A bomb.
        let bomb = dir.join("bomb.webp");
        std::fs::write(&bomb, animated_webp(16000, 16000, 4, 40, 0)).unwrap();
        assert!(add_clip(&dir, "water", &bomb, 0).unwrap_err().contains("decompression bomb"));

        assert_eq!(
            std::fs::read_to_string(dir.join("pet.json")).unwrap(),
            before,
            "manifest untouched"
        );
        assert!(!dir.join("water.webp").exists(), "no partial clip left behind");
    }

    #[test]
    fn a_walk_clip_that_does_not_loop_is_refused() {
        let (_d, dir) = a_pack();
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("bad_walk.webp");
        std::fs::write(&src, animated_webp(264, 280, 10, 58, 3)).unwrap();
        let err = add_clip(&dir, "walk", &src, 0).unwrap_err();
        assert!(err.contains("does not loop"), "{err}");
    }

    #[test]
    fn an_unknown_state_is_refused_and_the_valid_ones_are_listed() {
        let (_d, dir) = a_pack();
        let (_s, src) = clip_file("x");
        let err = add_clip(&dir, "teleport", &src, 0).unwrap_err();
        assert!(err.contains("idle") && err.contains("walk"), "{err}");
    }

    #[test]
    fn inventory_reports_every_state_including_the_empty_ones() {
        let (_d, dir) = a_pack();
        let inv = inventory(&dir).unwrap();
        assert_eq!(inv.len(), crate::avatar::clips::CLIP_STATES.len());
        let water = inv.iter().find(|(s, _)| s == "water").unwrap();
        assert!(water.1.is_none(), "water is not there yet");
        let idle = inv.iter().find(|(s, _)| s == "idle").unwrap();
        assert_eq!(idle.1.as_ref().unwrap().info.frames, 12);
    }

    #[test]
    fn clips_arrive_from_different_days_and_accounts_without_breaking_anything() {
        // The actual workflow: idle+walk from account A, then remind+water from account B,
        // then a better walk next week. The pet must be loadable after every single step.
        let (_d, dir) = a_pack();
        assert!(pet::load(&dir).is_ok());

        for state in ["remind", "water", "stretch"] {
            let (_s, src) = clip_file(state);
            add_clip(&dir, state, &src, 0).unwrap();
            assert!(pet::load(&dir).is_ok(), "loadable after adding {state}");
        }

        let (_s, better) = clip_file("better_walk");
        add_clip(&dir, "walk", &better, 0).unwrap();
        assert!(pet::load(&dir).is_ok(), "loadable after replacing walk");

        remove_clip(&dir, "stretch").unwrap();
        assert!(pet::load(&dir).is_ok(), "loadable after removing stretch");

        let pet = pet::load(&dir).unwrap();
        assert_eq!(pet.clips.len(), 4, "idle, walk, remind, water");
    }

    #[test]
    fn resolve_pet_finds_the_only_pet_and_asks_when_there_are_several() {
        let d = tempfile::tempdir().unwrap();
        let data = d.path();
        assert!(resolve_pet(data, None).unwrap_err().contains("no pets installed"));

        let (_a, one) = a_pack();
        std::fs::create_dir_all(pets_dir(data)).unwrap();
        copy_dir(&one, &pets_dir(data).join("amma"));
        assert!(resolve_pet(data, None).is_ok(), "one pet is unambiguous");

        let (_b, two) = a_pack();
        copy_dir(&two, &pets_dir(data).join("biscuit"));
        let err = resolve_pet(data, None).unwrap_err();
        assert!(err.contains("amma") && err.contains("biscuit"), "{err}");
        assert!(resolve_pet(data, Some("amma")).is_ok());
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}
