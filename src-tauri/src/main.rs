// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pos_tauri_lib::maintenance::run_process_if_requested();
    pos_tauri_lib::run()
}
