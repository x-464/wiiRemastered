use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown_iso")
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

fn app_extract_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
        .join("wiiMainMenu")
        .join("wit_extract");

    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

fn run_wit_extract(iso_path: &Path, extract_dir: &Path) -> Result<(), String> {
    if extract_dir.exists() {
        fs::remove_dir_all(extract_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(extract_dir).map_err(|e| e.to_string())?;

    let output = if cfg!(target_os = "macos") {
        Command::new("arch")
            .arg("-x86_64")
            .arg("wit")
            .arg("extract")
            .arg(iso_path)
            .arg("--files")
            .arg("+opening.bnr")
            .arg("--flat")
            .arg("--dest")
            .arg(extract_dir)  // Use parent dir
            .output()
            .map_err(|e| format!("Failed to run wit via Rosetta: {}", e))?
    } else {
        Command::new("wit")
            .arg("extract")
            .arg(iso_path)
            .arg("--files")
            .arg("+opening.bnr")
            .arg("--flat")
            .arg("--dest")
            .arg(extract_dir)
            .output()
            .map_err(|e| format!("Failed to run wit: {}", e))?
    };

    if !output.status.success() {
        return Err(format!(
            "wit failed for {}: {}",
            iso_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn find_opening_bnr(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;

    for entry in entries {
        let entry = entry.ok()?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(found) = find_opening_bnr(&path) {
                return Some(found);
            }
        } else if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.eq_ignore_ascii_case("opening.bnr") {
                    return Some(path);
                }
            }
        }
    }

    None
}

#[tauri::command]
pub fn unwrap_iso(app: AppHandle, iso_path: String) -> Result<String, String> {
    let iso = PathBuf::from(&iso_path);

    if !iso.is_file() {
        return Err("Selected path is not an ISO file".to_string());
    }

    let ext = iso
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext != "iso" {
        return Err("Selected file does not have .iso extension".to_string());
    }

    let root = app_extract_root(&app)?;
    let iso_name = safe_stem(&iso);
    let extract_dir = root.join(&iso_name);

    run_wit_extract(&iso, &extract_dir)?;

    let bnr_path = find_opening_bnr(&extract_dir)
        .ok_or_else(|| format!("opening.bnr not found under {}", extract_dir.display()))?;

    Ok(bnr_path.display().to_string())
}