//! Avatar file audit — the security half of "users can bring their own 3D model".
//!
//! An avatar is the riskiest thing a person will ever load into Tether. VRM/glTF files are
//! ZIPs full of JSON, images and binary buffers; they come from model shops, Discord servers and
//! strangers. Some of what they can carry is not a *code* risk at all — it is a privacy risk:
//!
//! - **Embedded textures can be photos of a real person.** A "cat avatar" can contain a
//!   photograph in a sidecar PNG, and nobody opening the file would know.
//! - **glTF `extras` / VRM `meta` blocks carry arbitrary author strings** — real names, email
//!   addresses, licence text, social handles — that can be silently re-published when the model
//!   is shared.
//! - **`asset.extras.title`/`author`** are set by whatever exported the file.
//!
//! So before Tether loads anything, `audit()` opens the container and *reports*:
//! external network references (an avatar that phones home is a tracker), embedded images and
//! their real dimensions, human-looking metadata, and any glTF extension that would need a
//! remote fetch to render. Nothing is blocked silently; the user is told what is inside and
//! chooses. That is the difference between a privacy tool and a nanny.
//!
//! The audit reads **only headers and JSON**. It never decodes pixels or buffers, so a malicious
//! 2 GB buffer cannot be used to exhaust memory during a security check.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

pub const MAX_AVATAR_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_GLTF_JSON_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Binary glTF (`.glb`), the usual interchange format.
    Glb,
    /// VRM 0.x / 1.0 — glTF plus humanoid bone mapping and blend-shape expressions.
    Vrm,
    /// Text glTF with external buffers.
    Gltf,
    /// A glTF file that failed to parse.
    Unknown,
}

impl Format {
    pub fn label(&self) -> &'static str {
        match self {
            Format::Glb => "glTF binary (.glb)",
            Format::Vrm => "VRM avatar",
            Format::Gltf => "glTF (.gltf)",
            Format::Unknown => "unrecognised",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    /// Worth telling the user about before they trust the file.
    Warn,
    /// Tether will not open this without an explicit override.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub what: String,
}

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// Simple heuristic: could this plausibly be a photograph of a real person?
    pub photo_like: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Audit {
    pub format: Option<Format>,
    pub bytes: u64,
    pub findings: Vec<Finding>,
    pub images: Vec<ImageInfo>,
    /// Strings that look like real-world identifiers found in metadata.
    pub pii: Vec<String>,
    /// Author/title strings declared by the exporter.
    pub declared_author: Option<String>,
    pub declared_title: Option<String>,
    /// glTF extensions present, e.g. `VRM`, `KHR_materials_unlit`.
    pub extensions: Vec<String>,
    pub humanoid_bones: usize,
    pub expressions: usize,
    pub mesh_count: usize,
    pub triangle_count: u64,
    pub animations: Vec<String>,
    /// True when the file references assets it does not contain (needs a network fetch).
    pub external_refs: Vec<String>,
    pub license: Option<String>,
}

impl Audit {
    pub fn worst(&self) -> Severity {
        if self.findings.iter().any(|f| f.severity == Severity::Block) {
            Severity::Block
        } else if self.findings.iter().any(|f| f.severity == Severity::Warn) {
            Severity::Warn
        } else {
            Severity::Info
        }
    }

    pub fn is_openable(&self) -> bool {
        self.worst() != Severity::Block
    }

