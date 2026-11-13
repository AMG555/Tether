#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    platform: &'static str,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Tether",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}

#[cfg(test)]
mod tests {
    use super::{get_app_info, AppInfo};

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
}
