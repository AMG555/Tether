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
    let dir = tether::default_data_dir().join("companions").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let manifest = CompanionItem {
        id: id.clone(),
        name: id.clone(),
        description: format!("Downloaded {id} companion pack"),
        tags: vec!["downloaded".to_string(), "catalog".to_string()],
        is_custom: false,
        is_installed: true,
        avatar_type: "procedural".to_string(),
        actions: vec![
            "idle".to_string(),
            "walk".to_string(),
            "wave".to_string(),
            "water".to_string(),
            "happy".to_string(),
        ],
        colors: Some(CompanionColors {
            body: "#2563eb".to_string(),
            accent: "#c2331a".to_string(),
            eyes: "#14120f".to_string(),
        }),
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), json).map_err(|e| e.to_string())?;
    Ok(manifest)
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
}
