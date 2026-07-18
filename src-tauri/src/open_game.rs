use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

/// Guards against launching two Dolphins at once (double-clicks, duplicate
/// events). Two instances fight over the NAND ("IOS_FS: failed to rename
/// temporary FST file"), stack error dialogs behind the render window, and
/// make closing look like the game relaunched (window #2 was behind #1).
static GAME_RUNNING: AtomicBool = AtomicBool::new(false);

/// Dolphin's own docs recommend launching the bundle's inner binary on
/// macOS (Dolphin.app/Contents/MacOS/Dolphin -b -e <game>) rather than
/// `open -a` — it stays our child process so waiting for game-closed works.
/// Prefers the binary named like the bundle, falls back to the first file.
fn resolve_launchable(path: String) -> PathBuf {
    let p = PathBuf::from(&path);

    if cfg!(target_os = "macos") && p.extension().and_then(|e| e.to_str()) == Some("app") {
        let macos_dir = p.join("Contents").join("MacOS");
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();

        let preferred = macos_dir.join(stem);
        if preferred.is_file() {
            return preferred;
        }
        if let Ok(entries) = std::fs::read_dir(&macos_dir) {
            if let Some(first) = entries.flatten().map(|e| e.path()).find(|c| c.is_file()) {
                return first;
            }
        }
    }

    p
}

#[tauri::command]
pub fn open_game(app: AppHandle, game_path: String, dolphin_path: String) -> Result<(), String> {
    if GAME_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("a game is already running".to_string());
    }

    std::thread::spawn(move || {
        let mut child = match Command::new(resolve_launchable(dolphin_path))
            .arg("-b")
            .arg("-e")
            .arg(game_path)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                GAME_RUNNING.store(false, Ordering::SeqCst);
                let _ = app.emit("game-error", e.to_string());
                return;
            }
        };

        let _ = child.wait();
        GAME_RUNNING.store(false, Ordering::SeqCst);
        let _ = app.emit("game-closed", ());
    });

    Ok(())
}