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
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
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

fn open_local_vault() -> Result<tether::vault::Vault, String> {
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
    let clamped = days.clamp(1, 365);
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
        let p =
            PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs/Startup/tether.url");
        Ok(p.exists())
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
    let mut list = vec![
        CompanionItem {
            id: "grace".into(),
            name: "Grace".into(),
            description: "A warm, grounding presence — reminds you to drink water and rest.".into(),
            tags: vec!["human".into(), "adult".into(), "calm".into()],
            is_custom: false,
            is_installed: true,
            avatar_type: "procedural".into(),
            actions: vec![
                "idle".into(),
                "walk".into(),
                "wave".into(),
                "water".into(),
                "stretch".into(),
            ],
            colors: Some(CompanionColors {
                body: "#2563eb".into(),
                accent: "#c2331a".into(),
                eyes: "#14120f".into(),
            }),
        },
        CompanionItem {
            id: "rory".into(),
            name: "Rory".into(),
            description: "The lion cub heir — small, boisterous, and convinced he runs the place."
                .into(),
            tags: vec!["animal".into(), "cub".into(), "playful".into()],
            is_custom: false,
            is_installed: true,
            avatar_type: "procedural".into(),
            actions: vec!["idle".into(), "walk".into(), "happy".into(), "wave".into()],
            colors: Some(CompanionColors {
                body: "#d97706".into(),
                accent: "#b45309".into(),
                eyes: "#14120f".into(),
            }),
        },
        CompanionItem {
            id: "finn".into(),
            name: "Finn".into(),
            description: "A football kid who takes every break to practice his step-overs.".into(),
            tags: vec!["human".into(), "kid".into(), "sporty".into()],
            is_custom: false,
            is_installed: true,
            avatar_type: "procedural".into(),
            actions: vec!["idle".into(), "walk".into(), "stretch".into(), "happy".into()],
            colors: Some(CompanionColors {
                body: "#059669".into(),
                accent: "#047857".into(),
                eyes: "#14120f".into(),
            }),
        },
        CompanionItem {
            id: "spike".into(),
            name: "Spike".into(),
            description: "A jackfruit with opinions — slow to ripen, but worth the wait.".into(),
            tags: vec!["food".into(), "whimsical".into(), "calm".into()],
            is_custom: false,
            is_installed: true,
            avatar_type: "procedural".into(),
            actions: vec!["idle".into(), "wave".into(), "stretch".into()],
            colors: Some(CompanionColors {
                body: "#65a30d".into(),
                accent: "#4d7c0f".into(),
                eyes: "#14120f".into(),
            }),
        },
    ];

    // Read user custom companions from disk
    let custom_dir = tether::default_data_dir().join("companions");
    if let Ok(entries) = std::fs::read_dir(custom_dir) {
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(manifest_path) {
                    if let Ok(item) = serde_json::from_str::<CompanionItem>(&content) {
                        list.push(item);
                    }
                }
            }
        }
    }

    Ok(list)
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
            "https://github.com/AMG555/Tether/releases/download/v0.1.0/companion-{clean_id}.zip"
        );
        let checksums_url =
            "https://github.com/AMG555/Tether/releases/download/v0.1.0/checksums.txt";
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
                .args([
                    "-fSL",
                    checksums_url,
                    "-o",
                    temp_checksums.to_str().unwrap_or_default(),
                ])
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
            if let Err(e) =
                verify_checksum(&zip_bytes, &checksums_text, &expected_filename)
            {
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
                .args([
                    "-fSL",
                    checksums_url,
                    "-o",
                    temp_checksums.to_str().unwrap_or_default(),
                ])
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
            if let Err(e) =
                verify_checksum(&zip_bytes, &checksums_text, &expected_filename)
            {
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
pub fn verify_checksum(
    data: &[u8],
    checksums_txt: &str,
    filename: &str,
) -> Result<(), String> {
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

    Ok(UninstallResult {
        autostart_removed,
        vault_purged,
        vault_path: dir.display().to_string(),
        message: if vault_purged {
            "Autostart removed and local vault completely purged.".to_string()
        } else {
            "Autostart removed. Local vault was preserved.".to_string()
        },
    })
}

#[tauri::command]
pub fn get_companion_clip(id: String, action: String) -> Result<Option<String>, String> {
    let clean_id = id.trim().to_lowercase();
    let clean_action = action.trim().to_lowercase();
    if clean_id.is_empty() || clean_action.is_empty() {
        return Ok(None);
    }

    let candidates = [
        format!("{clean_action}.webp"),
        format!("{clean_action}.still.webp"),
        "idle.webp".to_string(),
        "idle.still.webp".to_string(),
    ];

    let search_roots = [
        tether::default_data_dir().join("companions").join(&clean_id),
        PathBuf::from("assets/companions").join(&clean_id),
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
    fn app_info_has_a_stable_response_type() {
        let info: AppInfo = get_app_info();
        assert_eq!(info.name, "Tether");
    }

    #[test]
    fn vault_status_reports_offline_and_aes256() {
        let status = get_vault_status().unwrap();
        assert!(status.offline);
        assert_eq!(status.encryption, "AES-256-GCM + HKDF");
        assert_eq!(status.retention_days, 14);
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
