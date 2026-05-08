#[cfg_attr(mobile, tauri::mobile_entry_point)]

mod get_game_metadata;
mod get_bnr_with_wit;

pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .invoke_handler(tauri::generate_handler![get_game_metadata::get_iso_metadata, get_bnr_with_wit::extract_first_iso_banner])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}