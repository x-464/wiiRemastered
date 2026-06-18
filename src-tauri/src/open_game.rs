use std::process::Command;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn open_game(app: AppHandle, game_path: String, dolphin_path: String) -> Result<(), String> {
    std::thread::spawn(move || {
        let mut child = match Command::new(dolphin_path)
            .arg("-b")
            .arg("-e")
            .arg(game_path)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                let _ = app.emit("game-error", e.to_string());
                return;
            }
        };

        let _ = child.wait();
        let _ = app.emit("game-closed", ());
    });

    Ok(())
}