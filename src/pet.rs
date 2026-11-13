//! Desktop companion: pet manifests, validation and procedural generation.
//!
//! A pet is "two files in a folder" (see `docs/pet-format.md`), because the point is that you
//! can make one. This module owns the part that must be strict: **validation**. A pet is
//! content you downloaded, so it is untrusted input, and the checks in `validate` are the
//! difference between a cute desktop character and a way to hijack a speech bubble.
//!
//! What this module does *not* do: draw. Rendering lives in the GUI (`gui/`), and deliberately
//! so: the capture core must never depend on a graphics stack.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
pub const MAX_SPRITE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_NAME_CHARS: usize = 32;
/// A 3D avatar is a bigger file than a sprite sheet, but not unbounded: this is the same cap
/// the audit in [`crate::avatar::audit`] enforces, kept here so a manifest can be rejected
/// before the file is even opened.
pub const MAX_MODEL_BYTES: u64 = crate::avatar::audit::MAX_AVATAR_BYTES;
/// Whole clip pack ceiling. Ten clips at ~1 MB each is already generous for a desktop pet.
pub const MAX_CLIP_PACK_BYTES: u64 = 24 * 1024 * 1024;
pub const MAX_SPEECH_LINE: usize = 120;
pub const MAX_SPEECH_LINES: usize = 24;

/// PNG magic, checked before we hand the file to any decoder.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Animation {
    /// Frames in this row of the sheet.
    pub frames: u8,
    /// Column offset in the sheet (rows are `idle, walk, sleep, remind, celebrate`).
    pub row: u8,
}

/// A rigged 3D avatar (`VRM` for humans, `glTF`/`GLB` for anything else).
///
/// Optional: a pet with a `model` renders in 3D, and a pet without one keeps using its
/// `sprite.png`. The two paths are independent on purpose — 2D pets must never break because
/// 3D exists, and a machine without a GPU must still be able to show a pet.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Model {
    /// File name inside the pet folder. Must be a bare name: no directories, no `..`.
    pub file: String,
    /// Size multiplier, 0.25–4.0.
    pub scale: f32,
    /// Where the model's origin sits: `bottom` (default), `center` or `top`.
    pub anchor: String,
    /// Vertical nudge in world units, -2.0–2.0. For a model whose origin is not at its feet.
    pub y_offset: f32,
    /// Clip names to use for each state, if the file has them.
    pub clips: Clips,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Clips {
    pub idle: String,
    pub walk: String,
    pub sleep: String,
    pub remind: String,
    pub celebrate: String,
}

impl Model {
    pub fn is_set(&self) -> bool {
        !self.file.trim().is_empty()
    }

    /// Validates the model *declaration*. The file itself is audited separately by
    /// [`crate::avatar::audit`], which is the only place allowed to look inside it.
    fn validate(&self, dir: &Path, errs: &mut Vec<Invalid>) {
        if !self.is_set() {
            return;
        }
        // Path traversal: a manifest is downloaded content, so `../../../etc/passwd` in a `file`
        // field is the obvious attack. Only a bare file name is allowed.
        let f = self.file.trim();
        if f.contains('/') || f.contains('\\') || f.contains("..") || f.starts_with('.') {
            errs.push(Invalid {
                field: "model.file",
                reason: format!("{f:?} must be a bare file name, not a path"),
            });
            return;
        }
        let ext = Path::new(f)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "vrm" | "glb" | "gltf") {
            errs.push(Invalid {
                field: "model.file",
                reason: format!("{f:?} is not .vrm, .glb or .gltf"),
            });
        }
        if !(0.25..=4.0).contains(&self.scale) || self.scale.is_nan() {
            errs.push(Invalid {
                field: "model.scale",
                reason: format!("{} is outside 0.25..=4.0", self.scale),
            });
        }
        if !matches!(self.anchor.as_str(), "bottom" | "center" | "top") {
            errs.push(Invalid {
                field: "model.anchor",
                reason: format!("{:?} is not bottom|center|top", self.anchor),
            });
        }
        if !(-2.0..=2.0).contains(&self.y_offset) || self.y_offset.is_nan() {
            errs.push(Invalid {
                field: "model.y_offset",
                reason: format!("{} is outside -2.0..=2.0", self.y_offset),
            });
        }
        for (field, v) in [
            ("model.clips.idle", &self.clips.idle),
            ("model.clips.walk", &self.clips.walk),
            ("model.clips.sleep", &self.clips.sleep),
            ("model.clips.remind", &self.clips.remind),
            ("model.clips.celebrate", &self.clips.celebrate),
        ] {
            if let Err(e) = clean_text(field, v, 64) {
                errs.push(e);
            }
        }

