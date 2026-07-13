use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use log::info;
use serde::Serialize;
use tauri::{AppHandle, Manager};

const BRLAN_MAGIC: &[u8; 4] = b"RLAN";

// keyframe data types
const DATA_TYPE_STEP: u8 = 1;
const DATA_TYPE_HERMITE: u8 = 2;

#[derive(Serialize, Clone)]
pub struct Keyframe {
    pub frame: f32,
    pub value: f32,
    pub slope: f32,
}

#[derive(Serialize)]
pub struct AnimTarget {
    /// which sub-slot this targets (e.g. texture map index for RLTS/RLTP)
    pub index: u8,
    /// raw target id within the tag
    pub target: u8,
    /// 1 = step, 2 = hermite
    pub data_type: u8,
    /// friendly name the frontend keys off ("trans_x", "alpha", "visible", ...)
    pub property: String,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Serialize)]
pub struct AnimTag {
    /// tag magic: RLPA, RLVC, RLMC, RLVI, RLTS, RLTP, RLIM
    pub kind: String,
    pub targets: Vec<AnimTarget>,
}

#[derive(Serialize)]
pub struct AnimEntry {
    /// pane name (is_material == false) or material name (is_material == true)
    pub name: String,
    pub is_material: bool,
    pub tags: Vec<AnimTag>,
}

#[derive(Serialize)]
pub struct BannerAnimation {
    pub frame_count: u16,
    #[serde(rename = "loop")]
    pub loops: bool,
    /// texture names referenced by RLTP (texture pattern) keyframe values
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<String>,
    pub entries: Vec<AnimEntry>,
}

fn u8_at(data: &[u8], off: usize) -> Result<u8, String> {
    data.get(off)
        .copied()
        .ok_or_else(|| format!("Out of bounds u8 at {:#X}", off))
}

fn u16_at(data: &[u8], off: usize) -> Result<u16, String> {
    data.get(off..off + 2)
        .map(BigEndian::read_u16)
        .ok_or_else(|| format!("Out of bounds u16 at {:#X}", off))
}

fn u32_at(data: &[u8], off: usize) -> Result<u32, String> {
    data.get(off..off + 4)
        .map(BigEndian::read_u32)
        .ok_or_else(|| format!("Out of bounds u32 at {:#X}", off))
}

fn f32_at(data: &[u8], off: usize) -> Result<f32, String> {
    data.get(off..off + 4)
        .map(BigEndian::read_f32)
        .ok_or_else(|| format!("Out of bounds f32 at {:#X}", off))
}

fn fixed_string_at(data: &[u8], off: usize, len: usize) -> Result<String, String> {
    let bytes = data
        .get(off..off + len)
        .ok_or_else(|| format!("Out of bounds string at {:#X}", off))?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
    Ok(String::from_utf8_lossy(&bytes[..end]).trim().to_string())
}

