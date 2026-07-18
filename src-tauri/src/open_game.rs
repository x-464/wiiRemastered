use std::process::Command;
use tauri::{AppHandle, Emitter};

/// macOS .app bundles must go through LaunchServices (`open`) — executing
/// the inner Contents/MacOS binary directly runs Dolphin outside a proper
/// app session, which caused spurious "crashed"/second-instance dialogs and
/// ghost relaunches. `-W` blocks until Dolphin quits so game-closed
/// detection still works, `-n` forces a fresh instance, and `--args`
/// forwards the batch/exec flags. Windows (and raw binaries) spawn directly.
fn build_launch_command(dolphin_path: &str, game_path: &str) -> Command {
    if cfg!(target_os = "macos") && dolphin_path.ends_with(".app") {
        let mut cmd = Command::new("open");
        cmd.arg("-W")
            .arg("-n")
            .arg("-a")
            .arg(dolphin_path)
            .arg("--args")
            .arg("-b")
            .arg("-e")
            .arg(game_path);
        return cmd;
    }

    let mut cmd = Command::new(dolphin_path);
    cmd.arg("-b").arg("-e").arg(game_path);
    cmd
}

#[tauri::command]
pub fn open_game(app: AppHandle, game_path: String, dolphin_path: String) -> Result<(), String> {
    std::thread::spawn(move || {
        let mut child = match build_launch_command(&dolphin_path, &game_path).spawn()
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