mod commands;
mod diagnostics;

pub fn run() -> Result<(), tauri::Error> {
    let result = tauri::Builder::default()
        .setup(|app| {
            // Companion overlay window — transparent, always-on-top, no decorations.
            // Hidden until a companion is activated from the main window.
            let _overlay = tauri::WebviewWindowBuilder::new(
                app,
                "companion-overlay",
                tauri::WebviewUrl::App("companion-overlay.html".into()),
            )
            .title("Tether Companion")
            .inner_size(220.0, 280.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;
            Ok(())
        })
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
            commands::uninstall_autostart,
            commands::purge_vault_data,
            commands::uninstall_app,
            commands::get_companion_clip,
            commands::get_system_status,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = &result {
        eprintln!("{}", diagnostics::startup_failure_label(&error.to_string()));
    }

    result
}
