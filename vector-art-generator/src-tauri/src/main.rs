#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(evo_art_lib::commands::AppState {
            population: std::sync::Mutex::new(Default::default()),
        })
        .invoke_handler(tauri::generate_handler![
            evo_art_lib::commands::get_current_generation,
            evo_art_lib::commands::breed_next_generation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