fn c_string_at(data: &[u8], off: usize) -> Option<String> {
    if off >= data.len() {
        return None;
    }
    let mut end = off;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    Some(String::from_utf8_lossy(&data[off..end]).to_string())
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("anim")
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

fn json_output_root(app: &AppHandle, title: &str) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("generated_pngs")
        .join(title)
        .join("positioning_json");

    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

fn property_name(kind: &str, target: u8) -> String {
    let known = match (kind, target) {
        ("RLPA", 0) => "trans_x",
        ("RLPA", 1) => "trans_y",
        ("RLPA", 2) => "trans_z",
        ("RLPA", 3) => "rot_x",
        ("RLPA", 4) => "rot_y",
        ("RLPA", 5) => "rot_z",
        ("RLPA", 6) => "scale_x",
        ("RLPA", 7) => "scale_y",
        ("RLPA", 8) => "size_w",
        ("RLPA", 9) => "size_h",
        ("RLVI", 0) => "visible",
        ("RLVC", 16) => "alpha",
        ("RLVC", t @ 0..=15) => {
            let corner = ["lt", "rt", "lb", "rb"][(t / 4) as usize];
            let channel = ["r", "g", "b", "a"][(t % 4) as usize];
            return format!("vtx_{}_{}", corner, channel);
        }
        ("RLMC", 0) => "mat_r",
        ("RLMC", 1) => "mat_g",
        ("RLMC", 2) => "mat_b",
        ("RLMC", 3) => "mat_a",
        ("RLTS", 0) => "uv_trans_u",
        ("RLTS", 1) => "uv_trans_v",
        ("RLTS", 2) => "uv_rot",
        ("RLTS", 3) => "uv_scale_u",
        ("RLTS", 4) => "uv_scale_v",
        ("RLTP", 0) => "tex_pattern",
        _ => return format!("{}_{}", kind.to_ascii_lowercase(), target),
    };
    known.to_string()
}

fn parse_target(chunk: &[u8], base: usize, kind: &str) -> Result<AnimTarget, String> {
    let index = u8_at(chunk, base)?;
    let target = u8_at(chunk, base + 1)?;
    let data_type = u8_at(chunk, base + 2)?;
    let num_keyframes = u16_at(chunk, base + 4)? as usize;
    let kf_offset = u32_at(chunk, base + 8)? as usize;

    let mut keyframes = Vec::with_capacity(num_keyframes);
    let mut pos = base + kf_offset;

    for _ in 0..num_keyframes {
        match data_type {
            DATA_TYPE_HERMITE => {
                keyframes.push(Keyframe {
                    frame: f32_at(chunk, pos)?,
                    value: f32_at(chunk, pos + 4)?,
                    slope: f32_at(chunk, pos + 8)?,
                });
                pos += 12;
            }
            DATA_TYPE_STEP => {
                keyframes.push(Keyframe {
                    frame: f32_at(chunk, pos)?,
                    value: u16_at(chunk, pos + 4)? as f32,
                    slope: 0.0,
                });
                pos += 8;
            }
            other => {
                return Err(format!("Unknown keyframe data type {}", other));
            }
        }
    }

    Ok(AnimTarget {
        index,
        target,
        data_type,
        property: property_name(kind, target),
        keyframes,
    })
}

fn parse_pai1(chunk: &[u8]) -> Result<BannerAnimation, String> {
    let frame_count = u16_at(chunk, 0x08)?;
    let flags = u8_at(chunk, 0x0A)?;
    let num_timgs = u16_at(chunk, 0x0C)? as usize;
    let num_entries = u16_at(chunk, 0x0E)? as usize;
    let entry_table = u32_at(chunk, 0x10)? as usize;

    let mut textures = Vec::with_capacity(num_timgs);
    let timg_table = 0x14usize;
    for i in 0..num_timgs {
        let off = u32_at(chunk, timg_table + i * 4)? as usize;
        if let Some(name) = c_string_at(chunk, timg_table + off) {
            textures.push(name);
        }
    }

    let mut entries = Vec::with_capacity(num_entries);

    for i in 0..num_entries {
        let entry_base = u32_at(chunk, entry_table + i * 4)? as usize;
        let name = fixed_string_at(chunk, entry_base, 20)?;
        let num_tags = u8_at(chunk, entry_base + 20)? as usize;
        let is_material = u8_at(chunk, entry_base + 21)? != 0;

        let mut tags = Vec::with_capacity(num_tags);

        for t in 0..num_tags {
            let tag_base = entry_base + u32_at(chunk, entry_base + 24 + t * 4)? as usize;
            let kind = fixed_string_at(chunk, tag_base, 4)?;
            let num_targets = u8_at(chunk, tag_base + 4)? as usize;

            let mut targets = Vec::with_capacity(num_targets);
            for g in 0..num_targets {
                let target_base = tag_base + u32_at(chunk, tag_base + 8 + g * 4)? as usize;
                targets.push(parse_target(chunk, target_base, &kind)?);
            }

            tags.push(AnimTag { kind, targets });
        }

        entries.push(AnimEntry {
            name,
            is_material,
            tags,
        });
    }

    Ok(BannerAnimation {
        frame_count,
        loops: (flags & 1) != 0,
        textures,
        entries,
    })
}

#[tauri::command]
pub fn convert_brlan(app: AppHandle, brlan_path: String, title: String) -> Result<String, String> {
    let path = PathBuf::from(&brlan_path);

    let mut file = File::open(&path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    if buf.len() < 0x10 || &buf[0..4] != BRLAN_MAGIC {
        return Err("Not a BRLAN".to_string());
    }

    let header_size = u16_at(&buf, 0x0C)? as usize;
    let num_chunks = u16_at(&buf, 0x0E)? as usize;

    let mut animation: Option<BannerAnimation> = None;
    let mut current_pos = header_size;

    for _ in 0..num_chunks {
        if current_pos + 8 > buf.len() {
            break;
        }

        let tag = &buf[current_pos..current_pos + 4];
        let size = u32_at(&buf, current_pos + 4)? as usize;

        if size < 8 || current_pos + size > buf.len() {
            break;
        }

        if tag == b"pai1" {
            animation = Some(parse_pai1(&buf[current_pos..current_pos + size])?);
        }

        current_pos += size;
    }

    let animation = animation.ok_or_else(|| "BRLAN has no pai1 chunk".to_string())?;

    let out_root = json_output_root(&app, &title)?;
    let out_path = out_root.join(format!("{}_anim.json", safe_stem(&path)));

    let json = serde_json::to_string_pretty(&animation).map_err(|e| e.to_string())?;
    fs::write(&out_path, json).map_err(|e| e.to_string())?;

    info!(
        "BRLAN converted: {:?} -> {:?} (frames={}, loop={}, entries={})",
        path,
        out_path,
        animation.frame_count,
        animation.loops,
        animation.entries.len()
    );

    Ok(out_path.display().to_string())
}
