use byteorder::{BigEndian, ByteOrder};
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

const TPL_MAGIC: u32 = 0x0020AF30;
const GX_TF_I4: u32 = 0;
const GX_TF_I8: u32 = 1;

/// Kill switch for AI upscaling of decoded textures. Needs the free
/// realesrgan-ncnn-vulkan CLI on PATH (models folder alongside the exe);
/// if it's missing or fails, conversion silently falls back to 1x output,
/// so leaving this on without the tool installed is harmless.
const AI_UPSCALE_ENABLED: bool = true;
/// 4x so the browser downsamples (crisp) instead of upsampling (fuzzy) on
/// HD/4K displays. realesrgan-x4plus-anime is the line-art model — much
/// sharper text/edges than realesr-animevideov3 (which supports -s 2/3/4
/// and is faster, if softness is ever preferred).
const AI_UPSCALE_SCALE: u32 = 4;
const AI_UPSCALE_MODEL: &str = "realesrgan-x4plus-anime";
const AI_UPSCALE_BIN: &str = "realesrgan-ncnn-vulkan";
/// Textures thinner than this in either dimension are gradient strips or
/// slivers (e.g. WSR's 2x96 background): too little 2D structure for the
/// model, which hallucinates noise on them — and the browser's bilinear
/// stretch already renders them perfectly, so they stay 1x.
const AI_UPSCALE_MIN_DIM: u32 = 8;

/// Texture format of the first image in a TPL, if the file parses.
fn tpl_format(tpl: &[u8]) -> Option<u32> {
    if tpl.len() < 0x0C || BigEndian::read_u32(&tpl[0..4]) != TPL_MAGIC {
        return None;
    }
    let table = BigEndian::read_u32(&tpl[8..12]) as usize;
    let img_hdr = BigEndian::read_u32(tpl.get(table..table + 4)?) as usize;
    let fmt = BigEndian::read_u32(tpl.get(img_hdr + 4..img_hdr + 8)?);
    Some(fmt)
}

/// GX intensity textures (I4/I8) use the single channel as BOTH color and
/// alpha (R=G=B=A=I), but wimgt decodes them to opaque grayscale PNGs, losing
/// the implicit alpha (white clouds end up on black). Rewrite the PNG as RGBA
/// with A=I, which reproduces exactly what GX samples.
fn intensity_to_alpha(png_path: &Path) -> Result<(), String> {
    let file = File::open(png_path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    buf.truncate(info.buffer_size());

    let mut out = Vec::with_capacity((w * h * 4) as usize);
    match info.color_type {
        png::ColorType::Grayscale => {
            for &v in &buf {
                out.extend_from_slice(&[v, v, v, v]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in buf.chunks_exact(2) {
                let a = (px[0] as u16 * px[1] as u16 / 255) as u8;
                out.extend_from_slice(&[px[0], px[0], px[0], a]);
            }
        }
        png::ColorType::Rgb => {
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], px[0]]);
            }
        }
        png::ColorType::Rgba => {
            for px in buf.chunks_exact(4) {
                out.extend_from_slice(&[px[0], px[1], px[2], px[0]]);
            }
        }
        _ => return Ok(()), // palette output would be unexpected here; leave as-is
    }
    drop(reader);

    let file = File::create(png_path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&out)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Locate the upscaler. The project ships an `upscaler/` folder with a
/// shared `models/` dir and per-platform binaries (`windows/`, `macos/`);
/// it's searched for next to every ancestor of the app executable and the
/// working directory, with PATH as a final fallback. Returns (exe, models).
fn find_upscaler() -> Option<(PathBuf, PathBuf)> {
    let (platform_dir, exe_name) = if cfg!(target_os = "windows") {
        ("windows", "realesrgan-ncnn-vulkan.exe")
    } else {
        ("macos", "realesrgan-ncnn-vulkan")
    };

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        roots.extend(cur.ancestors().skip(1).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(Path::to_path_buf));
    }

    for root in roots {
        let dir = root.join("upscaler");
        let exe = dir.join(platform_dir).join(exe_name);
        if exe.is_file() {
            return Some((exe, dir.join("models")));
        }
    }

    // fall back to PATH
    let finder = if cfg!(target_os = "windows") { "where" } else { "which" };
    let output = Command::new(finder).arg(AI_UPSCALE_BIN).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let exe = PathBuf::from(stdout.lines().next()?.trim());
    let models = exe.parent()?.join("models");
    Some((exe, models))
}

/// Width/height of a PNG from its IHDR, without decoding the image.
fn png_dimensions(png_path: &Path) -> Option<(u32, u32)> {
    let decoder = png::Decoder::new(File::open(png_path).ok()?);
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}

/// Upscale the PNG in place. Returns Ok(false) when the tool isn't
/// available, so callers can treat that as a soft no-op.
fn ai_upscale(png_path: &Path) -> Result<bool, String> {
    let Some((exe, models)) = find_upscaler() else {
        return Ok(false); // tool not installed -> keep 1x png
    };

    let tmp_path = png_path.with_extension("upscaled.png");

    let output = Command::new(&exe)
        .arg("-i")
        .arg(png_path)
        .arg("-o")
        .arg(&tmp_path)
        .arg("-n")
        .arg(AI_UPSCALE_MODEL)
        .arg("-s")
        .arg(AI_UPSCALE_SCALE.to_string())
        .arg("-m")
        .arg(&models)
        .output()
        .map_err(|e| format!("failed to run {:?}: {}", exe, e))?;

    if !output.status.success() || !tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "{} failed: {}",
            AI_UPSCALE_BIN,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    fs::rename(&tmp_path, png_path).map_err(|e| e.to_string())?;
    Ok(true)
}

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

fn png_output_root(app: &AppHandle, title: String) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
        .join("wiiMainMenu")
        .join("cached_pngs")
        .join(title);

    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

#[tauri::command]
pub fn tpl_to_png(app: AppHandle, tpl_path: String, title: String) -> Result<String, String> {
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

    let out_root = png_output_root(&app, title)?;
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

    // intensity formats need their implicit alpha restored (see intensity_to_alpha)
    let tpl_bytes = fs::read(&tpl).map_err(|e| e.to_string())?;
    if let Some(fmt @ (GX_TF_I4 | GX_TF_I8)) = tpl_format(&tpl_bytes) {
        intensity_to_alpha(&png_path)
            .map_err(|e| format!("I{} alpha restore failed: {}", if fmt == GX_TF_I4 { 4 } else { 8 }, e))?;
    }

    // optional AI upscale, after alpha restore so the model sees proper RGBA
    if AI_UPSCALE_ENABLED {
        static NOT_FOUND_WARNED: std::sync::Once = std::sync::Once::new();
        let too_small = png_dimensions(&png_path)
            .is_some_and(|(w, h)| w.min(h) < AI_UPSCALE_MIN_DIM);
        if too_small {
            log::info!("upscale skipped (sliver texture, 1x is lossless): {:?}", png_path);
        } else {
            match ai_upscale(&png_path) {
                Ok(true) => log::info!("upscaled {}x: {:?}", AI_UPSCALE_SCALE, png_path),
                Ok(false) => NOT_FOUND_WARNED.call_once(|| {
                    log::warn!(
                        "AI upscaling is enabled but {} was not found (project folder or PATH); textures stay 1x",
                        AI_UPSCALE_BIN
                    );
                }),
                Err(e) => log::warn!("upscale skipped for {:?}: {}", png_path, e),
            }
        }
    }

    Ok(png_path.display().to_string())
}