#[cfg_attr(mobile, tauri::mobile_entry_point)]

mod get_game_metadata;
mod get_title_from_id;
mod unwrap_iso;
mod unwrap_bnr;
mod unwrap_bin;
mod tpl_to_png;
mod brlyt_to_json;

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
    .invoke_handler(tauri::generate_handler![
      get_game_metadata::get_id, 
      get_title_from_id::get_title_from_id,
      unwrap_iso::unwrap_iso, 
      unwrap_bnr::unwrap_bnr,
      unwrap_bin::unwrap_bin,
      tpl_to_png::tpl_to_png,
      brlyt_to_json::convert_brlyt
      ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}