    pub fn summary(&self) -> String {
        let tri = if self.triangle_count > 0 {
            format!("{} tris", human_count(self.triangle_count))
        } else {
            "no mesh data".to_string()
        };
        let rig = if self.humanoid_bones > 0 {
            format!("{} humanoid bones", self.humanoid_bones)
        } else if self.mesh_count > 0 {
            format!("{} meshes, no humanoid rig", self.mesh_count)
        } else {
            "unrigged".to_string()
        };
        format!("{} · {} · {}", self.format.map(|f| f.label()).unwrap_or("unknown"), tri, rig)
    }
}

fn human_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Audits `path` without decoding geometry or pixels.
pub fn audit(path: &Path) -> std::io::Result<Audit> {
    let meta = std::fs::metadata(path)?;
    let mut a = Audit { bytes: meta.len(), ..Default::default() };

    if meta.len() > MAX_AVATAR_BYTES {
        a.findings.push(Finding {
            severity: Severity::Block,
            what: format!(
                "{} MB is over the {} MB limit for an avatar",
                meta.len() / 1_048_576,
                MAX_AVATAR_BYTES / 1_048_576
            ),
        });
        return Ok(a);
    }

    let mut head = vec![0u8; 12];
    {
        let mut f = std::fs::File::open(path)?;
        f.read_exact(&mut head)?;
    }

    let is_glb = head[..4] == *b"glTF";
    let is_text_gltf = path.extension().map(|e| e.eq_ignore_ascii_case("gltf")).unwrap_or(false);

    if is_glb {
        let (json, bin_len, bin_offset) = read_glb_chunks(path)?;
        a.format = Some(if json.contains("\"VRM\"") || json.contains("\"VRMC_vrm\"") {
            Format::Vrm
        } else {
            Format::Glb
        });
        analyze_json(&mut a, &json, Some(path), bin_offset)?;
        if bin_len == 0 {
            a.findings.push(Finding {
                severity: Severity::Warn,
                what: "the GLB has a JSON chunk but no binary buffer (textures or geometry may be missing)".into(),
            });
        }
    } else if is_text_gltf {
        let mut s = String::new();
        std::fs::File::open(path)?.take(MAX_GLTF_JSON_BYTES as u64).read_to_string(&mut s)?;
        a.format = Some(if s.contains("\"VRM\"") || s.contains("\"VRMC_vrm\"") {
            Format::Vrm
        } else {
            Format::Gltf
        });
        analyze_json(&mut a, &s, Some(path), None)?;
    } else {
        a.format = Some(Format::Unknown);
        a.findings.push(Finding {
            severity: Severity::Block,
            what: "not a glTF/GLB/VRM file — Tether only loads 3D avatars in glTF format".into(),
        });
    }

    Ok(a)
}

/// Reads the JSON chunk and the binary chunk length from a GLB container.
/// Returns the JSON chunk, the binary chunk's length, and where the binary chunk's payload
/// starts in the file. The offset is what lets us read the first 32 bytes of an embedded PNG
/// without ever moving megabytes of texture data through memory.
fn read_glb_chunks(path: &Path) -> std::io::Result<(String, u64, Option<u64>)> {
    let mut f = std::fs::File::open(path)?;
    let mut header = [0u8; 12];
    f.read_exact(&mut header)?;
    // magic(4) version(4) length(4)
    let mut json = String::new();
    let mut bin_len = 0u64;
    let mut bin_offset = None;
    for _ in 0..8 {
        let mut chunk_head = [0u8; 8];
        if f.read_exact(&mut chunk_head).is_err() {
            break;
        }
        let len =
            u32::from_le_bytes([chunk_head[0], chunk_head[1], chunk_head[2], chunk_head[3]]) as u64;
        let kind = &chunk_head[4..8];
        if kind == b"JSON" {
            if len as usize > MAX_GLTF_JSON_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "JSON chunk is implausibly large",
                ));
            }
            let mut buf = vec![0u8; len as usize];
            f.read_exact(&mut buf)?;
            json = String::from_utf8_lossy(&buf).into_owned();
        } else if kind == b"BIN\0" {
            bin_len = len;
            use std::io::Seek;
            bin_offset = Some(f.stream_position()?);
            // Skip the payload; the audit reads image headers from it on demand.
            f.seek(std::io::SeekFrom::Current(len as i64))?;
        } else {
            use std::io::Seek;
            f.seek(std::io::SeekFrom::Current(len as i64))?;
        }
    }
    Ok((json, bin_len, bin_offset))
}

