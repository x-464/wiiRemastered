use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter};

/// A macOS .app bundle is a directory, not an executable. Resolve it to the
/// real binary in Contents/MacOS so we spawn (and can wait on) the actual
/// process — `open -a` would return immediately and break game-closed
/// detection. Prefers the binary named like the bundle, falls back to the
/// first file found. On other platforms the path passes through untouched.
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
    std::thread::spawn(move || {
        let mut child = match Command::new(resolve_launchable(dolphin_path))
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