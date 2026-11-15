mod commands;
mod diagnostics;

use tauri::Manager as _;

pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
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
        // Intercept the X-button: hide the main window instead of closing it.
        // The process must keep running so the crash-ring capture stays active.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
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
            commands::show_main_window,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // Tauri 2 exits the process when the last window closes by default.
            // Prevent that so the crash-ring capture keeps running in the background
            // after the user closes (hides) the main window.
            // The only intentional exit paths are:
            //   • Settings → Uninstall (calls std::process::exit directly)
            //   • OS kill / SIGTERM
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
                // Re-show the main window so the user is not left with nothing.
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
    Ok(())
}