fn analyze_json(
    a: &mut Audit,
    json: &str,
    path: Option<&Path>,
    bin_offset: Option<u64>,
) -> Result<(), std::io::Error> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("avatar JSON is malformed: {e}"),
        )
    })?;

    // Counts and rig information.
    if let Some(ms) = v.get("meshes").and_then(|m| m.as_array()) {
        a.mesh_count = ms.len();
        for mesh in ms {
            for prim in mesh.get("primitives").and_then(|p| p.as_array()).into_iter().flatten() {
                if let Some(idx) = prim.get("indices").and_then(|i| i.as_u64()) {
                    if let Some(acc) = v
                        .get("accessors")
                        .and_then(|x| x.as_array())
                        .and_then(|arr| arr.get(idx as usize))
                    {
                        if let Some(count) = acc.get("count").and_then(|c| c.as_u64()) {
                            a.triangle_count += count / 3;
                        }
                    }
                }
            }
        }
    }
    if let Some(anims) = v.get("animations").and_then(|x| x.as_array()) {
        for anim in anims {
            if let Some(name) = anim.get("name").and_then(|n| n.as_str()) {
                a.animations.push(name.to_string());
            }
        }
    }
    if let Some(ext) = v.get("extensions").and_then(|e| e.as_object()) {
        for key in ext.keys() {
            a.extensions.push(key.clone());
        }
        // VRM 0.x
        if let Some(vrm) = ext.get("VRM").and_then(|x| x.get("humanoid")) {
            if let Some(bones) = vrm.get("humanBones").and_then(|b| b.as_array()) {
                a.humanoid_bones = bones.len();
            }
        }
        if let Some(vrm) = ext.get("VRM").and_then(|x| x.get("blendShapeMaster")) {
            if let Some(groups) = vrm.get("blendShapeGroups").and_then(|b| b.as_array()) {
                a.expressions = groups.len();
            }
        }
        // VRM 1.0
        if let Some(vrm) = ext.get("VRMC_vrm") {
            if let Some(h) =
                vrm.get("humanoid").and_then(|x| x.get("humanBones")).and_then(|b| b.as_object())
            {
                a.humanoid_bones = h.len();
            }
            if let Some(ex) = vrm.get("expressions").and_then(|x| x.as_object()) {
                a.expressions = ex.len();
            }
            if let Some(meta) = vrm.get("meta") {
                a.declared_author =
                    meta.get("authors").or_else(|| meta.get("author")).map(value_to_string);
                a.declared_title = meta.get("name").map(value_to_string);
                a.license = meta.get("licenseUrl").map(value_to_string);
            }
        }
        // A fingerprinting vector: extensions that need remote resources.
        for key in ["KHR_draco_mesh_compression", "EXT_meshopt_compression"] {
            if ext.contains_key(key) {
                a.findings.push(Finding {
                    severity: Severity::Info,
                    what: format!("{key}: compressed geometry (decoded locally, no network)"),
                });
            }
        }
    }

    // glTF asset metadata is free text supplied by the exporter.
    if let Some(asset) = v.get("asset") {
        if a.declared_author.is_none() {
            a.declared_author = asset.get("author").map(value_to_string);
        }
        if a.declared_title.is_none() {
            a.declared_title = asset.get("title").map(value_to_string);
        }
        if let Some(rest) = asset.get("extras") {
            collect_pii(a, rest);
        }
    }
    if let Some(extras) = v.get("extras") {
        collect_pii(a, extras);
    }

    // Every URI in `buffers`/`images`: anything not a data URI means the file needs the network.
    for key in ["buffers", "images"] {
        if let Some(arr) = v.get(key).and_then(|x| x.as_array()) {
            for entry in arr {
                if let Some(uri) = entry.get("uri").and_then(|u| u.as_str()) {
                    if !uri.starts_with("data:") {
                        a.external_refs.push(uri.to_string());
                    }
                }
            }
        }
    }
    if !a.external_refs.is_empty() {
        a.findings.push(Finding {
            severity: Severity::Block,
            what: format!(
                "{} external file reference(s): this avatar is not self-contained and would \
                 have to fetch {} to render. Tether never loads remote assets.",
                a.external_refs.len(),
                a.external_refs.first().cloned().unwrap_or_default()
            ),
        });
    }

    // Embedded images. Dimensions come from the file header — PNG IHDR or a JPEG SOF marker —
    // never from decoding. A 40000×40000 texture is a valid file that costs ~6 GB to expand, so
    // refusing it *before* a decoder sees it is the whole point of this pass.
    if let Some(arr) = v.get("images").and_then(|x| x.as_array()) {
        let views = v.get("bufferViews").and_then(|x| x.as_array());
        for entry in arr {
            let name = entry.get("name").and_then(|n| n.as_str()).unwrap_or("image").to_string();
            let mut info = ImageInfo { name, width: 0, height: 0, bytes: 0, photo_like: false };

            // Case 1: a data URI carries the bytes inline in the JSON.
            if let Some(uri) = entry.get("uri").and_then(|u| u.as_str()) {
                if let Some(b64) = uri.split("base64,").nth(1) {
                    if let Some(bytes) = base64_prefix(b64, 64) {
                        info.bytes = bytes.len() as u64;
                        if let Some((w, h, _)) = image_dimensions(&bytes) {
                            info.width = w;
                            info.height = h;
                        }
                    }
                }
            } else if let (Some(idx), Some(views), Some((base, src))) =
                (entry.get("bufferView").and_then(|i| i.as_u64()), views, bin_offset.zip(path))
            {
                // Case 2: bytes live in the GLB binary chunk, addressed by bufferView.
                if let Some(view) = views.get(idx as usize) {
                    let off = view.get("byteOffset").and_then(|o| o.as_u64()).unwrap_or(0);
                    let len = view.get("byteLength").and_then(|l| l.as_u64()).unwrap_or(0);
                    info.bytes = len;
                    if len > 0 {
                        if let Some(head) = read_range(src, base + off, 96) {
                            if let Some((w, h, _)) = image_dimensions(&head) {
                                info.width = w;
                                info.height = h;
                            }
                        }
                    }
                }
            }

            // A photograph is the difference between "a texture" and "a picture of a face".
            info.photo_like = info.width >= 1024 && info.height >= 1024;
            if info.width > 0 && info.height > 0 {
                let megapixels = u64::from(info.width) * u64::from(info.height);
                if megapixels > 64_000_000 {
                    a.findings.push(Finding {
                        severity: Severity::Warn,
                        what: format!(
                            "embedded image \"{}\" is {}×{} ({:.0} Mpx): decoding it would cost \
                             roughly {:.0} MB of RAM. Refuse if you did not expect it.",
                            info.name,
                            info.width,
                            info.height,
                            megapixels as f64 / 1e6,
                            megapixels as f64 * 4.0 / 1e6
                        ),
                    });
                }
            }
            a.images.push(info);
        }
    }

    Ok(())
}

