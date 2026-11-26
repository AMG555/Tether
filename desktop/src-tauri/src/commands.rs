use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub ready: bool,
    pub draft_count: usize,
    pub retention_days: i64,
    pub vault_path: String,
    pub encryption: &'static str,
    pub offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftItem {
    pub id: i64,
    pub app: String,
    pub preview: String,
    pub chars: i64,
    pub revisions: i64,
    pub updated_at: i64,
    pub age_human: String,
    pub days_left: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftDetail {
    pub id: i64,
    pub app: String,
    pub text: String,
    pub chars: i64,
    pub revisions: i64,
    pub updated_at: i64,
    pub age_human: String,
    pub days_left: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_custom: bool,
    pub is_installed: bool,
    pub avatar_type: String,
    pub actions: Vec<String>,
    pub colors: Option<CompanionColors>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionColors {
    pub body: String,
    pub accent: String,
    pub eyes: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCompanionPayload {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub avatar_type: String,
    pub actions: Vec<String>,
    pub colors: Option<CompanionColors>,
    pub model_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub autostart_removed: bool,
    pub vault_purged: bool,
    pub vault_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatusInfo {
    pub platform: String,
    pub uptime_secs: u64,
    pub autostart_installed: bool,
    pub retention_days: i64,
    pub vault_exists: bool,
    pub vault_path: String,
}

fn to_base64(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).map(|&b| b as u32).unwrap_or(0);
        let b2 = chunk.get(2).map(|&b| b as u32).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn open_local_vault() -> Result<tether::vault::Vault, String> {
    let dir = tether::default_data_dir();
    let key_file = dir.join("key.txt");
    let key = if key_file.exists() {
        let hex =
            std::fs::read_to_string(&key_file).map_err(|e| format!("cannot read key: {e}"))?;
        tether::crypto::SecretKey::from_hex(hex.trim())
            .ok_or_else(|| "corrupt key file".to_string())?
    } else {
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create data dir: {e}"))?;
        let key = tether::crypto::SecretKey::random()
            .map_err(|_| "cannot generate secret key".to_string())?;
        std::fs::write(&key_file, key.to_hex()).map_err(|e| format!("cannot write key: {e}"))?;
        let _ = tether::vault::set_private(&key_file);
        key
    };
    tether::vault::Vault::open(&dir, &key).map_err(|e| format!("cannot open vault: {e}"))
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo { name: "Tether", version: env!("CARGO_PKG_VERSION"), platform: std::env::consts::OS }
}

#[tauri::command]
pub fn get_vault_status() -> Result<VaultStatus, String> {
    let dir = tether::default_data_dir();
    let (cfg, _) = tether::config::Config::load_or_default(&dir);
    let (ready, draft_count) = match open_local_vault() {
        Ok(v) => {
            let count = v.list(usize::MAX, 0).map(|r| r.len()).unwrap_or(0);
            (true, count)
        }
        Err(_) => (false, 0),
    };

    Ok(VaultStatus {
        ready,
        draft_count,
        retention_days: cfg.retention_days,
        vault_path: dir.display().to_string(),
        encryption: "AES-256-GCM + HKDF",
        offline: true,
    })
}

#[tauri::command]
pub fn list_drafts(query: Option<String>, limit: Option<usize>) -> Result<Vec<DraftItem>, String> {
    let vault = match open_local_vault() {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };
    let dir = tether::default_data_dir();
    let (cfg, _) = tether::config::Config::load_or_default(&dir);
    let retention_days = cfg.retention_days;
    let lim = limit.unwrap_or(100);
    let rows = if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
        vault.search(&q, lim).map_err(|e| e.to_string())?
    } else {
        vault.list(lim, 160).map_err(|e| e.to_string())?
    };

    let now_ts = tether::vault::now();
    let items = rows
        .into_iter()
        .map(|r| {
            let age_human = r.age_human();
            let age_secs = (now_ts - r.updated_at).max(0);
            let age_days = age_secs / 86_400;
            let days_left = (retention_days - age_days).max(0);
            DraftItem {
                id: r.id,
                app: r.app.unwrap_or_else(|| "Unknown App".to_string()),
                preview: r.preview.unwrap_or_default(),
                chars: r.chars,
                revisions: r.revisions,
                updated_at: r.updated_at,
                age_human,
                days_left,
            }
        })
        .collect();
    Ok(items)
}

#[tauri::command]
pub fn get_draft(id: i64) -> Result<DraftDetail, String> {
    let vault = open_local_vault()?;
    let dir = tether::default_data_dir();
    let (cfg, _) = tether::config::Config::load_or_default(&dir);
    let (row, text) = vault.get(id).map_err(|e| e.to_string())?;
    let now_ts = tether::vault::now();
    let age_human = row.age_human();
    let age_secs = (now_ts - row.updated_at).max(0);
    let days_left = (cfg.retention_days - age_secs / 86_400).max(0);
    Ok(DraftDetail {
        id: row.id,
        app: row.app.unwrap_or_else(|| "Unknown App".to_string()),
        text,
        chars: row.chars,
        revisions: row.revisions,
        updated_at: row.updated_at,
        age_human,
        days_left,
    })
}

#[tauri::command]
pub fn get_revisions(id: i64) -> Result<Vec<String>, String> {
    let vault = open_local_vault()?;
    vault.revisions(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_draft(id: i64) -> Result<(), String> {
    let vault = open_local_vault()?;
    vault.delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prune_vault(max_days: Option<i64>) -> Result<usize, String> {
    let dir = tether::default_data_dir();
    let (cfg, _) = tether::config::Config::load_or_default(&dir);
    let days = max_days.unwrap_or(cfg.retention_days);
    let vault = open_local_vault()?;
    vault.prune_older_than(days).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_retention_days(days: i64) -> Result<i64, String> {
    let dir = tether::default_data_dir();
    let (mut cfg, _) = tether::config::Config::load_or_default(&dir);
    let clamped = days.clamp(1, 3650); // allow up to ~10 years for custom periods
    cfg.retention_days = clamped;
    cfg.save(&dir).map_err(|e| e.to_string())?;
    if let Ok(vault) = open_local_vault() {
        let _ = vault.prune_older_than(clamped);
    }
    Ok(clamped)
}

#[tauri::command]
pub fn get_autostart_status() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA").ok_or("cannot find %APPDATA%")?;
        let startup = PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs/Startup");
        let active = startup.join("tether.cmd").exists()
            || startup.join("tether.url").exists()
            || startup.join("tether.lnk").exists();
        Ok(active)
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("cannot find $HOME")?;
        let p = PathBuf::from(home).join("Library/LaunchAgents/dev.tether.plist");
        Ok(p.exists())
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").ok_or("cannot find $HOME")?;
        let p = PathBuf::from(home).join(".config/autostart/tether.desktop");
        Ok(p.exists())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA").ok_or("cannot find %APPDATA%")?;
        let p =
            PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs/Startup/tether.url");
        if enabled {
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            tether::setup::install_autostart(&current_exe)?;
            Ok(true)
        } else {
            if p.exists() {
                let _ = std::fs::remove_file(&p);
            }
            Ok(false)
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("cannot find $HOME")?;
        let p = PathBuf::from(home).join("Library/LaunchAgents/dev.tether.plist");
        if enabled {
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            tether::setup::install_autostart(&current_exe)?;
            Ok(true)
        } else {
            if p.exists() {
                let _ = std::fs::remove_file(&p);
            }
            Ok(false)
        }
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").ok_or("cannot find $HOME")?;
        let p = PathBuf::from(home).join(".config/autostart/tether.desktop");
        if enabled {
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            tether::setup::install_autostart(&current_exe)?;
            Ok(true)
        } else {
            if p.exists() {
                let _ = std::fs::remove_file(&p);
            }
            Ok(false)
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = enabled;
        Err("autostart not supported on this platform".into())
    }
}

#[tauri::command]
pub fn list_companions() -> Result<Vec<CompanionItem>, String> {
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut list: Vec<CompanionItem> = Vec::new();

    // 1. Read bundled companion packs from assets/companions/ (relative to the exe).
    //    Every companion that ships with the app has a manifest.json there with
    //    avatarType:"clips" — use those as the authoritative installed companions.
    let bundled_dir = bundled_companions_dir();
    if let Ok(entries) = std::fs::read_dir(&bundled_dir) {
        let mut bundled: Vec<CompanionItem> = entries
            .flatten()
            .filter_map(|e| {
                let manifest = e.path().join("manifest.json");
                let content = std::fs::read_to_string(manifest).ok()?;
                let mut item = serde_json::from_str::<CompanionItem>(&content).ok()?;
                item.is_installed = true; // bundled == always installed
                Some(item)
            })
            .collect();
        // Sort by name so the grid order is deterministic.
        bundled.sort_by(|a, b| a.name.cmp(&b.name));
        for item in bundled {
            seen_ids.insert(item.id.clone());
            list.push(item);
        }
    }

    // 2. Fall back to procedural stubs for any companion whose manifest.json is
    //    missing from the bundled dir (e.g. a dev environment where assets/ wasn't
    //    copied next to the binary).  All 32 companions ship bundled; these stubs
    //    are a last-resort safety net so the grid is never empty.
    #[allow(clippy::type_complexity)]
    let fallbacks: &[(&str, &str, &str, &[&str], &[&str], &str, &str)] = &[
        (
            "grace",
            "Grace",
            "A warm, grounding presence — she will remind you to drink water and rest.",
            &["human", "adult", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#2563eb",
            "#c2331a",
        ),
        (
            "rory",
            "Rory",
            "The lion cub heir — small, boisterous, and convinced he runs the place.",
            &["animal", "cub", "playful"],
            &["idle", "walk", "happy", "wave", "water", "stretch", "sad", "eyes"],
            "#d97706",
            "#b45309",
        ),
        (
            "axel",
            "Axel",
            "A skater boy who grinds deadlines the same way he grinds rails.",
            &["human", "kid", "sporty"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#7c3aed",
            "#6d28d9",
        ),
        (
            "bao",
            "Bao",
            "A steamed bun with a fiery heart and unwavering patience.",
            &["food", "cute", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#dc2626",
            "#b91c1c",
        ),
        (
            "blaze",
            "Blaze",
            "A pint-sized superhero who takes your water reminders very seriously.",
            &["human", "kid", "hero"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#ea580c",
            "#c2410c",
        ),
        (
            "bolt",
            "Bolt",
            "An energetic robot who charges up on your typing speed.",
            &["robot", "tech", "energetic"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0284c7",
            "#0369a1",
        ),
        (
            "captain-dew",
            "Captain Dew",
            "A water superhero whose only power is showing up every time you forget to hydrate.",
            &["human", "hero", "water"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0891b2",
            "#0e7490",
        ),
        (
            "chip",
            "Chip",
            "A mischievous chipmunk who stashes keystrokes like acorns.",
            &["animal", "cute", "playful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#92400e",
            "#78350f",
        ),
        (
            "coco",
            "Coco",
            "A cheerful golden retriever who wags whenever you finish a paragraph.",
            &["animal", "dog", "warm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#b45309",
            "#92400e",
        ),
        (
            "daisy",
            "Daisy",
            "A gentle bunny who nibbles carrots while you draft your notes.",
            &["animal", "bunny", "gentle"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#db2777",
            "#be185d",
        ),
        (
            "doc",
            "Doc",
            "A wise owl physician reminding you of eye ergonomics and breaks.",
            &["animal", "wise", "health"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#4f46e5",
            "#4338ca",
        ),
        (
            "ember",
            "Ember",
            "A tiny flame sprite who sparkles whenever your typing flow heats up.",
            &["elemental", "magic", "energetic"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#ef4444",
            "#dc2626",
        ),
        (
            "finn",
            "Finn",
            "A football kid who takes every break to practice his step-overs.",
            &["human", "kid", "sporty"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#059669",
            "#047857",
        ),
        (
            "iris",
            "Iris",
            "An artist fairy who paints colorful sparkles along your margins.",
            &["fantasy", "creative", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#7c3aed",
            "#6d28d9",
        ),
        (
            "luna",
            "Luna",
            "A moon fairy who keeps your circadian rhythm honest when you work late.",
            &["fantasy", "fairy", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#1d4ed8",
            "#1e40af",
        ),
        (
            "marmalade",
            "Marmalade",
            "A drowsy ginger cat purring quietly on top of your words.",
            &["animal", "cat", "cozy"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#c2410c",
            "#9a3412",
        ),
        (
            "nimbus",
            "Nimbus",
            "A miniature thundercloud raining soft inspiration onto your draft.",
            &["weather", "whimsical", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#4b5563",
            "#374151",
        ),
        (
            "nova",
            "Nova",
            "A cosmic astronaut floating gently beside your editor window.",
            &["space", "tech", "curious"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#6d28d9",
            "#5b21b6",
        ),
        (
            "ollie",
            "Ollie",
            "The class bookworm — always mid-chapter, always has a fun fact ready.",
            &["human", "kid", "studious"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0369a1",
            "#075985",
        ),
        (
            "peanut",
            "Peanut",
            "A hamster running on a tiny wheel whenever you type.",
            &["animal", "rodent", "cute"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#92400e",
            "#78350f",
        ),
        (
            "penny",
            "Penny",
            "A retro mechanical penguin who waddles in rhythm with your typing.",
            &["animal", "robot", "playful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0e7490",
            "#155e75",
        ),
        (
            "pixel",
            "Pixel",
            "A retro 8-bit companion bouncing along your text lines.",
            &["retro", "pixel", "playful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#7c3aed",
            "#6d28d9",
        ),
        (
            "quest",
            "Quest",
            "A gamer kid who treats every break like an achievement unlocked.",
            &["human", "kid", "tech"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#1d4ed8",
            "#1e40af",
        ),
        (
            "sage",
            "Sage",
            "A yoga instructor who demonstrates desk stretches with enthusiasm.",
            &["human", "sporty", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#065f46",
            "#064e3b",
        ),
        (
            "shadow",
            "Shadow",
            "A stealth panther who watches over your cursor silently.",
            &["animal", "cat", "sleek"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#1f2937",
            "#111827",
        ),
        (
            "spike",
            "Spike",
            "A jackfruit with opinions — slow to ripen, but worth the wait.",
            &["food", "whimsical", "calm"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#65a30d",
            "#4d7c0f",
        ),
        (
            "splash",
            "Splash",
            "A playful dolphin leaping through sentences when you make progress.",
            &["animal", "aquatic", "happy"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0891b2",
            "#0e7490",
        ),
        (
            "theo",
            "Theo",
            "A thoughtful philosopher bear contemplating your arguments.",
            &["animal", "bear", "thoughtful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#78350f",
            "#6b2d0f",
        ),
        (
            "twirl",
            "Twirl",
            "An acrobatic squirrel spinning through ideas and revisions.",
            &["animal", "agile", "cheerful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#b45309",
            "#92400e",
        ),
        (
            "waddle",
            "Waddle",
            "A chubby duckling splashing joyfully after every saved revision.",
            &["animal", "duck", "cute"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#0369a1",
            "#075985",
        ),
        (
            "zen",
            "Zen",
            "A meditative rock spirit maintaining tranquil focus in your workspace.",
            &["nature", "peaceful", "mindful"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#064e3b",
            "#065f46",
        ),
        (
            "zippy",
            "Zippy",
            "A lightning-fast hummingbird darting around fast typing spurts.",
            &["animal", "bird", "fast"],
            &["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"],
            "#b45309",
            "#92400e",
        ),
    ];
    for (id, name, desc, tags, actions, body, accent) in fallbacks {
        if !seen_ids.contains(*id) {
            list.push(CompanionItem {
                id: (*id).into(),
                name: (*name).into(),
                description: (*desc).into(),
                tags: tags.iter().map(|s| (*s).into()).collect(),
                is_custom: false,
                is_installed: true,
                avatar_type: "clips".into(),
                actions: actions.iter().map(|s| (*s).into()).collect(),
                colors: Some(CompanionColors {
                    body: (*body).into(),
                    accent: (*accent).into(),
                    eyes: "#14120f".into(),
                }),
            });
        }
    }

    // 3. User-installed custom companions from the data directory.
    let custom_dir = tether::default_data_dir().join("companions");
    if let Ok(entries) = std::fs::read_dir(custom_dir) {
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(manifest_path) {
                    if let Ok(item) = serde_json::from_str::<CompanionItem>(&content) {
                        if !seen_ids.contains(&item.id) {
                            seen_ids.insert(item.id.clone());
                            list.push(item);
                        }
                    }
                }
            }
        }
    }

    Ok(list)
}

/// Returns the directory that contains bundled companion packs.
///
/// Resolution order (first that exists wins):
///   1. Sibling `assets/companions/` next to the running executable — covers
///      production Tauri bundles where the NSIS installer places the app files.
///   2. Workspace-relative `assets/companions/` — covers `cargo run` / dev builds
///      where the exe lives deep inside `target/`.
fn bundled_companions_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));

        // 1. Sibling of the exe — covers the real app binary in target/debug/ or
        //    the installed NSIS bundle where the exe sits next to assets/.
        let p = exe_dir.join("assets/companions");
        if p.is_dir() {
            return p;
        }

        // 2. One level up — covers test binaries in target/debug/deps/ where
        //    assets/ lives at target/debug/assets/ (one directory above deps/).
        let p = exe_dir.join("../assets/companions");
        if p.is_dir() {
            return p;
        }

        // 3. Three/four levels up — covers dev binary in desktop/src-tauri/target/debug/
        //    relative to the workspace root assets/companions/
        let p = exe_dir.join("../../../assets/companions");
        if p.is_dir() {
            return p;
        }
        let p = exe_dir.join("../../../../assets/companions");
        if p.is_dir() {
            return p;
        }
    }
    // 4. Working-directory-relative paths — covers `cargo run` from workspace root or `npm run tauri:dev` from desktop/
    let p = PathBuf::from("assets/companions");
    if p.is_dir() {
        return p;
    }
    let p = PathBuf::from("../assets/companions");
    if p.is_dir() {
        return p;
    }
    PathBuf::from("assets/companions")
}

#[tauri::command]
pub fn save_custom_companion(payload: CustomCompanionPayload) -> Result<CompanionItem, String> {
    let dir = tether::default_data_dir().join("companions");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let slug = payload.id.clone().unwrap_or_else(|| {
        payload
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    });
    if slug.is_empty() {
        return Err("invalid companion name".into());
    }

    let item = CompanionItem {
        id: slug.clone(),
        name: payload.name,
        description: payload.description,
        tags: payload.tags,
        is_custom: true,
        is_installed: true,
        avatar_type: payload.avatar_type,
        actions: payload.actions,
        colors: payload.colors,
    };

    let comp_dir = dir.join(&slug);
    std::fs::create_dir_all(&comp_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?;
    std::fs::write(comp_dir.join("manifest.json"), json).map_err(|e| e.to_string())?;

    if let Some(data) = payload.model_data {
        let _ = std::fs::write(comp_dir.join("model.json"), data);
    }

    Ok(item)
}

#[tauri::command]
pub fn delete_custom_companion(id: String) -> Result<(), String> {
    let dir = tether::default_data_dir().join("companions").join(&id);
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn install_catalog_companion(id: String) -> Result<CompanionItem, String> {
    let clean_id = id.trim().to_lowercase();
    if clean_id.is_empty()
        || clean_id.contains('/')
        || clean_id.contains('\\')
        || clean_id.contains("..")
    {
        return Err("invalid companion identifier".into());
    }

    let dest = tether::default_data_dir().join("companions").join(&clean_id);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    // 1. If local repository assets exist, copy directly (fast local dev / offline cache)
    let local_asset_dir = std::path::PathBuf::from("assets/companions").join(&clean_id);
    let mut installed_from_local = false;
    if local_asset_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&local_asset_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(file_name) = p.file_name() {
                        let _ = std::fs::copy(&p, dest.join(file_name));
                    }
                }
            }
            installed_from_local = true;
        }
    }

    // 2. If not local, download from GitHub Releases asset storage and extract.
    //    Before writing anything to the data directory, fetch checksums.txt and verify
    //    the SHA-256 of the downloaded zip.  Any mismatch is a hard error; the temp file
    //    is deleted and nothing is written to the data directory.
    if !installed_from_local {
        let zip_url = format!(
            "https://github.com/AMG555/Tether/releases/latest/download/companion-{clean_id}.zip"
        );
        let checksums_url =
            "https://github.com/AMG555/Tether/releases/latest/download/checksums.txt";
        let temp_zip = std::env::temp_dir().join(format!("companion-{clean_id}.zip"));
        let temp_checksums = std::env::temp_dir().join("tether-companion-checksums.txt");

        // Helper: clean up temp files on any early return.
        let cleanup = || {
            let _ = std::fs::remove_file(&temp_zip);
            let _ = std::fs::remove_file(&temp_checksums);
        };

        // Download via curl or powershell
        #[cfg(target_os = "windows")]
        {
            // Download the zip.
            let status = std::process::Command::new("curl.exe")
                .args(["-fSL", &zip_url, "-o", temp_zip.to_str().unwrap_or_default()])
                .status();
            let downloaded = match status {
                Ok(s) if s.success() => true,
                _ => {
                    let ps_script = format!(
                        "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                        zip_url,
                        temp_zip.display()
                    );
                    std::process::Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", &ps_script])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                }
            };
            if !downloaded {
                cleanup();
                return Err(format!("failed to download {clean_id} from GitHub Releases"));
            }

            // Download checksums.txt.
            let cs_ok = std::process::Command::new("curl.exe")
                .args(["-fSL", checksums_url, "-o", temp_checksums.to_str().unwrap_or_default()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !cs_ok {
                cleanup();
                return Err("failed to download checksums.txt for verification".into());
            }

            // Verify SHA-256 before extracting.
            let zip_bytes =
                std::fs::read(&temp_zip).map_err(|e| format!("cannot read zip: {e}"))?;
            let checksums_text = std::fs::read_to_string(&temp_checksums)
                .map_err(|e| format!("cannot read checksums: {e}"))?;
            let expected_filename = format!("companion-{clean_id}.zip");
            if let Err(e) = verify_checksum(&zip_bytes, &checksums_text, &expected_filename) {
                cleanup();
                return Err(e);
            }
            let _ = std::fs::remove_file(&temp_checksums);

            let extract_status = std::process::Command::new("tar.exe")
                .args([
                    "-xf",
                    temp_zip.to_str().unwrap_or_default(),
                    "-C",
                    dest.to_str().unwrap_or_default(),
                ])
                .status();
            let extracted = match extract_status {
                Ok(s) if s.success() => true,
                _ => {
                    let ps_extract = format!(
                        "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                        temp_zip.display(),
                        dest.display()
                    );
                    std::process::Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", &ps_extract])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                }
            };
            let _ = std::fs::remove_file(&temp_zip);
            if !extracted {
                return Err(format!("failed to extract companion {clean_id}"));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Download the zip.
            let status = std::process::Command::new("curl")
                .args(["-fSL", &zip_url, "-o", temp_zip.to_str().unwrap_or_default()])
                .status();
            if !status.map(|s| s.success()).unwrap_or(false) {
                cleanup();
                return Err(format!("failed to download {clean_id} from GitHub Releases"));
            }

            // Download checksums.txt.
            let cs_ok = std::process::Command::new("curl")
                .args(["-fSL", checksums_url, "-o", temp_checksums.to_str().unwrap_or_default()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !cs_ok {
                cleanup();
                return Err("failed to download checksums.txt for verification".into());
            }

            // Verify SHA-256 before extracting.
            let zip_bytes =
                std::fs::read(&temp_zip).map_err(|e| format!("cannot read zip: {e}"))?;
            let checksums_text = std::fs::read_to_string(&temp_checksums)
                .map_err(|e| format!("cannot read checksums: {e}"))?;
            let expected_filename = format!("companion-{clean_id}.zip");
            if let Err(e) = verify_checksum(&zip_bytes, &checksums_text, &expected_filename) {
                cleanup();
                return Err(e);
            }
            let _ = std::fs::remove_file(&temp_checksums);

            let extract = std::process::Command::new("tar")
                .args([
                    "-xf",
                    temp_zip.to_str().unwrap_or_default(),
                    "-C",
                    dest.to_str().unwrap_or_default(),
                ])
                .status();
            let _ = std::fs::remove_file(&temp_zip);
            if !extract.map(|s| s.success()).unwrap_or(false) {
                return Err(format!("failed to extract companion {clean_id}"));
            }
        }
    }

    // 3. Ensure manifest.json exists
    let manifest_path = dest.join("manifest.json");
    if manifest_path.exists() {
        if let Ok(c) = std::fs::read_to_string(&manifest_path) {
            if let Ok(mut item) = serde_json::from_str::<CompanionItem>(&c) {
                item.is_installed = true;
                return Ok(item);
            }
        }
    }

    let name_cap =
        clean_id.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
            + if clean_id.len() > 1 { &clean_id[1..] } else { "" };
    let item = CompanionItem {
        id: clean_id.clone(),
        name: name_cap,
        description: format!("Downloaded {clean_id} companion pack"),
        tags: vec!["downloaded".to_string(), "catalog".to_string()],
        is_custom: false,
        is_installed: true,
        avatar_type: "clips".to_string(),
        actions: vec![
            "idle".to_string(),
            "walk".to_string(),
            "wave".to_string(),
            "water".to_string(),
            "stretch".to_string(),
            "happy".to_string(),
            "sad".to_string(),
            "eyes".to_string(),
        ],
        colors: Some(CompanionColors {
            body: "#2563eb".to_string(),
            accent: "#c2331a".to_string(),
            eyes: "#14120f".to_string(),
        }),
    };
    let json = serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?;
    let _ = std::fs::write(&manifest_path, json);
    Ok(item)
}

/// Verifies the SHA-256 digest of `data` against a `checksums.txt` entry for `filename`.
///
/// `checksums.txt` format (one entry per line):
/// `<hex-sha256>  <filename>`
///
/// Returns `Ok(())` if the digest matches, `Err(message)` otherwise.  Keeping this as
/// a pure function (no I/O) makes it unit-testable without touching the network.
pub fn verify_checksum(data: &[u8], checksums_txt: &str, filename: &str) -> Result<(), String> {
    // Find the expected digest for this filename.
    let expected_hex = checksums_txt
        .lines()
        .find_map(|line| {
            // Allow both "  " (sha256sum style) and " " separators.
            let line = line.trim();
            if let Some((hex, name)) = line.split_once("  ").or_else(|| line.split_once(' ')) {
                if name.trim() == filename {
                    return Some(hex.trim().to_lowercase());
                }
            }
            None
        })
        .ok_or_else(|| {
            format!("companion-{filename} is not listed in checksums.txt — refusing to install an unlisted file")
        })?;

    if expected_hex.len() != 64 {
        return Err(format!(
            "checksums.txt entry for {filename} has unexpected length {} (want 64 hex chars)",
            expected_hex.len()
        ));
    }

    // Compute SHA-256 using the sha2 crate already in the dependency tree.
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let actual = hasher.finalize();
    let actual_hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();

    if actual_hex != expected_hex {
        return Err(format!(
            "checksum mismatch for {filename}: \
             expected {expected_hex}, got {actual_hex} — the file may be corrupt or tampered with"
        ));
    }

    Ok(())
}

#[tauri::command]
pub fn uninstall_autostart() -> Result<bool, String> {
    tether::setup::uninstall_autostart()
}

#[tauri::command]
pub fn purge_vault_data() -> Result<bool, String> {
    let dir = tether::default_data_dir();
    let db_path = dir.join("vault.db");
    let wal_path = dir.join("vault.db-wal");
    let shm_path = dir.join("vault.db-shm");
    if db_path.exists() {
        let _ = std::fs::remove_file(db_path);
    }
    if wal_path.exists() {
        let _ = std::fs::remove_file(wal_path);
    }
    if shm_path.exists() {
        let _ = std::fs::remove_file(shm_path);
    }
    Ok(true)
}

#[tauri::command]
pub fn uninstall_app(purge_data: bool) -> Result<UninstallResult, String> {
    let autostart_removed = tether::setup::uninstall_autostart().unwrap_or(false);
    let dir = tether::default_data_dir();
    let vault_purged = if purge_data {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
            true
        } else {
            false
        }
    } else {
        false
    };

    // On Windows: find the NSIS-generated uninstaller via `reg query`, launch it,
    // then exit so the uninstaller can remove the binary while it is not in use.
    #[cfg(target_os = "windows")]
    {
        if let Some(uninst) = find_nsis_uninstall_string() {
            let _ = std::process::Command::new(&uninst).spawn();
            // Exit after a short delay so the UI can show the confirmation first.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                std::process::exit(0);
            });
            return Ok(UninstallResult {
                autostart_removed,
                vault_purged,
                vault_path: dir.display().to_string(),
                message: "Launching Windows uninstaller — Tether will close now.".to_string(),
            });
        }
    }

    Ok(UninstallResult {
        autostart_removed,
        vault_purged,
        vault_path: dir.display().to_string(),
        message: if vault_purged {
            "Autostart removed and local vault completely purged.".to_string()
        } else {
            "Autostart removed. Local vault was preserved. To fully remove Tether, \
             use Add or Remove Programs in Windows Settings."
                .to_string()
        },
    })
}

/// Queries the Windows registry for the NSIS `UninstallString` value.
/// Checks both HKCU (currentUser install) and HKLM (perMachine install).
/// Uses `reg query` so no extra crate dependency is needed.
#[cfg(target_os = "windows")]
fn find_nsis_uninstall_string() -> Option<String> {
    let keys = [
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\app.tether.desktop",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\app.tether.desktop",
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Tether",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Tether",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\app.tether.desktop",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Tether",
    ];
    for key in &keys {
        let out = std::process::Command::new("reg")
            .args(["query", key, "/v", "UninstallString"])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let text = String::from_utf8_lossy(&o.stdout);
                // Output format: "    UninstallString    REG_SZ    C:\path\to\uninst.exe"
                for line in text.lines() {
                    if line.contains("UninstallString") {
                        // Split on "REG_SZ" and take what follows.
                        if let Some(pos) = line.find("REG_SZ") {
                            let path = line[pos + 6..].trim().trim_matches('"');
                            if !path.is_empty() {
                                return Some(path.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[tauri::command]
pub fn get_companion_clip(id: String, action: String) -> Result<Option<String>, String> {
    let clean_id = id.trim().to_lowercase();
    let clean_action = action.trim().to_lowercase();
    if clean_id.is_empty() || clean_action.is_empty() {
        return Ok(None);
    }

    // Prefer the animated .webp; fall back to idle.webp only (never .still.webp
    // since those are single-frame stills and would look frozen on the desktop).
    let candidates = [
        format!("{clean_action}.webp"),
        "idle.webp".to_string(),
    ];

    let search_roots = [
        tether::default_data_dir().join("companions").join(&clean_id),
        bundled_companions_dir().join(&clean_id),
    ];

    for root in &search_roots {
        for candidate in &candidates {
            let p = root.join(candidate);
            if p.is_file() {
                if let Ok(bytes) = std::fs::read(&p) {
                    let b64 = to_base64(&bytes);
                    return Ok(Some(format!("data:image/webp;base64,{b64}")));
                }
            }
        }
    }

    Ok(None)
}

/// Shows (un-hides) the main application window.
/// Called from the frontend when the user wants to bring Tether back after the
/// window was hidden via the close button — no extra plugin or OS permission needed.
#[tauri::command]
pub fn show_main_window(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// Logical CSS dimensions of the compact floating companion overlay window.
/// Sized tightly around the 160×220 companion + headroom for speech bubbles (~5%
/// of a 1080p screen) instead of full-screen so:
/// 1. Chromium browsers (Chrome/Edge/Brave/DuckDuckGo) never trigger Native Window
///    Occlusion and blank out pages underneath.
/// 2. Windows Explorer never classifies the overlay as fullscreen, so the auto-hide
///    taskbar always reveals on edge hover.
/// 3. No Win32 `SetWindowRgn` GDI clipping is needed — eliminating white-box flashes.
const OVERLAY_LOGICAL_W: f64 = 360.0;
const OVERLAY_LOGICAL_H: f64 = 310.0;

#[cfg(target_os = "windows")]
mod hit_tracker {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    /// Physical screen-space rect `(left, top, right, bottom)` of the companion's
    /// interactive body inside the compact overlay window.
    static HIT_RECT: Mutex<(i32, i32, i32, i32)> = Mutex::new((0, 0, 0, 0));
    /// Held true while the user is actively dragging so fast cursor moves never drop drag.
    pub static DRAG_LOCK: AtomicBool = AtomicBool::new(false);
    /// Tracks whether the overlay window is currently shown.
    pub static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
    /// Cached `set_ignore_cursor_events` state so we only flip `WS_EX_TRANSPARENT`
    /// on hover enter/leave transitions (zero repaint churn).
    pub static CURRENTLY_IGNORING: AtomicBool = AtomicBool::new(true);
    /// Ensures at most one lightweight cursor-tracking thread is spawned.
    static STARTED: AtomicBool = AtomicBool::new(false);

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(lp_point: *mut Point) -> i32;
    }

    pub fn set_rect(left: i32, top: i32, right: i32, bottom: i32) {
        let mut g = HIT_RECT.lock().unwrap_or_else(|e| e.into_inner());
        *g = (left, top, right, bottom);
    }

    pub fn ensure_started(app: tauri::AppHandle) {
        use tauri::Manager as _;
        if STARTED.swap(true, Ordering::AcqRel) {
            return;
        }
        std::thread::spawn(move || loop {
            if !OVERLAY_VISIBLE.load(Ordering::Relaxed) {
                if !CURRENTLY_IGNORING.load(Ordering::Relaxed) {
                    if let Some(w) = app.get_webview_window("companion-overlay") {
                        let _ = w.set_ignore_cursor_events(true);
                    }
                    CURRENTLY_IGNORING.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            let should_ignore = if DRAG_LOCK.load(Ordering::Relaxed) {
                false
            } else {
                let (l, t, r, b) = *HIT_RECT.lock().unwrap_or_else(|e| e.into_inner());
                if r <= l || b <= t {
                    true
                } else {
                    let mut pt = Point { x: 0, y: 0 };
                    let ok = unsafe { GetCursorPos(&mut pt) } != 0;
                    if ok {
                        !(pt.x >= l && pt.x <= r && pt.y >= t && pt.y <= b)
                    } else {
                        true
                    }
                }
            };

            if should_ignore != CURRENTLY_IGNORING.load(Ordering::Relaxed) {
                if let Some(w) = app.get_webview_window("companion-overlay") {
                    if w.set_ignore_cursor_events(should_ignore).is_ok() {
                        CURRENTLY_IGNORING.store(should_ignore, Ordering::Relaxed);
                    }
                } else {
                    break;
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(20));
        });
    }
}

/// Shows the compact transparent companion overlay window near the bottom centre
/// of the primary monitor without stealing focus or triggering browser occlusion.
#[tauri::command]
pub fn show_companion_overlay(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{Manager as _, PhysicalPosition, PhysicalSize};
    let w =
        app.get_webview_window("companion-overlay").ok_or("companion-overlay window not found")?;

    let already_visible = w.is_visible().unwrap_or(false);

    if let Ok(Some(monitor)) = app.primary_monitor() {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let pw = (OVERLAY_LOGICAL_W * scale).round() as u32;
        let ph = (OVERLAY_LOGICAL_H * scale).round() as u32;
        let _ = w.set_size(PhysicalSize::new(pw, ph));
        // Only place at bottom-centre when showing from hidden — never teleport
        // an already-visible roaming companion back to the bottom on character switch.
        if !already_visible {
            let start_x = pos.x + ((size.width as i32 - pw as i32) / 2).max(0);
            let start_y =
                pos.y + (size.height as i32 - ph as i32 - (8.0 * scale).round() as i32).max(0);
            let _ = w.set_position(PhysicalPosition::new(start_x, start_y));
        }
    }

    // Disable native window shadow and Windows 11 1px DWM border ring so the
    // transparent overlay has zero visible box around the companion.
    let _ = w.set_shadow(false);
    #[cfg(target_os = "windows")]
    strip_dwm_border(&w);

    // Start click-through; the hover tracker clears WS_EX_TRANSPARENT only when
    // the cursor hovers over the companion's body.
    let _ = w.set_ignore_cursor_events(true);

    // Show without stealing keyboard focus — the overlay is purely visual.
    w.show().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::Ordering;
        set_topmost_noactivate(&w);
        hit_tracker::DRAG_LOCK.store(false, Ordering::Relaxed);
        hit_tracker::CURRENTLY_IGNORING.store(true, Ordering::Relaxed);
        hit_tracker::OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
        hit_tracker::ensure_started(app.clone());
    }

    Ok(())
}

/// Toggles OS-level cursor passthrough on the companion overlay window.
/// Pass `ignore = false` during active drag so fast cursor movement never drops it.
#[tauri::command]
pub fn set_companion_passthrough(app: tauri::AppHandle, ignore: bool) -> Result<(), String> {
    use tauri::Manager as _;
    let w =
        app.get_webview_window("companion-overlay").ok_or("companion-overlay window not found")?;
    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::Ordering;
        hit_tracker::DRAG_LOCK.store(!ignore, Ordering::Relaxed);
        hit_tracker::CURRENTLY_IGNORING.store(ignore, Ordering::Relaxed);
    }
    w.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())
}

/// Repositions the compact companion overlay window to logical screen position
/// `(x, y)` and updates the character body's interactive hitbox `(width, height)`.
///
/// Uses DWM-composited `SetWindowPos(SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)`
/// without any `SetWindowRgn` GDI clipping — zero white-box flashing, zero browser
/// occlusion, and smooth 60 fps movement across the entire desktop.
#[tauri::command]
pub fn set_hit_zone_region(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
) -> Result<(), String> {
    use tauri::Manager as _;
    let w =
        app.get_webview_window("companion-overlay").ok_or("companion-overlay window not found")?;

    let s = if scale > 0.0 { scale } else { 1.0 };
    let win_size = w.outer_size().ok();
    let (win_pw, win_ph) = match win_size {
        Some(sz) if sz.width > 0 && sz.height > 0 => (sz.width as i32, sz.height as i32),
        _ => ((OVERLAY_LOGICAL_W * s).round() as i32, (OVERLAY_LOGICAL_H * s).round() as i32),
    };

    let (mon_x, mon_y, mon_w, mon_h) = match app.primary_monitor() {
        Ok(Some(m)) => {
            let p = m.position();
            let sz = m.size();
            (p.x, p.y, sz.width as i32, sz.height as i32)
        }
        _ => (0, 0, (1920.0 * s) as i32, (1080.0 * s) as i32),
    };

    // Keep the overlay within primary monitor bounds with at least 6 logical px
    // clearance from the bottom edge so the Windows auto-hide taskbar always triggers.
    let bottom_pad = (6.0 * s).round() as i32;
    let max_x = mon_x + (mon_w - win_pw).max(0);
    let max_y = mon_y + (mon_h - win_ph - bottom_pad).max(0);
    let wx = (mon_x + (x as f64 * s).round() as i32).clamp(mon_x, max_x);
    let wy = (mon_y + (y as f64 * s).round() as i32).clamp(mon_y, max_y);

    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;

        #[link(name = "user32")]
        extern "system" {
            fn SetWindowPos(
                hwnd: *mut c_void,
                insert_after: *mut c_void,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                flags: u32,
            ) -> i32;
        }

        if let Ok(hwnd_raw) = w.hwnd() {
            unsafe {
                SetWindowPos(
                    hwnd_raw.0,
                    std::ptr::null_mut(),
                    wx,
                    wy,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        // Update physical screen-space hitbox of the character body centered in the window.
        let hit_pw = ((width as f64 * s).round() as i32).clamp(1, win_pw);
        let hit_ph = ((height as f64 * s).round() as i32).clamp(1, win_ph);
        let hit_l = wx + (win_pw - hit_pw) / 2;
        let hit_b = wy + win_ph - (10.0 * s).round() as i32;
        let hit_t = (hit_b - hit_ph).max(wy);
        hit_tracker::set_rect(hit_l, hit_t, hit_l + hit_pw, hit_b);
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri::PhysicalPosition;
        let _ = (width, height);
        let _ = w.set_position(PhysicalPosition::new(wx, wy));
    }

    Ok(())
}

/// Hides the companion overlay window.  Called from the main window via IPC.
#[tauri::command]
pub fn hide_companion_overlay(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager as _;
    let w =
        app.get_webview_window("companion-overlay").ok_or("companion-overlay window not found")?;
    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::Ordering;
        hit_tracker::OVERLAY_VISIBLE.store(false, Ordering::Relaxed);
        hit_tracker::DRAG_LOCK.store(false, Ordering::Relaxed);
    }
    w.hide().map_err(|e| e.to_string())
}

/// Initiates a native OS window drag for the companion overlay.
/// The overlay calls this on mousedown so the user can reposition it.
#[tauri::command]
pub fn start_companion_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

/// Returns the current capture status: whether the keyboard hook is active, how many
/// events have been seen, and how many drafts are stored in the vault.
#[tauri::command]
pub fn get_capture_status(
    state: tauri::State<'_, std::sync::Mutex<crate::AppState>>,
) -> Result<CaptureStatus, String> {
    let guard = state.lock().map_err(|_| "state lock poisoned".to_string())?;
    let capturing = guard.capturing.load(std::sync::atomic::Ordering::Relaxed);
    let (events_seen, drafts_saved) = match &guard.engine {
        Some(engine) => {
            let seen = engine.stats.events_seen.load(std::sync::atomic::Ordering::Relaxed);
            let commits = engine.stats.commits.load(std::sync::atomic::Ordering::Relaxed);
            (seen, commits)
        }
        None => (0, 0),
    };
    Ok(CaptureStatus { capturing, events_seen, drafts_saved })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub capturing: bool,
    pub events_seen: u64,
    pub drafts_saved: u64,
}

#[tauri::command]
pub fn get_system_status() -> Result<SystemStatusInfo, String> {
    let dir = tether::default_data_dir();
    let autostart_installed = get_autostart_status().unwrap_or(false);
    let (cfg, _) = tether::config::Config::load_or_default(&dir);

    Ok(SystemStatusInfo {
        platform: std::env::consts::OS.to_string(),
        uptime_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        autostart_installed,
        retention_days: cfg.retention_days,
        vault_exists: dir.join("vault.db").exists(),
        vault_path: dir.display().to_string(),
    })
}

/// Serialised form of the active companion stored in the data directory.
/// Both the main window and the companion overlay read/write through this
/// so they share one source of truth instead of each window's isolated localStorage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCompanionState {
    pub id: String,
    pub name: String,
    pub avatar_type: String,
    pub colors: CompanionColors,
    /// The enabled actions for this companion. When non-empty the overlay state
    /// machine cycles only through these states (all 8 by default).
    #[serde(default)]
    pub actions: Vec<String>,
    /// Free roam mode flag shared across WebViews.
    #[serde(default)]
    pub free_roam: bool,
    /// Whether the companion overlay window should be visible.
    /// Persisted so the user's show/hide choice survives a system restart.
    /// Defaults to `true` on first run and when reading older state files
    /// that pre-date this field.
    #[serde(default = "default_visible")]
    pub visible: bool,
}

fn default_visible() -> bool {
    true
}

/// Returns the currently active companion from the shared state file.
/// If the stored state has no actions list (older save), look up the companion's
/// manifest to populate it — so the overlay always gets the full action set.
#[tauri::command]
pub fn get_active_companion() -> ActiveCompanionState {
    let path = tether::default_data_dir().join("active-companion.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(mut state) = serde_json::from_str::<ActiveCompanionState>(&raw) {
            if !state.id.is_empty() {
                // Backfill actions if missing (older saves or companions set before this change).
                if state.actions.is_empty() {
                    state.actions = resolve_companion_actions(&state.id);
                }
                return state;
            }
        }
    }
    ActiveCompanionState {
        id: "grace".into(),
        name: "Grace".into(),
        avatar_type: "procedural".into(),
        colors: CompanionColors {
            body: "#2563eb".into(),
            accent: "#c2331a".into(),
            eyes: "#14120f".into(),
        },
        actions: default_actions(),
        free_roam: false,
        visible: true,
    }
}

/// Returns the standard 8-action list that every bundled companion ships with.
fn default_actions() -> Vec<String> {
    ["idle", "walk", "wave", "water", "stretch", "happy", "sad", "eyes"]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Looks up the companion manifest to get its declared action list.
/// Falls back to the standard 8 actions if the manifest cannot be read.
fn resolve_companion_actions(id: &str) -> Vec<String> {
    let candidates =
        [tether::default_data_dir().join("companions").join(id), bundled_companions_dir().join(id)];
    for dir in &candidates {
        let manifest = dir.join("manifest.json");
        if let Ok(raw) = std::fs::read_to_string(manifest) {
            if let Ok(item) = serde_json::from_str::<CompanionItem>(&raw) {
                if !item.actions.is_empty() {
                    return item.actions;
                }
            }
        }
    }
    default_actions()
}

/// Persists the active companion choice to disk so both the main window and the
/// companion overlay can read it via IPC instead of relying on isolated localStorage.
#[tauri::command]
pub fn set_active_companion(state: ActiveCompanionState) -> Result<(), String> {
    let dir = tether::default_data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&state).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("active-companion.json"), json).map_err(|e| e.to_string())
}

/// On Windows: sets HWND_TOPMOST with SWP_NOACTIVATE so the companion overlay
/// floats above normal windows without suppressing the taskbar auto-hide reveal.
/// The standard `set_always_on_top(true)` path in Tauri uses a different set of
/// flags that can interfere with the shell's WM_APPACTIVATE detection.
#[cfg(target_os = "windows")]
fn set_topmost_noactivate(w: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    // Win32 SetWindowPos constants.
    const TOPMOST: *mut c_void = -1isize as *mut c_void;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOACTIVATE: u32 = 0x0010;

    // We call user32 directly — it is always linked on Windows and is already
    // used by lib.rs, so there is no extra linker cost.
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(
            hwnd: *mut c_void,
            insert_after: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    if let Ok(hwnd_raw) = w.hwnd() {
        unsafe {
            // Position as HWND_TOPMOST without stealing activation/focus.
            // This ensures the companion floats above normal application windows
            // while remaining non-intrusive.
            SetWindowPos(hwnd_raw.0, TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// On Windows 11, DWM draws a 1px hairline border and rounded corner chrome even
/// on borderless transparent windows unless both attributes are explicitly turned
/// off. Setting `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE` and `DWMWCP_DONOTROUND`
/// removes the visible box while preserving per-pixel alpha transparency.
#[cfg(target_os = "windows")]
fn strip_dwm_border(w: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            dw_attribute: u32,
            pv_attribute: *const c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    if let Ok(hwnd_raw) = w.hwnd() {
        unsafe {
            let corner = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd_raw.0,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let border = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd_raw.0,
                DWMWA_BORDER_COLOR,
                &border as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

/// Information returned when checking for an application update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_notes: String,
    pub release_url: String,
    pub installer_url: String,
}

/// Compares two semver strings like "0.1.33" and "0.1.34".
/// Returns true if `remote` is strictly greater than `local`.
pub fn is_newer_semver(local: &str, remote: &str) -> bool {
    let parse_nums = |s: &str| -> Vec<u64> {
        let clean = s.trim().trim_start_matches('v');
        clean
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect()
    };
    let l_nums = parse_nums(local);
    let r_nums = parse_nums(remote);
    let max_len = l_nums.len().max(r_nums.len());
    for i in 0..max_len {
        let l = *l_nums.get(i).unwrap_or(&0);
        let r = *r_nums.get(i).unwrap_or(&0);
        if r > l {
            return true;
        } else if r < l {
            return false;
        }
    }
    false
}

/// Checks the latest GitHub release for Tether and determines if a newer version exists.
#[tauri::command]
pub fn check_for_updates() -> Result<UpdateCheckResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let api_url = "https://api.github.com/repos/AMG555/Tether/releases/latest";

    let json_str = {
        #[cfg(target_os = "windows")]
        {
            let out = std::process::Command::new("curl.exe")
                .args(["-fsSL", "-H", "User-Agent: Tether-Desktop-App", api_url])
                .output();
            match out {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                _ => {
                    let ps_script = format!(
                        r#"$res = Invoke-RestMethod -Uri '{}' -Headers @{{'User-Agent'='Tether-Desktop-App'}} -TimeoutSec 10; $res | ConvertTo-Json -Depth 4"#,
                        api_url
                    );
                    let ps_out = std::process::Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", &ps_script])
                        .output()
                        .map_err(|e| format!("failed to check updates: {e}"))?;
                    if !ps_out.status.success() {
                        return Err(format!(
                            "GitHub release check failed: {}",
                            String::from_utf8_lossy(&ps_out.stderr).trim()
                        ));
                    }
                    String::from_utf8_lossy(&ps_out.stdout).to_string()
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let out = std::process::Command::new("curl")
                .args(["-fsSL", "-H", "User-Agent: Tether-Desktop-App", api_url])
                .output()
                .map_err(|e| format!("failed to check updates: {e}"))?;
            if !out.status.success() {
                return Err("GitHub release check failed".into());
            }
            String::from_utf8_lossy(&out.stdout).to_string()
        }
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid release payload: {e}"))?;

    let tag = parsed
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing tag_name in release".to_string())?;

    let latest_version = tag.trim_start_matches('v').to_string();
    let release_notes = parsed
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let release_url = parsed
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com/AMG555/Tether/releases/latest")
        .to_string();

    let update_available = is_newer_semver(&current_version, &latest_version);

    let installer_url = format!(
        "https://github.com/AMG555/Tether/releases/download/{tag}/Tether_x64-setup.exe"
    );

    Ok(UpdateCheckResult {
        current_version,
        latest_version,
        update_available,
        release_notes,
        release_url,
        installer_url,
    })
}

/// Downloads the latest Windows NSIS installer, verifies SHA-256 against checksums.txt,
/// unblocks Zone.Identifier (prevents SmartScreen), and launches the installer.
#[tauri::command]
pub fn install_update() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // 1. Fetch release info to know the tag and asset URLs.
        let check = check_for_updates()?;
        if !check.update_available {
            return Ok("Application is already up to date.".to_string());
        }

        let tag = format!("v{}", check.latest_version);
        let installer_url = format!(
            "https://github.com/AMG555/Tether/releases/download/{tag}/Tether_x64-setup.exe"
        );
        let checksums_url = format!(
            "https://github.com/AMG555/Tether/releases/download/{tag}/checksums.txt"
        );

        let temp_dir = std::env::temp_dir();
        let target_exe = temp_dir.join(format!("Tether_{}_x64-setup.exe", check.latest_version));
        let temp_checksums = temp_dir.join(format!("tether-update-checksums-{}.txt", check.latest_version));

        // Helper cleanup
        let cleanup = || {
            let _ = std::fs::remove_file(&target_exe);
            let _ = std::fs::remove_file(&temp_checksums);
        };

        // Download installer
        let status = std::process::Command::new("curl.exe")
            .args(["-fSL", &installer_url, "-o", target_exe.to_str().unwrap_or_default()])
            .status();
        let downloaded = match status {
            Ok(s) if s.success() => true,
            _ => {
                let ps_script = format!(
                    "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                    installer_url,
                    target_exe.display()
                );
                std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-Command", &ps_script])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }
        };

        if !downloaded || !target_exe.exists() {
            cleanup();
            return Err("Failed to download update installer from GitHub Releases.".into());
        }

        // Download checksums.txt
        let cs_ok = std::process::Command::new("curl.exe")
            .args(["-fSL", &checksums_url, "-o", temp_checksums.to_str().unwrap_or_default()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if cs_ok && temp_checksums.exists() {
            if let Ok(cs_text) = std::fs::read_to_string(&temp_checksums) {
                if let Ok(exe_bytes) = std::fs::read(&target_exe) {
                    // Try verifying with Tether_x64-setup.exe or versioned name
                    let v_name = format!("Tether_{}_x64-setup.exe", check.latest_version);
                    let ok1 = verify_checksum(&exe_bytes, &cs_text, "Tether_x64-setup.exe");
                    let ok2 = verify_checksum(&exe_bytes, &cs_text, &v_name);
                    if ok1.is_err() && ok2.is_err() {
                        cleanup();
                        return Err(format!(
                            "Update verification failed: {}",
                            ok1.err().unwrap_or_else(|| "checksum mismatch".into())
                        ));
                    }
                }
            }
        }

        // Unblock file to avoid SmartScreen unrecognized warning on downloaded binary
        let unblock_script = format!(
            "Unblock-File -Path '{}' -ErrorAction SilentlyContinue",
            target_exe.display()
        );
        let _ = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &unblock_script])
            .status();

        // Launch the NSIS installer. NSIS automatically upgrades the installation in place
        // without touching %APPDATA%/tether/data (vault.db and user settings are safe).
        let spawn_res = std::process::Command::new(&target_exe).spawn();
        match spawn_res {
            Ok(_) => {
                Ok(format!(
                    "Update {} downloaded. The installer is now launching to complete the update.",
                    check.latest_version
                ))
            }
            Err(e) => Err(format!("Failed to launch update installer: {e}")),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("In-app automatic updates are currently supported on Windows. Please update via package manager or GitHub releases.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_contains_only_non_sensitive_camel_case_metadata() {
        let info = get_app_info();
        assert_eq!(
            serde_json::to_value(info).unwrap(),
            serde_json::json!({
                "name": "Tether",
                "version": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS,
            })
        );
    }

    #[test]
    fn is_newer_semver_correctly_evaluates_versions() {
        assert!(is_newer_semver("0.1.33", "0.1.34"));
        assert!(is_newer_semver("0.1.33", "0.2.0"));
        assert!(is_newer_semver("0.1.33", "1.0.0"));
        assert!(is_newer_semver("v0.1.33", "v0.1.35"));
        assert!(!is_newer_semver("0.1.33", "0.1.33"));
        assert!(!is_newer_semver("0.1.34", "0.1.33"));
        assert!(!is_newer_semver("1.0.0", "0.9.9"));
    }

    #[test]
    fn app_info_has_a_stable_response_type() {
        let info: AppInfo = get_app_info();
        assert_eq!(info.name, "Tether");
    }

    #[test]
    fn vault_status_reports_offline_and_aes256() {
        let status = get_vault_status().unwrap();
        assert!(status.offline);
        assert_eq!(status.encryption, "AES-256-GCM + HKDF");
        // retention_days is user-configurable; just verify it is a sane positive value.
        assert!(status.retention_days >= 1, "retention_days must be positive, got {}", status.retention_days);
    }

    #[test]
    fn list_companions_includes_builtins() {
        let companions = list_companions().unwrap();
        assert!(companions.len() >= 4);
        assert!(companions.iter().any(|c| c.id == "grace"));
        assert!(companions.iter().any(|c| c.id == "rory"));
    }

    #[test]
    fn install_catalog_companion_sanitizes_id() {
        assert!(install_catalog_companion("..".into()).is_err());
        assert!(install_catalog_companion("".into()).is_err());
        assert!(install_catalog_companion("foo/bar".into()).is_err());
    }

    #[test]
    fn system_status_reports_local_platform_and_vault() {
        let status = get_system_status().unwrap();
        assert_eq!(status.platform, std::env::consts::OS);
        assert!(status.retention_days >= 1);
    }

    #[test]
    fn to_base64_encodes_standard_vectors() {
        assert_eq!(to_base64(b""), "");
        assert_eq!(to_base64(b"f"), "Zg==");
        assert_eq!(to_base64(b"fo"), "Zm8=");
        assert_eq!(to_base64(b"foo"), "Zm9v");
    }

    #[test]
    fn get_companion_clip_returns_none_for_missing() {
        let clip = get_companion_clip("nonexistent-companion-xyz".into(), "idle".into()).unwrap();
        assert!(clip.is_none());
    }

    #[test]
    fn get_companion_clip_returns_data_url_for_bundled_grace() {
        // This test verifies that the bundled_companions_dir() resolution finds the
        // real grace/idle.webp shipped with the app.  It runs against the debug build
        // where assets/companions/ is copied next to the exe by the build script.
        let clip = get_companion_clip("grace".into(), "idle".into()).unwrap();
        assert!(
            clip.is_some(),
            "grace/idle.webp must be found — bundled_companions_dir() path resolution failed"
        );
        let url = clip.unwrap();
        assert!(
            url.starts_with("data:image/webp;base64,"),
            "clip must be a webp data URL, got: {}",
            &url[..url.len().min(60)]
        );
        assert!(url.len() > 100, "data URL must have real content, not empty base64");
    }

    // ── verify_checksum unit tests ────────────────────────────────────────────

    /// Build a real SHA-256 hex string for a byte slice using sha2 so the golden
    /// value tracks any future dependency change automatically.
    fn sha256_hex(data: &[u8]) -> String {
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(data);
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn verify_checksum_accepts_matching_digest() {
        let data = b"hello companion";
        let hex = sha256_hex(data);
        let checksums = format!("{hex}  companion-grace.zip\n");
        assert!(
            verify_checksum(data, &checksums, "companion-grace.zip").is_ok(),
            "matching digest must be accepted"
        );
    }

    #[test]
    fn verify_checksum_rejects_wrong_digest() {
        let data = b"real zip bytes";
        let hex = sha256_hex(data);
        let tampered = b"tampered zip bytes";
        let checksums = format!("{hex}  companion-grace.zip\n");
        let err = verify_checksum(tampered, &checksums, "companion-grace.zip")
            .expect_err("mismatched digest must be rejected");
        assert!(err.contains("mismatch"), "error should mention mismatch: {err}");
    }

    #[test]
    fn verify_checksum_rejects_unlisted_filename() {
        let data = b"some bytes";
        let hex = sha256_hex(data);
        let checksums = format!("{hex}  companion-rory.zip\n");
        let err = verify_checksum(data, &checksums, "companion-grace.zip")
            .expect_err("unlisted file must be rejected");
        assert!(err.contains("not listed"), "error should mention not listed: {err}");
    }

    #[test]
    fn verify_checksum_handles_single_space_separator() {
        let data = b"single space format";
        let hex = sha256_hex(data);
        // Some tools emit a single space instead of two.
        let checksums = format!("{hex} companion-finn.zip\n");
        assert!(
            verify_checksum(data, &checksums, "companion-finn.zip").is_ok(),
            "single-space separator must be accepted"
        );
    }
}
