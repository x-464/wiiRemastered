#[cfg_attr(mobile, tauri::mobile_entry_point)]

use serde::Serialize;
use std::fs::File;
use std::io::Read;

#[derive(Serialize)]
struct IsoMetadata {
    id: String,
    title: String,
}

#[tauri::command]
fn get_iso_metadata(path: String) -> Result<IsoMetadata, String> {
    let mut file = File::open(&path).map_err(|e| e.to_string())?;

    let mut header = [0u8; 0x60];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;

    let id = String::from_utf8_lossy(&header[0..6])
        .trim_matches('\0')
        .trim()
        .to_string();

    let title = String::from_utf8_lossy(&header[0x20..0x20 + 64])
        .trim_matches('\0')
        .trim()
        .to_string();

    Ok(IsoMetadata { id, title })
}


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
    .invoke_handler(tauri::generate_handler![get_iso_metadata])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}