/// Reads at most `n` bytes at `offset`, reusing one file handle. Used for image headers.
fn read_range(path: &Path, offset: u64, n: usize) -> Option<Vec<u8>> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf).ok()?;
    buf.truncate(got);
    Some(buf)
}

/// Decodes the first `n` bytes of a base64 data URI. Bounded on purpose: the audit must never
/// materialise a decoded megabyte to learn a four-byte width field.
fn base64_prefix(b64: &str, n: usize) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = b64.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(n);
    for chunk in bytes.chunks(4) {
        if out.len() >= n || chunk.len() < 2 {
            break;
        }
        let v: Vec<u8> = chunk.iter().filter_map(|c| val(*c)).collect();
        if v.len() < 2 {
            break;
        }
        out.push((v[0] << 2) | (v[1] >> 4));
        if v.len() > 2 {
            out.push((v[1] << 4) | (v[2] >> 2));
        }
        if v.len() > 3 {
            out.push((v[2] << 6) | v[3]);
        }
    }
    out.truncate(n);
    Some(out)
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(value_to_string).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

/// Looks for things that identify a real person in free-text metadata.
///
/// The point is not to block the file. It is to answer a question the user cannot answer by
/// looking at a 3D model: *whose data is in here, and will sharing this re-publish it?*
fn collect_pii(a: &mut Audit, value: &serde_json::Value) {
    let mut flat: BTreeMap<String, String> = BTreeMap::new();
    flatten(value, "", &mut flat);
    for (key, val) in flat {
        if val.len() < 4 || val.len() > 300 {
            continue;
        }
        let lower = val.to_lowercase();
        let looks_like_email = val.contains('@') && val.contains('.') && !val.contains(' ');
        let looks_like_url = lower.starts_with("http://") || lower.starts_with("https://");
        let key_hints_identity =
            ["author", "name", "artist", "creator", "contact", "email", "copyright"]
                .iter()
                .any(|k| key.to_lowercase().contains(k));
        if looks_like_email || (key_hints_identity && !looks_like_url) {
            let entry = format!("{key}: {val}");
            if !a.pii.contains(&entry) {
                a.pii.push(entry);
            }
        }
    }
    if !a.pii.is_empty() {
        a.findings.push(Finding {
            severity: Severity::Warn,
            what: format!(
                "metadata contains {} item(s) that may identify a real person or licence holder. \
                 Re-sharing this avatar re-publishes them.",
                a.pii.len()
            ),
        });
    }
}

