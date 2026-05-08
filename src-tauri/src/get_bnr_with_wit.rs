use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize)]
pub struct WitExtractResult {
    pub iso_path: String,
    pub output_dir: String,
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

fn first_iso_in_dir(dir: &Path) -> Result<PathBuf, String> {
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("iso") {
                    return Ok(path);
                }
            }
        }
    }

    Err("No .iso file found in selected folder".to_string())
}

#[tauri::command]
pub fn extract_first_iso_banner(folder_path: String) -> Result<WitExtractResult, String> {
    let folder = PathBuf::from(&folder_path);
    if !folder.is_dir() {
        return Err("Selected path is not a folder".to_string());
    }

    let iso_path = first_iso_in_dir(&folder)?;
    let extract_dir = folder.join("wit_extract");

    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir).map_err(|e| e.to_string())?;
    }

    let output = if cfg!(target_os = "macos") {
        Command::new("arch")
            .arg("-x86_64")
            .arg("wit")
            .arg("extract")
            .arg(&iso_path)
            .arg("--psel=data")
            .arg(format!("--dest={}", extract_dir.display()))
            .output()
            .map_err(|e| format!("Failed to run wit via Rosetta: {}", e))?
    } else if cfg!(target_os = "windows") {
        Command::new("wit")
            .arg("extract")
            .arg(&iso_path)
            .arg("--psel=data")
            .arg(format!("--dest={}", extract_dir.display()))
            .output()
            .map_err(|e| format!("Failed to run wit on Windows: {}", e))?
    } else {
        Command::new("wit")
            .arg("extract")
            .arg(&iso_path)
            .arg("--psel=data")
            .arg(format!("--dest={}", extract_dir.display()))
            .output()
            .map_err(|e| format!("Failed to run wit: {}", e))?
    };

    Ok(WitExtractResult {
        iso_path: iso_path.display().to_string(),
        output_dir: extract_dir.display().to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status.code().unwrap_or(-1),
    })
}