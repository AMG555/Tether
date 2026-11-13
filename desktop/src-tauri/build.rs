fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_app_info",
                "get_vault_status",
                "list_drafts",
                "get_draft",
                "get_revisions",
                "delete_draft",
                "prune_vault",
                "get_autostart_status",
                "set_autostart",
                "list_companions",
                "save_custom_companion",
                "delete_custom_companion",
                "install_catalog_companion",
            ]),
        ),
    )
    .expect("failed to configure the Tauri command allowlist");
}
