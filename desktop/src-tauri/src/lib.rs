mod commands;
mod diagnostics;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::Manager as _;

/// Shared capture state, held for the lifetime of the Tauri process.
///
/// Design: the Engine lives here (not in commands.rs) so the capture loop
/// and the UI command handlers share the same in-process vault and ring.
/// Commands that only need vault access open their own handle via
/// `open_local_vault()` to avoid holding the engine lock across IPC calls.
pub struct AppState {
    /// The running capture engine. `None` only during the brief window
    /// between app start and the first `start_capture` call if capture
    /// failed to initialise (e.g. vault key missing on first run).
    pub engine: Option<Arc<tether::engine::Engine>>,
    /// Flipped to `true` to stop the capture and flush threads cleanly.
    pub stop: Arc<AtomicBool>,
    /// Whether the keyboard hook thread is currently running.
    pub capturing: Arc<AtomicBool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            engine: None,
            stop: Arc::new(AtomicBool::new(false)),
            capturing: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(AppState::new()))
        // Intercept the X-button: hide the main window instead of closing it.
        // The process keeps running so the in-process capture stays active.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Start the capture engine on the main Tauri setup thread.
            // Any failure here is non-fatal: the UI still loads and shows the
            // vault; it will display a "capture not running" status to the user.
            let dir = tether::default_data_dir();
            let key_file = dir.join("key.txt");
            let started = if key_file.exists() {
                match commands::open_local_vault() {
                    Ok(vault) => {
                        match tether::engine::Engine::new(vault, 4096, tether::policy::Policy::default()) {
                            Ok(engine) => {
                                // Replay anything in the crash buffer before accepting new input.
                                let _ = engine.recover();
                                // Load retention preference.
                                let (cfg, _) = tether::config::Config::load_or_default(&dir);
                                engine.set_retention_days(cfg.retention_days);

                                let engine = Arc::new(engine);
                                let stop = Arc::new(AtomicBool::new(false));
                                let capturing = Arc::new(AtomicBool::new(false));

                                // Flush thread: commits keystrokes to the vault every second.
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    std::thread::spawn(move || {
                                        e.run_commit_loop(&s, Duration::from_secs(1));
                                    });
                                }

                                // Capture thread: installs the OS keyboard hook.
                                {
                                    let e = Arc::clone(&engine);
                                    let s = Arc::clone(&stop);
                                    let c = Arc::clone(&capturing);
                                    std::thread::spawn(move || {
                                        c.store(true, Ordering::Relaxed);
                                        let mut source = tether::capture::default_source();
                                        let sink: Arc<dyn Fn(tether::ring::Event) + Send + Sync> =
                                            Arc::new(move |ev| e.ingest(ev));
                                        source.run(sink, &s);
                                        c.store(false, Ordering::Relaxed);
                                    });
                                }

                                // Store state for command handlers.
                                let state = app.state::<std::sync::Mutex<AppState>>();
                                if let Ok(mut guard) = state.lock() {
                                    guard.engine = Some(engine);
                                    guard.stop = stop;
                                    guard.capturing = capturing;
                                }
                                true
                            }
                            Err(_) => false,
                        }
                    }
                    Err(_) => false,
                }
            } else {
                // First run: vault not yet initialised. User must run `tether init` or
                // the UI will guide them through it. Capture starts after init.
                false
            };
            if !started {
                eprintln!("tether: capture not started — vault not initialised yet (run `tether init`)");
            }
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
            commands::show_main_window,
            commands::show_companion_overlay,
            commands::hide_companion_overlay,
            commands::start_companion_drag,
            commands::get_capture_status,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // Tauri 2 exits the process when the last window closes by default.
            // Prevent that so the in-process capture keeps running in the background.
            // The only intentional exit paths are:
            //   • Settings → Uninstall (calls std::process::exit directly)
            //   • OS kill / SIGTERM
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
    Ok(())
}
