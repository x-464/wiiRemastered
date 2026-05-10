use std::fs::File;
use std::io::Read;

#[tauri::command]
pub fn get_id(path: String) -> Result<String, String> {
    let mut file = File::open(&path).map_err(|e| e.to_string())?;

    let mut header = [0u8; 0x60];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;

    let id = String::from_utf8_lossy(&header[0..6])
        .trim_matches('\0')
        .trim()
        .to_string();

    Ok(id)
}