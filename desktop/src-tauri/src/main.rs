#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if tether_desktop::run().is_err() {
        std::process::exit(1);
    }
}