        let path = dir.join(f);
        match std::fs::metadata(&path) {
            Err(_) => errs.push(Invalid {
                field: "model.file",
                reason: format!("{f} is missing beside pet.json"),
            }),
            Ok(m) => {
                if m.len() > MAX_MODEL_BYTES {
                    errs.push(Invalid {
                        field: "model.file",
                        reason: format!("{} bytes, limit is {MAX_MODEL_BYTES}", m.len()),
                    });
                } else if let Err(reason) = check_model_header(&path) {
                    errs.push(Invalid { field: "model.file", reason });
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Pet {
    pub name: String,
    pub author: String,
    pub version: u32,
    pub frame_size: u32,
    pub scale: u32,
    pub anchor: String,
    pub fps: Fps,
    pub speech: Speech,
    pub palette: Vec<String>,
    pub license: String,
    /// Optional rigged 3D avatar. Independent of `sprite.png`.
    pub model: Model,
    /// Optional **clip pack**: state name (`idle`, `walk`, `water`, …) to animated image file.
    ///
    /// This is the format realistic AI-generated characters arrive in: a character image, run
    /// through an image-to-video model, delivered as animated WebP with a real alpha channel.
    /// Full-frame art, no rig, no engine, no GPU. A pet with `clips` needs no sprite and no
    /// model, and every clip is header-audited before the renderer sees it.
    pub clips: BTreeMap<String, String>,
    /// Filled in by the loader: where the files actually are.
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl Default for Pet {
    fn default() -> Self {
        Self {
            name: "tether".into(),
            author: "you".into(),
            version: 1,
            frame_size: 32,
            scale: 2,
            anchor: "bottom".into(),
            fps: Fps::default(),
            speech: Speech::default(),
            palette: Vec::new(),
            license: "CC0".into(),
            model: Model::default(),
            clips: BTreeMap::new(),
            dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Fps {
    pub idle: u8,
    pub walk: u8,
    pub sleep: u8,
    pub remind: u8,
    pub celebrate: u8,
}

impl Default for Fps {
    fn default() -> Self {
        Self { idle: 4, walk: 8, sleep: 2, remind: 6, celebrate: 10 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Speech {
    pub water: Vec<String>,
    #[serde(rename = "break")]
    pub break_: Vec<String>,
    pub posture: Vec<String>,
    pub recovered: Vec<String>,
    pub welcome_back: Vec<String>,
}

impl Speech {
    pub fn for_kind(&self, key: &str) -> Option<&[String]> {
        let v = match key {
            "water" => &self.water,
            "break" => &self.break_,
            "posture" => &self.posture,
            "recovered" => &self.recovered,
            "welcome_back" => &self.welcome_back,
            _ => return None,
        };
        if v.is_empty() {
            None
        } else {
            Some(v.as_slice())
        }
    }

    fn all(&self) -> Vec<(&'static str, &Vec<String>)> {
        vec![
            ("water", &self.water),
            ("break", &self.break_),
            ("posture", &self.posture),
            ("recovered", &self.recovered),
            ("welcome_back", &self.welcome_back),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub field: &'static str,
    pub reason: String,
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

/// Characters that must never appear in pet-supplied text that reaches a UI.
///
/// Bidi overrides are the sneaky one: `U+202E` visually reverses a substring, so a reminder
/// that reads "water break" in the manifest can *display* as something else entirely. A pet is
/// downloaded content, so this is a real injection path, not a theoretical one.
fn is_forbidden_char(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{202A}'..='\u{202E}' | // LRE RLE PDF LRO RLO
            '\u{2066}'..='\u{2069}' | // LRI RLI FSI PDI
            '\u{200E}' | '\u{200F}'   // LRM RLM
        )
}

fn clean_text(field: &'static str, s: &str, max: usize) -> Result<(), Invalid> {
    if s.chars().count() > max {
        return Err(Invalid {
            field,
            reason: format!("{} chars, limit is {max}", s.chars().count()),
        });
    }
    if let Some(c) = s.chars().find(|c| is_forbidden_char(*c)) {
        return Err(Invalid {
            field,
            reason: format!("contains a control or text-direction character (U+{:04X})", c as u32),
        });
    }
    Ok(())
}

/// Validates the manifest *and* the files it points at. Strict on purpose: a pet that cannot
/// be validated is refused with a message that says exactly what to fix.
pub fn validate(pet: &Pet, dir: &Path) -> Result<(), Vec<Invalid>> {
    let mut errs = Vec::new();

    if pet.name.trim().is_empty() {
        errs.push(Invalid { field: "name", reason: "is empty".into() });
    }
    if let Err(e) = clean_text("name", &pet.name, MAX_NAME_CHARS) {
        errs.push(e);
    }
    if !pet.author.is_empty() {
        if let Err(e) = clean_text("author", &pet.author, 64) {
            errs.push(e);
        }
    }
    if pet.version != 1 {
        errs.push(Invalid {
            field: "version",
            reason: format!("unsupported ({}, this build reads 1)", pet.version),
        });
    }

    // Frame size: power of two in 16..=128. Powers of two keep pixel art crisp at every scale
    // and make a maliciously huge sheet fail early.
    if !pet.frame_size.is_power_of_two() || !(16..=128).contains(&pet.frame_size) {
        errs.push(Invalid {
            field: "frame_size",
            reason: format!("{} is not a power of two in 16..=128", pet.frame_size),
        });
    }
    if !(1..=8).contains(&pet.scale) {
        errs.push(Invalid { field: "scale", reason: format!("{} is outside 1..=8", pet.scale) });
    }
    if !matches!(pet.anchor.as_str(), "bottom" | "center" | "top") {
        errs.push(Invalid {
            field: "anchor",
            reason: format!("{:?} is not bottom|center|top", pet.anchor),
        });
    }
    // A 10000-fps pet is a battery-flattening tool, not a pet.
    for (field, v) in [
        ("fps.idle", pet.fps.idle),
        ("fps.walk", pet.fps.walk),
        ("fps.sleep", pet.fps.sleep),
        ("fps.remind", pet.fps.remind),
        ("fps.celebrate", pet.fps.celebrate),
    ] {
        if !(1..=30).contains(&v) {
            errs.push(Invalid { field, reason: format!("{v} is outside 1..=30") });
        }
    }

    for (key, lines) in pet.speech.all() {
        if lines.len() > MAX_SPEECH_LINES {
            errs.push(Invalid {
                field: "speech",
                reason: format!("{key} has {} lines, limit is {MAX_SPEECH_LINES}", lines.len()),
            });
        }
        for line in lines {
            if line.chars().count() > MAX_SPEECH_LINE {
                errs.push(Invalid {
                    field: "speech",
                    reason: format!(
                        "{key} line is {} chars, limit is {MAX_SPEECH_LINE}",
                        line.chars().count()
                    ),
                });
            }
            for c in line.chars().filter(|c| is_forbidden_char(*c)) {
                errs.push(Invalid {
                    field: "speech",
                    reason: format!("{key} line contains U+{:04X}", c as u32),
                });
            }
        }
    }

    for p in &pet.palette {
        let ok =
            p.len() == 7 && p.starts_with('#') && p[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            errs.push(Invalid { field: "palette", reason: format!("{p:?} is not #rrggbb") });
        }
    }

    pet.model.validate(dir, &mut errs);
    validate_clips(pet, dir, &mut errs);

    // A pet needs exactly one kind of art. Saying so is kinder than "sprite.png missing".
    let has_clips = !pet.clips.is_empty();
    let has_model = pet.model.is_set();
    if has_clips && has_model {
        errs.push(Invalid {
            field: "clips",
            reason: "a pet uses either `clips` or `model`, not both — pick the one you have".into(),
        });
    }

    // Files. A pet needs art: a sprite sheet, a 3D model, or both. A pet with a working model
    // is allowed to ship no sprite at all, which is the point of the 3D path.
    if (pet.model.is_set() || has_clips) && !dir.join("sprite.png").exists() {
        return if errs.is_empty() { Ok(()) } else { Err(errs) };
    }
    let sprite = dir.join("sprite.png");
    match std::fs::metadata(&sprite) {
        Err(_) => {
            errs.push(Invalid { field: "sprite.png", reason: "missing beside pet.json".into() })
        }
        Ok(m) => {
            if m.len() > MAX_SPRITE_BYTES {
                errs.push(Invalid {
                    field: "sprite.png",
                    reason: format!("{} bytes, limit is {}", m.len(), MAX_SPRITE_BYTES),
                });
            } else if let Ok(head) = read_head(&sprite, 8) {
                if head != PNG_MAGIC {
                    errs.push(Invalid { field: "sprite.png", reason: "is not a PNG".into() });
                }
            }
            if let Ok((w, h)) = png_size(&sprite) {
                let fs = pet.frame_size;
                if w == 0 || h == 0 || w % fs != 0 || h % fs != 0 {
                    errs.push(Invalid {
                        field: "sprite.png",
                        reason: format!("{w}x{h} is not a multiple of frame_size {fs}"),
                    });
                } else if w / fs > 8 {
                    errs.push(Invalid {
                        field: "sprite.png",
                        reason: format!("{} columns, more than the 8 a row may hold", w / fs),
                    });
                }
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

fn read_head(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf)?;
    buf.truncate(got);
    Ok(buf)
}

/// Validates a clip pack: known state names, bare file names, and a header audit of every clip.
///
/// The audit is the reason this exists. An animated WebP is a container whose header can claim a
/// 16000×16000 canvas and 100 000 frames; the decoder believes it and allocates. Reading the
/// header costs a few hundred bytes and lets us refuse the file before any of that happens.
fn validate_clips(pet: &Pet, dir: &Path, errs: &mut Vec<Invalid>) {
    if pet.clips.is_empty() {
        return;
    }
    // Only `idle` is required. A pack is assembled over days and across accounts, so requiring
    // `walk` on day one would make the intended workflow impossible. A pet with no walk cycle
    // stands still, which is a valid pet; `tether ask --clips` shows what is still missing.
    if !pet.clips.contains_key("idle") {
        errs.push(Invalid {
            field: "clips",
            reason: "a clip pack needs `idle` — it is what the pet plays at rest. \
                     Add it with: tether ask --set-clip idle FILE"
                .into(),
        });
    }
    let mut total_bytes: u64 = 0;
    for (state, file) in &pet.clips {
        if !crate::avatar::clips::is_clip_state(state) {
            let known: Vec<&str> =
                crate::avatar::clips::CLIP_STATES.iter().map(|(k, _)| *k).collect();
            errs.push(Invalid {
                field: "clips",
                reason: format!("{state:?} is not an animation state (try {})", known.join(", ")),
            });
            continue;
        }
        if let Err(e) = clean_text("clips", file, 128) {
            errs.push(e);
            continue;
        }
        let f = file.trim();
        if f.contains('/') || f.contains('\\') || f.contains("..") || f.starts_with('.') {
            errs.push(Invalid {
                field: "clips",
                reason: format!("{f:?} must be a bare file name, not a path"),
            });
            continue;
        }
        let path = dir.join(f);
        match std::fs::metadata(&path) {
            Err(_) => errs.push(Invalid {
                field: "clips",
                reason: format!("{state}: {f} is missing beside pet.json"),
            }),
            Ok(m) => {
                total_bytes = total_bytes.saturating_add(m.len());
                match crate::avatar::clips::sniff(&path) {
                    Ok(info) => {
                        if !info.container.is_animated() {
                            errs.push(Invalid {
                                field: "clips",
                                reason: format!(
                                    "{state}: {f} is a still image — {state} needs frames to play"
                                ),
                            });
                        } else if state == "walk" && (info.loops != 0) {
                            // A walk cycle that plays once then freezes is a bug the author
                            // cannot see in a browser preview, so say it here.
                            errs.push(Invalid {
                                field: "clips",
                                reason: format!(
                                    "{state}: {f} does not loop (loop count {}); a walk cycle \
                                     should loop forever",
                                    info.loops
                                ),
                            });
                        }
                    }
                    Err(e) => {
                        errs.push(Invalid { field: "clips", reason: format!("{state}: {f} {e}") })
                    }
                }
            }
        }
    }
    if total_bytes > MAX_CLIP_PACK_BYTES {
        errs.push(Invalid {
            field: "clips",
            reason: format!(
                "the pack is {total_bytes} bytes, limit is {MAX_CLIP_PACK_BYTES}. Re-encode the \
                 clips smaller; 264×280 at 17 fps is what the built-ins use."
            ),
        });
    }
}

/// GLB header (`glTF`, version, declared length) or a JSON-looking `.gltf`/`.vrm`.
///
/// Header only: never parses the JSON, never touches a buffer. The deep inspection happens in
/// [`crate::avatar::audit`], on demand, so opening a pet folder stays cheap.
fn check_model_header(path: &Path) -> Result<(), String> {
    let head = read_head(path, 12).map_err(|e| e.to_string())?;
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("gltf") => {
            let first = head.iter().copied().find(|c| !c.is_ascii_whitespace());
            if first != Some(b'{') {
                return Err("does not start with a JSON object".into());
            }
            Ok(())
        }
        _ => {
            if head.len() < 12 || &head[..4] != b"glTF" {
                return Err("is not a binary glTF (missing glTF magic)".into());
            }
            let version = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
            if version != 2 {
                return Err(format!("binary glTF version {version}, this build reads 2"));
            }
            Ok(())
        }
    }
}

/// Reads width/height straight from the IHDR chunk.
///
/// Deliberately does **not** decode pixels: a 60000×60000 PNG is a valid file that costs 14 GB
/// to decompress. Reading the header costs 33 bytes and lets us refuse it before any allocator
/// sees it.
pub fn png_size(path: &Path) -> std::io::Result<(u32, u32)> {
    let head = read_head(path, 33)?;
    if head.len() < 24 || head[..8] != PNG_MAGIC {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a PNG"));
    }
    // IHDR length (4) + type (4) then width (4) and height (4), big endian.
    let w = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
    let h = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
    Ok((w, h))
}

/// Loads `<dir>/pet.json` and validates it.
pub fn load(dir: &Path) -> Result<Pet, Vec<Invalid>> {
    let manifest = dir.join("pet.json");
    let meta = std::fs::metadata(&manifest).map_err(|_| {
        vec![Invalid { field: "pet.json", reason: format!("not found in {}", dir.display()) }]
    })?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(vec![Invalid {
            field: "pet.json",
            reason: format!("{} bytes, limit is {MAX_MANIFEST_BYTES}", meta.len()),
        }]);
    }
    let raw = std::fs::read_to_string(&manifest)
        .map_err(|e| vec![Invalid { field: "pet.json", reason: e.to_string() }])?;
    let mut pet: Pet = serde_json::from_str(&raw)
        .map_err(|e| vec![Invalid { field: "pet.json", reason: e.to_string() }])?;
    validate(&pet, dir)?;
    pet.dir = Some(dir.to_path_buf());
    Ok(pet)
}

/// Lists installed pets, skipping (and reporting) any that fail validation. A broken third-party
/// pet must never stop the app from starting.
pub fn installed(pets_dir: &Path) -> (Vec<Pet>, Vec<(PathBuf, String)>) {
    let mut good = Vec::new();
    let mut bad = Vec::new();
    let Ok(entries) = std::fs::read_dir(pets_dir) else {
        return (good, bad);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match load(&path) {
            Ok(p) => good.push(p),
            Err(errs) => {
                let joined = errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ");
                bad.push((path, joined));
            }
        }
    }
    good.sort_by_key(|a| a.name.to_lowercase());
    (good, bad)
}

/// Installs a pet folder after validating it. Refuses to overwrite an existing pet, because
/// silently replacing someone's art is rude.
pub fn install(src: &Path, pets_dir: &Path) -> Result<PathBuf, String> {
    let pet = load(src)
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n  "))?;
    let dest = pets_dir.join(slug(&pet.name));
    if dest.exists() {
        return Err(format!(
            "a pet named {:?} is already installed at {} — remove it first",
            pet.name,
            dest.display()
        ));
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    // Copy every file the manifest names, not a hardcoded pair. The old list silently produced
    // a broken install for any pet with clips or a 3D model — the folder looked fine until the
    // app tried to load an image that was never copied.
    let mut wanted = vec![PathBuf::from("pet.json")];
    if src.join("sprite.png").exists() {
        wanted.push(PathBuf::from("sprite.png"));
    }
    if pet.model.is_set() {
        wanted.push(PathBuf::from(pet.model.file.trim()));
    }
    for file in pet.clips.values() {
        wanted.push(PathBuf::from(file.trim()));
    }
    wanted.extend(std::fs::read_dir(src).into_iter().flatten().flatten().filter_map(|e| {
        // A `.companion` bundle may ship extras (a licence, a preview); copy anything small.
        let p = e.path();
        let name = p.file_name()?.to_string_lossy().to_string();
        let ok =
            p.is_file() && (name.ends_with(".txt") || name.ends_with(".md") || name == "LICENSE");
        ok.then(|| PathBuf::from(name))
    }));

    for f in wanted {
        let from = src.join(&f);
        let to = dest.join(&f);
        std::fs::copy(&from, &to).map_err(|e| format!("copying {}: {e}", f.display()))?;
    }
    Ok(dest)
}

/// Filesystem-safe name from a display name.
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() {
        "pet".to_string()
    } else {
        s.chars().take(40).collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Procedural generation: a valid, animated pet with no drawing tools.
// ---------------------------------------------------------------------------------------------

pub struct Sprite {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major.
    pub pixels: Vec<u8>,
}

impl Sprite {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; (width * height * 4) as usize] }
    }

    fn put(&mut self, x: i32, y: i32, rgba: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        // Simple source-over so the blob's shading layers read correctly.
        let a = rgba[3] as f32 / 255.0;
        if a >= 1.0 {
            self.pixels[i..i + 4].copy_from_slice(&rgba);
            return;
        }
        for (c, source) in rgba[..3].iter().enumerate() {
            let dst = self.pixels[i + c] as f32;
            self.pixels[i + c] = (*source as f32 * a + dst * (1.0 - a)).round() as u8;
        }
        self.pixels[i + 3] = 255;
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, rgba);
            }
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, r: i32, rgba: [u8; 4]) {
        for yy in cy - r..=cy + r {
            for xx in cx - r..=cx + r {
                let dx = xx - cx;
                let dy = yy - cy;
                if dx * dx + dy * dy <= r * r {
                    self.put(xx, yy, rgba);
                }
            }
        }
    }

    fn encode_png(&self) -> Vec<u8> {
        encode_rgba_png(self.width, self.height, &self.pixels)
    }
}

pub fn hex_rgb(s: &str) -> [u8; 3] {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return [0x8b, 0x5c, 0xf6];
    }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0x80);
    [p(0), p(2), p(4)]
}

/// Draws the reference pet: a squash-and-stretch blob with eyes. Six cells of pixel art, so a
/// first-time author has something that already feels alive to paint over.
///
/// Layout matches the format spec: rows idle(4), walk(4), sleep(2), remind(2), celebrate(3).
pub fn generate(fs: u32, body: [u8; 3], accent: [u8; 3]) -> Sprite {
    const ROWS: &[(u8, u8)] = &[(4, 0), (4, 1), (2, 2), (2, 3), (3, 4)];
    let cols: u32 = ROWS.iter().map(|(f, _)| *f as u32).max().unwrap_or(4);
    let mut sheet = Sprite::new(fs * cols, fs * 5);
    let dark = [body[0] / 3, body[1] / 3, body[2] / 3, 255];
    let outline = [body[0] / 5, body[1] / 5, body[2] / 5, 255];
    let light =
        [body[0].saturating_add(40), body[1].saturating_add(30), body[2].saturating_add(20), 255];
    let glint = [255u8, 255, 255, 210];
    let accent_rgba = [accent[0], accent[1], accent[2], 255];

    // One 32px reference grid, scaled to whatever `fs` is, so the same art works at every size.
    let u = (fs as f32 / 32.0).max(1.0);
    let px = |v: f32| (v * u).round() as i32;
    let cx = fs as i32 / 2;

    for (row, (frames, _)) in ROWS.iter().enumerate() {
        for frame in 0..*frames {
            // Draw into a frame-sized canvas, then place it. Drawing straight into the sheet
            // with an offset is how you get art bleeding across cell boundaries.
            let mut cell = Sprite::new(fs, fs);
            let f = frame as f32;
            let (squash, lift, blink, lean) = match row {
                0 => {
                    let phase = (f / 4.0) * std::f32::consts::TAU;
                    (phase.sin() * 0.10, (phase.cos() * 1.2).abs(), frame == 2, 0.0)
                }
                1 => {
                    let phase = (f / 4.0) * std::f32::consts::TAU;
                    (phase.sin() * 0.06, (phase.sin() * 1.6).abs(), false, phase.cos() * 1.5)
                }
                2 => (0.14, -0.6, true, 0.0),
                3 => (-0.06, 1.4, false, 0.0),
                _ => {
                    let phase = (f / 3.0) * std::f32::consts::TAU;
                    (phase.sin() * 0.12, phase.sin().abs() * 2.6, false, 0.0)
                }
            };

            let rw = (px(9.0 * (1.0 + squash))).max(3);
            let rh = (px(9.0 * (1.0 - squash * 0.8))).max(3);
            let cy = px(18.0) - px(lift) - rh / 2;
            let bx = cx + px(lean);

            // Body: outline first, then fill, then two tone layers. A one-pixel outline is the
            // single biggest readability win in pixel art — without it the blob reads as a
            // smudge once it sits on a wallpaper of a similar hue.
            cell.disc(bx, cy, rw + 1, outline);
            cell.disc(bx, cy, rw, [body[0], body[1], body[2], 255]);
            cell.disc(bx, cy + rh / 3, (rw - px(1.0)).max(2), [body[0], body[1], body[2], 255]);
            cell.disc(bx - rw / 3, cy - rh / 4, (rw / 4).max(1), light);

            // Eyes: small blocks, not discs. A radius-1 disc is a plus sign, which reads as a
            // sparkle rather than a face.
            let eye_dx = px(3.2).max(2);
            let eye_dy = -px(1.0);
            let ew = px(1.8).max(1);
            let eh = if blink { (u as i32).max(1) } else { px(2.4).max(1) };
            for side in [-1, 1] {
                let ex = bx + side * eye_dx - ew / 2;
                cell.rect(ex, cy + eye_dy - (eh - 1) / 2, ew, eh, dark);
                // A single white pixel only once there are enough pixels to spare: at 16/32px
                // it swallows the eye and reads as a caret instead of a glint.
                if !blink && fs >= 48 {
                    cell.put(ex, cy + eye_dy - (eh - 1) / 2, glint);
                }
            }

            // A two-pixel mouth. Costs nothing, and it is most of the difference between
            // "a coloured circle" and "a creature you don't mind having on your desktop".
            if !blink && fs >= 32 {
                let mw = px(2.2).max(1);
                let my = cy + eye_dy + px(3.4).max(2);
                cell.rect(bx - mw / 2, my, mw, (u as i32).max(1), dark);
                if row == 4 {
                    // Open smile when celebrating.
                    cell.rect(bx - mw / 2, my + (u as i32).max(1), mw, (u as i32).max(1), dark);
                }
            }

            // Feet, so it stands on the taskbar instead of hovering.
            let foot_w = px(2.5).max(1);
            let foot_y = cy + rh - px(0.5);
            let spread = px(3.0).max(1);
            cell.rect(bx - spread - foot_w / 2, foot_y, foot_w, foot_w.max(1), dark);
            cell.rect(bx + spread - foot_w / 2, foot_y, foot_w, foot_w.max(1), dark);

            // The "remind" pose gets an accent mark above it, so attention is unmistakable.
            if row == 3 {
                cell.rect(
                    bx + rw + px(0.5),
                    cy - rh,
                    (u as i32).max(1),
                    px(3.0).max(1),
                    accent_rgba,
                );
            }

            blit(&mut sheet, &cell, frame as i32 * fs as i32, row as i32 * fs as i32);
        }
    }
    sheet
}

/// Source-over copy of an entire frame into the sheet at `(ox, oy)`.
fn blit(dst: &mut Sprite, src: &Sprite, ox: i32, oy: i32) {
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            let a = src.pixels[i + 3];
            if a == 0 {
                continue;
            }
            dst.put(ox + x, oy + y, [src.pixels[i], src.pixels[i + 1], src.pixels[i + 2], a]);
        }
    }
}

/// Minimal RGBA PNG encoder (single IDAT, filter type 0). ~60 lines instead of a dependency,
/// which keeps the "no bloat" promise and means the generator cannot pull a C library in.
pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *entry = c;
        }
        let mut crc = 0xFFFF_FFFFu32;
        for b in data {
            crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(body.len() + 12);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut typed = kind.to_vec();
        typed.extend_from_slice(body);
        out.extend_from_slice(&typed);
        out.extend_from_slice(&crc32(&typed).to_be_bytes());
        out
    }

    // zlib container with stored (uncompressed) deflate blocks — valid, and the file is tiny.
    let stride = (width * 4) as usize;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0u8); // filter: none
        raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut pos = 0usize;
    while pos < raw.len() {
        let n = (raw.len() - pos).min(65535);
        let last = pos + n == raw.len();
        zlib.push(if last { 1 } else { 0 });
        zlib.extend_from_slice(&(n as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(n as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + n]);
        pos += n;
    }
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in &raw {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&PNG_MAGIC);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, no interlace
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IDAT", &zlib));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

/// Writes a complete, valid pet folder. Used by `tether ask --new`.
pub fn scaffold(
    dir: &Path,
    name: &str,
    frame_size: u32,
    body_hex: &str,
    accent_hex: &str,
) -> Result<Pet, String> {
    let pet = Pet {
        name: name.to_string(),
        frame_size: frame_size.clamp(16, 128).next_power_of_two(),
        palette: vec![body_hex.to_string(), accent_hex.to_string()],
        ..Default::default()
    };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let sprite = generate(pet.frame_size, hex_rgb(body_hex), hex_rgb(accent_hex));
    std::fs::write(dir.join("sprite.png"), sprite.encode_png()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&pet).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("pet.json"), json).map_err(|e| e.to_string())?;
    validate(&pet, dir)
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?;
    Ok(Pet { dir: Some(dir.to_path_buf()), ..pet })
}

/// Loads the pet's colours, for the GUI's palette-matched speech bubble.
pub fn mood_colors(pet: &Pet) -> ([u8; 3], [u8; 3]) {
    let body = pet.palette.first().map(|c| hex_rgb(c)).unwrap_or([0x8b, 0x5c, 0xf6]);
    let accent = pet.palette.get(1).map(|c| hex_rgb(c)).unwrap_or([0xf4, 0x72, 0xb6]);
    (body, accent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_pet(dir: &Path, pet: &Pet) {
        std::fs::write(dir.join("pet.json"), serde_json::to_string_pretty(pet).unwrap()).unwrap();
        let sprite = generate(pet.frame_size, [0x8b, 0x5c, 0xf6], [0xf4, 0x72, 0xb6]);
        std::fs::write(dir.join("sprite.png"), sprite.encode_png()).unwrap();
    }

    /// Minimal but structurally valid GLB: magic, version 2, declared length, one JSON chunk.
    fn glb_bytes() -> Vec<u8> {
        let json = br#"{"asset":{"version":"2.0"},"nodes":[]}"#;
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        let total = 12 + 8 + json.len();
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(json);
        out
    }

    fn pet_with_model(file: &str) -> Pet {
        Pet {
            name: "Mochi".into(),
            model: Model {
                file: file.into(),
                scale: 1.0,
                anchor: "bottom".into(),
                y_offset: 0.0,
                clips: Clips::default(),
            },
            ..Pet::default()
        }
    }

    #[test]
    fn a_3d_pet_needs_no_sprite() {
        let d = tmp();
        std::fs::write(d.path().join("model.glb"), glb_bytes()).unwrap();
        let pet = pet_with_model("model.glb");
        std::fs::write(d.path().join("pet.json"), serde_json::to_string(&pet).unwrap()).unwrap();
        assert!(validate(&pet, d.path()).is_ok(), "model alone is enough art");
        assert!(load(d.path()).is_ok());
    }

    #[test]
    fn a_pet_with_neither_art_is_still_refused() {
        let d = tmp();
        let pet = Pet::default();
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "sprite.png"));
    }

    #[test]
    fn a_2d_pet_is_unaffected_by_the_model_field() {
        let d = tmp();
        let pet = Pet::default();
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_ok());
        let loaded = load(d.path()).unwrap();
        assert!(!loaded.model.is_set());
    }

    #[test]
    fn model_path_traversal_is_refused() {
        let d = tmp();
        for bad in ["../evil.glb", "sub/model.glb", "/etc/passwd.glb", ".hidden.glb", "..\\x.glb"] {
            let pet = pet_with_model(bad);
            let errs = validate(&pet, d.path()).unwrap_err();
            assert!(
                errs.iter().any(|e| e.field == "model.file" && e.reason.contains("bare file name")),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn model_extension_is_checked() {
        let d = tmp();
        std::fs::write(d.path().join("pet.exe"), b"MZ").unwrap();
        let errs = validate(&pet_with_model("pet.exe"), d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("not .vrm, .glb or .gltf")));
    }

    #[test]
    fn a_file_that_is_not_glb_is_refused_by_header_alone() {
        let d = tmp();
        std::fs::write(d.path().join("model.glb"), b"RIFF....WEBP").unwrap();
        let errs = validate(&pet_with_model("model.glb"), d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("missing glTF magic")));
    }

    #[test]
    fn a_wrong_glb_version_is_named_in_the_error() {
        let d = tmp();
        let mut b = glb_bytes();
        b[4..8].copy_from_slice(&3u32.to_le_bytes());
        std::fs::write(d.path().join("model.glb"), b).unwrap();
        let errs = validate(&pet_with_model("model.glb"), d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("version 3")));
    }

    #[test]
    fn model_scale_and_anchor_are_bounded() {
        let d = tmp();
        std::fs::write(d.path().join("model.glb"), glb_bytes()).unwrap();
        let mut pet = pet_with_model("model.glb");
        pet.model.scale = 99.0;
        pet.model.anchor = "sideways".into();
        pet.model.y_offset = 9.0;
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "model.scale"));
        assert!(errs.iter().any(|e| e.field == "model.anchor"));
        assert!(errs.iter().any(|e| e.field == "model.y_offset"));
    }

    #[test]
    fn a_json_gltf_is_accepted_by_header() {
        let d = tmp();
        std::fs::write(d.path().join("a.gltf"), br#"{"asset":{"version":"2.0"}}"#).unwrap();
        assert!(validate(&pet_with_model("a.gltf"), d.path()).is_ok());
    }

    /// A structurally valid animated WebP, so clip validation can be tested without art.
    fn animated_webp(w: u32, h: u32, frames: u32, ms: u32, loop_count: u16) -> Vec<u8> {
        let mut chunks = Vec::new();
        let mut vp8x = vec![0x02 | 0x10];
        vp8x.extend_from_slice(&[0, 0, 0]);
        vp8x.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        chunks.extend_from_slice(b"VP8X");
        chunks.extend_from_slice(&(vp8x.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&vp8x);
        let mut anim = vec![0, 0, 0, 0];
        anim.extend_from_slice(&loop_count.to_le_bytes());
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

    fn clip_pet() -> Pet {
        let mut pet = Pet { name: "Mochi".into(), ..Pet::default() };
        pet.clips.insert("idle".into(), "idle.webp".into());
        pet.clips.insert("walk".into(), "walk.webp".into());
        pet
    }

    fn write_clips(dir: &Path) {
        std::fs::write(dir.join("idle.webp"), animated_webp(264, 280, 20, 58, 0)).unwrap();
        std::fs::write(dir.join("walk.webp"), animated_webp(264, 280, 18, 58, 0)).unwrap();
    }

    #[test]
    fn a_clip_pack_pet_needs_no_sprite() {
        let d = tmp();
        write_clips(d.path());
        let pet = clip_pet();
        assert!(validate(&pet, d.path()).is_ok(), "clips alone are enough art");
    }

    #[test]
    fn a_clip_pack_needs_idle_but_not_walk() {
        let d = tmp();
        write_clips(d.path());

        // `walk` is optional on purpose: a pack is assembled over days and across accounts,
        // so requiring it on day one would make the intended workflow impossible.
        let mut without_walk = clip_pet();
        without_walk.clips.remove("walk");
        assert!(validate(&without_walk, d.path()).is_ok(), "a stationary pet is a valid pet");

        // `idle` is not optional: it is what plays when nothing else is happening.
        let mut without_idle = clip_pet();
        without_idle.clips.remove("idle");
        let errs = validate(&without_idle, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("needs `idle`")), "{errs:?}");
        assert!(
            errs.iter().any(|e| e.reason.contains("--set-clip idle")),
            "the error must say how to fix it: {errs:?}"
        );
    }

    #[test]
    fn an_unknown_state_is_named_and_the_valid_ones_are_listed() {
        let d = tmp();
        write_clips(d.path());
        let mut pet = clip_pet();
        pet.clips.insert("evil".into(), "idle.webp".into());
        let errs = validate(&pet, d.path()).unwrap_err();
        let e = errs.iter().find(|e| e.reason.contains("evil")).unwrap();
        assert!(e.reason.contains("idle"), "should list valid states: {}", e.reason);
    }

    #[test]
    fn clip_path_traversal_is_refused() {
        let d = tmp();
        write_clips(d.path());
        let mut pet = clip_pet();
        pet.clips.insert("water".into(), "../../../etc/passwd.webp".into());
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("bare file name")), "{errs:?}");
    }

    #[test]
    fn a_missing_clip_file_is_reported_by_state() {
        let d = tmp();
        write_clips(d.path());
        let mut pet = clip_pet();
        pet.clips.insert("water".into(), "water.webp".into());
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("water: water.webp is missing")), "{errs:?}");
    }

    #[test]
    fn a_still_image_cannot_stand_in_for_a_clip() {
        let d = tmp();
        write_clips(d.path());
        // A still WebP (VP8X without the animation flag).
        let mut still = animated_webp(264, 280, 0, 0, 0);
        still[20] = 0x10; // clear the animation flag in VP8X
        std::fs::write(d.path().join("water.webp"), &still).unwrap();
        let mut pet = clip_pet();
        pet.clips.insert("water".into(), "water.webp".into());
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("is a still image")), "{errs:?}");
    }

    #[test]
    fn a_walk_cycle_that_does_not_loop_is_refused() {
        let d = tmp();
        std::fs::write(d.path().join("idle.webp"), animated_webp(64, 64, 8, 50, 0)).unwrap();
        std::fs::write(d.path().join("walk.webp"), animated_webp(64, 64, 8, 50, 3)).unwrap();
        let errs = validate(&clip_pet(), d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("does not loop")), "{errs:?}");
    }

    #[test]
    fn a_clip_bomb_is_refused_by_header() {
        let d = tmp();
        std::fs::write(d.path().join("idle.webp"), animated_webp(64, 64, 8, 50, 0)).unwrap();
        // 12000x12000 canvas: ~576 MB decoded, from a few kilobytes of file.
        std::fs::write(d.path().join("walk.webp"), animated_webp(12000, 12000, 8, 50, 0)).unwrap();
        let errs = validate(&clip_pet(), d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("decompression bomb")), "{errs:?}");
    }

    #[test]
    fn clips_and_model_are_mutually_exclusive() {
        let d = tmp();
        write_clips(d.path());
        std::fs::write(d.path().join("model.glb"), glb_bytes()).unwrap();
        let mut pet = clip_pet();
        pet.model = Model {
            file: "model.glb".into(),
            scale: 1.0,
            anchor: "bottom".into(),
            y_offset: 0.0,
            clips: Clips::default(),
        };
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("not both")), "{errs:?}");
    }

    #[test]
    fn scaffold_produces_a_valid_pet() {
        let d = tmp();
        let pet = scaffold(d.path(), "Mochi", 32, "#c2331a", "#1f6b3a").unwrap();
        assert_eq!(pet.name, "Mochi");
        assert!(d.path().join("sprite.png").exists());
        let loaded = load(d.path()).unwrap();
        assert_eq!(loaded.frame_size, 32);
    }

    #[test]
    fn scaffolded_sprite_has_the_documented_grid() {
        let d = tmp();
        scaffold(d.path(), "Mochi", 32, "#c2331a", "#1f6b3a").unwrap();
        let (w, h) = png_size(&d.path().join("sprite.png")).unwrap();
        assert_eq!(w, 32 * 4, "4 columns");
        assert_eq!(h, 32 * 5, "5 rows");
    }

    #[test]
    fn generated_png_is_a_real_png() {
        let d = tmp();
        scaffold(d.path(), "p", 16, "#123456", "#abcdef").unwrap();
        let raw = std::fs::read(d.path().join("sprite.png")).unwrap();
        assert_eq!(&raw[..8], &PNG_MAGIC);
        assert!(raw.windows(4).any(|w| w == b"IHDR"));
        assert!(raw.windows(4).any(|w| w == b"IEND"));
        assert_eq!(png_size(&d.path().join("sprite.png")).unwrap().0, 64);
    }

    #[test]
    fn generated_sprite_is_not_blank() {
        let d = tmp();
        scaffold(d.path(), "p", 32, "#c2331a", "#1f6b3a").unwrap();
        let pet = load(d.path()).unwrap();
        let px_count = (pet.frame_size * 4 * pet.frame_size * 5) as usize;
        assert!(px_count > 0);
        // The blob must actually draw pixels; a transparent sheet would pass every other check.
        let s = generate(pet.frame_size, [0x8b, 0x5c, 0xf6], [0xf4, 0x72, 0xb6]);
        let opaque = s.pixels.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(opaque > 200, "only {opaque} opaque pixels — did the pet vanish?");
    }

    #[test]
    fn bidi_override_in_a_name_is_refused() {
        // U+202E makes "Mochi" display as something else entirely. This is a real trick.
        let d = tmp();
        let mut pet = Pet { name: "Mo\u{202E}chi".into(), ..Default::default() };
        pet.frame_size = 32;
        std::fs::create_dir_all(d.path()).unwrap();
        write_pet(d.path(), &pet);
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(
            errs.iter().any(|e| e.reason.contains("text-direction")),
            "bidi override must be caught: {errs:?}"
        );
    }

    #[test]
    fn newline_in_a_speech_line_is_refused() {
        let d = tmp();
        let mut pet = Pet::default();
        pet.speech.water = vec!["water\nbreak: click here".into()];
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_err());
    }

    #[test]
    fn oversized_speech_is_refused() {
        let d = tmp();
        let mut pet = Pet::default();
        pet.speech.water = vec!["x".repeat(MAX_SPEECH_LINE + 1)];
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_err());
    }

    #[test]
    fn bad_frame_size_is_refused() {
        let d = tmp();
        let pet = Pet { frame_size: 30, ..Default::default() };
        write_pet(d.path(), &pet);
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.field == "frame_size"), "{errs:?}");
    }

    #[test]
    fn giant_frame_size_is_refused() {
        let d = tmp();
        let pet = Pet { frame_size: 1024, ..Default::default() };
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_err());
    }

    #[test]
    fn insane_fps_is_refused() {
        let d = tmp();
        let mut pet = Pet::default();
        pet.fps.idle = 240;
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_err());
    }

    #[test]
    fn grid_mismatch_is_refused_with_a_useful_message() {
        let d = tmp();
        let pet = Pet { frame_size: 48, ..Default::default() };
        // Deliberately write a 32px sprite against a 48px manifest.
        std::fs::write(d.path().join("pet.json"), serde_json::to_string(&pet).unwrap()).unwrap();
        std::fs::write(
            d.path().join("sprite.png"),
            generate(32, [1, 2, 3], [4, 5, 6]).encode_png(),
        )
        .unwrap();
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("multiple of frame_size")), "{errs:?}");
    }

    #[test]
    fn non_png_sprite_is_refused() {
        let d = tmp();
        let pet = Pet::default();
        std::fs::write(d.path().join("pet.json"), serde_json::to_string(&pet).unwrap()).unwrap();
        std::fs::write(d.path().join("sprite.png"), b"<svg>not a png</svg>").unwrap();
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("not a PNG")), "{errs:?}");
    }

    #[test]
    fn oversized_sprite_is_refused_before_decoding() {
        let d = tmp();
        let pet = Pet::default();
        std::fs::write(d.path().join("pet.json"), serde_json::to_string(&pet).unwrap()).unwrap();
        let big = vec![0u8; (MAX_SPRITE_BYTES + 1) as usize];
        std::fs::write(d.path().join("sprite.png"), big).unwrap();
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs.iter().any(|e| e.reason.contains("limit is")), "{errs:?}");
    }

    #[test]
    fn unsupported_version_is_refused_politely() {
        let d = tmp();
        let pet = Pet { version: 7, ..Default::default() };
        write_pet(d.path(), &pet);
        let errs = validate(&pet, d.path()).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| e.field == "version" && e.reason.contains("this build reads 1")));
    }

    #[test]
    fn bad_palette_is_refused() {
        let d = tmp();
        let pet =
            Pet { palette: vec!["rebeccapurple".into(), "#12345".into()], ..Default::default() };
        write_pet(d.path(), &pet);
        assert!(validate(&pet, d.path()).is_err());
    }

    #[test]
    fn install_copies_every_file_the_manifest_names() {
        // Regression: install used to copy exactly `pet.json` + `sprite.png`, so a pet with
        // clips or a 3D model installed "successfully" and then failed to load its own art.
        let d = tmp();
        let src = d.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        write_clips(&src);
        std::fs::write(src.join("pet.json"), serde_json::to_string(&clip_pet()).unwrap()).unwrap();

        let pets = d.path().join("pets");
        let dest = install(&src, &pets).unwrap();
        for f in ["pet.json", "idle.webp", "walk.webp"] {
            assert!(dest.join(f).exists(), "{f} was not copied");
        }
        assert!(load(&dest).is_ok(), "the installed copy must validate");
    }

    #[test]
    fn install_copies_a_model_file() {
        let d = tmp();
        let src = d.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("model.glb"), glb_bytes()).unwrap();
        std::fs::write(
            src.join("pet.json"),
            serde_json::to_string(&pet_with_model("model.glb")).unwrap(),
        )
        .unwrap();
        let dest = install(&src, &d.path().join("pets")).unwrap();
        assert!(dest.join("model.glb").exists());
        assert!(load(&dest).is_ok());
    }

    #[test]
    fn install_copies_and_refuses_to_clobber() {
        let src = tmp();
        let pets = tmp();
        scaffold(src.path(), "Mochi", 32, "#c2331a", "#1f6b3a").unwrap();
        let dest = install(src.path(), pets.path()).unwrap();
        assert!(dest.join("pet.json").exists());
        assert!(dest.join("sprite.png").exists());
        assert!(install(src.path(), pets.path()).is_err(), "second install must refuse");
    }

    #[test]
    fn installed_skips_broken_pets_without_failing() {
        let pets = tmp();
        scaffold(&pets.path().join("good"), "Good", 32, "#c2331a", "#1f6b3a").unwrap();
        let broken = pets.path().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("pet.json"), "{ not json").unwrap();
        let (good, bad) = installed(pets.path());
        assert_eq!(good.len(), 1);
        assert_eq!(bad.len(), 1);
        assert_eq!(good[0].name, "Good");
    }

    #[test]
    fn slug_is_safe() {
        assert_eq!(slug("Mochi"), "mochi");
        assert_eq!(slug("My Pet!! 2"), "my-pet-2");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("///"), "pet");
    }

    #[test]
    fn default_pet_has_useful_speech_fallbacks() {
        let pet = Pet::default();
        assert!(pet.speech.for_kind("water").is_none(), "empty means use builtins");
    }
}