fn flatten(v: &serde_json::Value, prefix: &str, out: &mut BTreeMap<String, String>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(val, &key, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                flatten(val, &format!("{prefix}[{i}]"), out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

/// Sniffs PNG/JPEG dimensions from a byte slice without decoding.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32, &'static str)> {
    if bytes.len() >= 24 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h, "png"));
    }
    if bytes.len() > 4 && bytes[..2] == [0xFF, 0xD8] {
        // Walk JPEG segments for a SOFn marker.
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h, "jpeg"));
            }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            i += 2 + len.max(2);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// Builds a minimal but structurally real GLB around a given JSON payload.
    fn write_glb(path: &Path, json: &str) -> std::io::Result<()> {
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        let json_bytes = json.as_bytes();
        let padded = (json_bytes.len() + 3) & !3;
        let total = 12 + 8 + padded as u32;
        out.extend_from_slice(&total.to_le_bytes());
        out.extend_from_slice(&(padded as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(json_bytes);
        out.resize(out.len() + (padded - json_bytes.len()), b' ');
        std::fs::write(path, out)
    }

    fn good_vrm_json() -> String {
        r#"{
          "asset": {"version": "2.0", "generator": "VRoid"},
          "extensions": {
            "VRMC_vrm": {
              "specVersion": "1.0",
              "meta": {"name": "Aoi", "authors": ["someone"], "licenseUrl": "https://vrm.dev/licenses/1.0/"},
              "humanoid": {"humanBones": {"hips": {}, "spine": {}, "head": {}, "leftHand": {}}},
              "expressions": {"happy": {}, "blink": {}}
            }
          },
          "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
          "accessors": [{"count": 300}, {"count": 900}],
          "images": [{"name": "face_tex", "mimeType": "image/png", "bufferView": 2}],
          "buffers": [{"byteLength": 1234}],
          "animations": [{"name": "idle"}, {"name": "wave"}]
        }"#
        .to_string()
    }

    #[test]
    fn detects_a_vrm_and_reads_its_rig() {
        let d = tmp();
        let p = d.path().join("aoi.vrm");
        write_glb(&p, &good_vrm_json()).unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.format, Some(Format::Vrm));
        assert_eq!(a.humanoid_bones, 4);
        assert_eq!(a.expressions, 2);
        assert_eq!(a.mesh_count, 1);
        assert_eq!(a.triangle_count, 300);
        assert_eq!(a.animations, vec!["idle".to_string(), "wave".to_string()]);
        assert!(a.is_openable());
        assert_eq!(a.declared_title.as_deref(), Some("Aoi"));
        assert!(a.summary().contains("VRM"));
        assert!(a.summary().contains("humanoid bones"));
    }

    #[test]
    fn blocks_external_references() {
        let d = tmp();
        let p = d.path().join("remote.glb");
        write_glb(
            &p,
            r#"{"asset":{"version":"2.0"},"images":[{"uri":"https://tracker.example/p.png"}],"buffers":[{"uri":"data:application/octet-stream;base64,AA"}]}"#,
        )
        .unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.worst(), Severity::Block, "an avatar that fetches a URL must be blocked");
        assert!(!a.is_openable());
        assert_eq!(a.external_refs.len(), 1);
        assert!(a.external_refs[0].contains("tracker.example"));
    }

    #[test]
    fn data_uris_are_not_external() {
        let d = tmp();
        let p = d.path().join("inline.glb");
        write_glb(
            &p,
            r#"{"asset":{"version":"2.0"},"images":[{"uri":"data:image/png;base64,iVBORw0KGgo="}],"buffers":[{"uri":"data:application/octet-stream;base64,AA"}]}"#,
        )
        .unwrap();
        let a = audit(&p).unwrap();
        assert!(a.external_refs.is_empty());
        assert!(a.is_openable());
    }

    #[test]
    fn flags_identifying_metadata() {
        let d = tmp();
        let p = d.path().join("pii.glb");
        write_glb(
            &p,
            r#"{"asset":{"version":"2.0","extras":{"author":"Jane Q. Real","contact":"jane@example.com","note":"made with a tutorial"}}}"#,
        )
        .unwrap();
        let a = audit(&p).unwrap();
        assert!(a.worst() != Severity::Info || !a.pii.is_empty());
        assert!(a.pii.iter().any(|s| s.contains("jane@example.com")), "{:?}", a.pii);
        assert!(a
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warn && f.what.contains("identify a real person")));
    }

    #[test]
    fn oversized_avatar_is_blocked_before_reading() {
        let d = tmp();
        let p = d.path().join("huge.glb");
        // Sparse file larger than the cap.
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(MAX_AVATAR_BYTES + 1).unwrap();
        drop(f);
        let a = audit(&p).unwrap();
        assert_eq!(a.worst(), Severity::Block);
        assert!(a.findings.iter().any(|f| f.what.contains("over the")));
    }

    #[test]
    fn non_gltf_is_blocked_with_a_clear_reason() {
        let d = tmp();
        let p = d.path().join("cat.png");
        std::fs::write(&p, b"\x89PNG\r\n\x1a\n....").unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.format, Some(Format::Unknown));
        assert!(a.findings.iter().any(|f| f.what.contains("not a glTF")));
        assert!(!a.is_openable());
    }

    #[test]
    fn malformed_json_does_not_panic() {
        let d = tmp();
        let p = d.path().join("bad.glb");
        write_glb(&p, "{ this is not json").unwrap();
        // audit() surfaces the parse failure as an Err rather than panicking.
        assert!(audit(&p).is_err());
    }

    #[test]
    fn gltf_text_file_is_supported() {
        let d = tmp();
        let p = d.path().join("model.gltf");
        std::fs::write(&p, r#"{"asset":{"version":"2.0"},"meshes":[]}"#).unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.format, Some(Format::Gltf));
        assert!(a.is_openable());
    }

    #[test]
    fn detects_vrm_zero_point_x() {
        let d = tmp();
        let p = d.path().join("old.vrm");
        write_glb(
            &p,
            r#"{"asset":{"version":"2.0"},"extensions":{"VRM":{"humanoid":{"humanBones":[{"bone":"hips"},{"bone":"head"}]},"blendShapeMaster":{"blendShapeGroups":[{"name":"Joy"}]}}}}"#,
        )
        .unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.format, Some(Format::Vrm));
        assert_eq!(a.humanoid_bones, 2);
        assert_eq!(a.expressions, 1);
    }

    /// A GLB whose image lives in the binary chunk, as every real exporter writes it.
    fn glb_with_embedded_png(w: u32, h: u32) -> Vec<u8> {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&w.to_be_bytes());
        png.extend_from_slice(&h.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"images":[{{"name":"face","bufferView":0}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{}}}]}}"#,
            png.len()
        );
        let mut jb = json.into_bytes();
        while jb.len() % 4 != 0 {
            jb.push(b' ');
        }
        let mut bb = png;
        while bb.len() % 4 != 0 {
            bb.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        let total = 12 + 8 + jb.len() + 8 + bb.len();
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(jb.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&jb);
        out.extend_from_slice(&(bb.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bb);
        out
    }

    #[test]
    fn embedded_image_dimensions_are_read_from_the_binary_chunk() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.glb");
        std::fs::write(&p, glb_with_embedded_png(512, 768)).unwrap();
        let a = audit(&p).unwrap();
        assert_eq!(a.images.len(), 1);
        assert_eq!((a.images[0].width, a.images[0].height), (512, 768), "real IHDR dimensions");
        assert_eq!(a.images[0].name, "face");
    }

    #[test]
    fn a_texture_bomb_is_warned_about_before_decoding() {
        // 12000x12000 = 144 Mpx. Decoding would cost ~576 MB. The audit never decodes it, it
        // reads 24 bytes of header and says so.
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bomb.glb");
        std::fs::write(&p, glb_with_embedded_png(12000, 12000)).unwrap();
        let a = audit(&p).unwrap();
        assert_eq!((a.images[0].width, a.images[0].height), (12000, 12000));
        assert!(
            a.findings.iter().any(|f| f.what.contains("Mpx")),
            "must name the cost: {:?}",
            a.findings
        );
    }

    #[test]
    fn base64_prefix_decodes_enough_for_a_header() {
        let raw = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        let b64 = "iVBORw0KGgoAAAAN";
        let got = base64_prefix(b64, raw.len()).unwrap();
        assert_eq!(got, raw);
    }

    #[test]
    fn a_real_png_dimension_survives_a_data_uri() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("u.glb");
        // 1x1 transparent PNG, base64, as an `images[].uri` data URI.
        let uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let json = format!(r#"{{"asset":{{"version":"2.0"}},"images":[{{"uri":"{uri}"}}]}}"#);
        let mut jb = json.into_bytes();
        while jb.len() % 4 != 0 {
            jb.push(b' ');
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((12 + 8 + jb.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(jb.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&jb);
        std::fs::write(&p, out).unwrap();
        let a = audit(&p).unwrap();
        assert_eq!((a.images[0].width, a.images[0].height), (1, 1), "data URI dimensions");
    }

    #[test]
    fn sniff_png_dimensions() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&256u32.to_be_bytes());
        png.extend_from_slice(&512u32.to_be_bytes());
        assert_eq!(image_dimensions(&png), Some((256, 512, "png")));
    }

    #[test]
    fn sniff_jpeg_dimensions() {
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // APP0, len 16
        jpeg.extend_from_slice(&[0u8; 14]);
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]); // SOF0
        jpeg.extend_from_slice(&480u16.to_be_bytes());
        jpeg.extend_from_slice(&640u16.to_be_bytes());
        jpeg.extend_from_slice(&[0u8; 8]);
        assert_eq!(image_dimensions(&jpeg), Some((640, 480, "jpeg")));
    }

    #[test]
    fn compressing_extensions_are_reported_as_local() {
        let d = tmp();
        let p = d.path().join("draco.glb");
        write_glb(
            &p,
            r#"{"asset":{"version":"2.0"},"extensions":{"KHR_draco_mesh_compression":{}}}"#,
        )
        .unwrap();
        let a = audit(&p).unwrap();
        assert!(a.is_openable(), "draco decodes locally, so it is not a network risk");
        assert!(a.findings.iter().any(|f| f.what.contains("decoded locally")));
    }
}
