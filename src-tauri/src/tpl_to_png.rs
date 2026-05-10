use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn png_output_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
        .join("generated_pngs");

    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

#[tauri::command]
pub fn tpl_to_png(app: AppHandle, tpl_path: String) -> Result<String, String> {
    let tpl = PathBuf::from(&tpl_path);

    if !tpl.is_file() {
        return Err("Provided tpl path is not a file".to_string());
    }

    let ext = tpl
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext != "tpl" {
        return Err("Provided file is not a .tpl".to_string());
    }

    let out_root = png_output_root(&app)?;
    let file_name = format!("{}.png", safe_stem(&tpl));
    let png_path = out_root.join(file_name);

    let output = if cfg!(target_os = "macos") {
        Command::new("arch")
            .arg("-x86_64")
            .arg("wimgt")
            .arg("copy")
            .arg(&tpl)
            .arg(&png_path)
            .arg("--overwrite")
            .output()
            .map_err(|e| format!("Failed to run wimgt via Rosetta: {}", e))?
    } else {
        Command::new("wimgt")
            .arg("copy")
            .arg(&tpl)
            .arg(&png_path)
            .arg("--overwrite")
            .output()
            .map_err(|e| format!("Failed to run wimgt: {}", e))?
    };

    if !output.status.success() {
        return Err(format!(
            "wimgt failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    if !png_path.exists() {
        return Err("wimgt reported success but PNG was not created".to_string());
    }

    Ok(png_path.display().to_string())
}