//! Avatar support: 3D humans and animals for the desktop companion.
//!
//! Two halves, deliberately separated:
//!
//! - [`audit`] — **security**. Opens a `.vrm`/`.glb`/`.gltf` and reports what is inside it
//!   (external network references, embedded photos, identifying metadata, rig capability)
//!   without decoding a single pixel or vertex. Untrusted input is read by header, not trusted.
//! - [`builtin`] — **a starting point**. Procedurally generated, rigged, animated humanoid and
//!   animal avatars so a user can launch the companion immediately, before they've made or
//!   downloaded anything. No network, no asset licence problems, a few KB of parameters.
//!
//! Why not ship "realistic" models in the repo? A convincing human avatar is 5–80 MB of rigged
//! mesh, textures and clothing, usually under a licence that forbids redistribution, and it
//! freezes a design that artists should own. So Savepoint is an excellent *runtime* for
//! user-supplied VRM/glTF avatars — the format VRoid Studio and most 3D character stores already
//! export — and provides procedural stand-ins that look intentional rather than realistic.

pub mod audit;
pub mod builtin;

pub use audit::{audit, Audit, Format, Severity};

/// Where user avatars live.
pub fn avatar_dir(data: &std::path::Path) -> std::path::PathBuf {
    data.join("avatars")
}

/// Lists avatar files with a one-line audit for each. Broken files are reported, never dropped
/// silently — "your avatar didn't load" with no reason is the worst possible answer.
pub fn list(data: &std::path::Path) -> Vec<(std::path::PathBuf, Result<AuditSummary, String>)> {
    let dir = avatar_dir(data);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !matches!(ext.as_str(), "vrm" | "glb" | "gltf") {
            continue;
        }
        let summary = match audit::audit(&path) {
            Ok(a) => Ok(AuditSummary {
                name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                summary: a.summary(),
                humanoid: a.humanoid_bones > 0,
                findings: a.findings.len(),
                openable: a.is_openable(),
            }),
            Err(e) => Err(e.to_string()),
        };
        out.push((path, summary));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[derive(Debug, Clone)]
pub struct AuditSummary {
    pub name: String,
    pub summary: String,
    pub humanoid: bool,
    pub findings: usize,
    pub openable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_dir_is_under_the_data_dir() {
        let d = std::path::Path::new("/tmp/x");
        assert_eq!(avatar_dir(d), std::path::Path::new("/tmp/x/avatars"));
    }

    #[test]
    fn list_ignores_non_avatar_files() {
        let d = tempfile::tempdir().unwrap();
        let dir = avatar_dir(d.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "hi").unwrap();
        assert!(list(d.path()).is_empty());
    }
}
