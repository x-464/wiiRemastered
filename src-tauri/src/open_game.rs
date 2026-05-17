use std::process::Command;

#[tauri::command]
pub fn open_game(game_path: String, dolphin_path: String) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("/Applications/Dolphin.app/Contents/MacOS/Dolphin")
    } else {
        Command::new(dolphin_path)
    };

    cmd.arg("-b").arg("-e").arg(game_path);
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}