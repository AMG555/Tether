mod commands;
mod diagnostics;

pub fn run() -> Result<(), tauri::Error> {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_vault_status,
            commands::list_drafts,
            commands::get_draft,
            commands::get_revisions,
            commands::delete_draft,
            commands::prune_vault,
            commands::set_retention_days,
            commands::get_autostart_status,
            commands::set_autostart,
            commands::list_companions,
            commands::save_custom_companion,
            commands::delete_custom_companion,
            commands::install_catalog_companion,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = &result {
        eprintln!("{}", diagnostics::startup_failure_label(&error.to_string()));
    }

    result
}
