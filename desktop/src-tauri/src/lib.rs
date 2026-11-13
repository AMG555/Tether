mod commands;
mod diagnostics;

pub fn run() -> Result<(), tauri::Error> {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::get_app_info])
        .run(tauri::generate_context!());

    if let Err(error) = &result {
        eprintln!("{}", diagnostics::startup_failure_label(&error.to_string()));
    }

    result
}
