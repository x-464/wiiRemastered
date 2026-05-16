use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ReadBytesExt};
use log::info;
use serde::Serialize;
use tauri::{AppHandle, Manager};

const BRLYT_MAGIC: &[u8; 4] = b"RLYT";

#[derive(Serialize, Default, Clone)]
pub struct TextureEntry {
    pub index: usize,
    pub name: String,
    pub png_candidate: String,
}

#[derive(Serialize, Default, Clone)]
pub struct TextureMapEntry {
    pub texture_id: u16,
    pub settings: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_candidate: Option<String>,
}

#[derive(Serialize, Default, Clone)]
pub struct MaterialEntry {
    pub index: usize,
    pub name: String,
    pub offset_in_mat1: usize,
    pub fore_color: [i16; 4],
    pub back_color: [i16; 4],
    pub color_reg_3: [i16; 4],
    pub tev_colors: [[u8; 4]; 4],
    pub flags: u32,
    pub has_material_color: bool,
    pub has_channel_control: bool,
    pub has_blend_mode: bool,
    pub has_alpha_compare: bool,
    pub tev_stage_count: usize,
    pub indirect_stage_count: usize,
    pub indirect_matrix_count: usize,
    pub has_tev_swap_table: bool,
    pub tex_coord_gen_count: usize,
    pub texture_srt_count: usize,
    pub texture_map_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texture_maps: Vec<TextureMapEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texture_indices: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texture_names: Vec<String>,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct Pane {
    pub name: String,
    pub user_data: String,
    pub visible: bool,
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub alpha: u8,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_index: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_candidate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_left_color: Option<[u8; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_right_color: Option<[u8; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_left_color: Option<[u8; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_right_color: Option<[u8; 4]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uv_sets: Vec<[f32; 4]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Pane>,
}

#[derive(Serialize, Default, Clone)]
pub struct PictureBinding {
    pub pane_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_index: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_candidate: Option<String>,
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub alpha: u8,
}

#[derive(Serialize, Default, Clone)]
pub struct Mat1DebugEntry {
    pub index: usize,
    pub raw_offset: usize,
    pub candidate_abs_a: usize,
    pub candidate_abs_b: usize,
    pub candidate_abs_c: usize,
    pub preview_a_hex: String,
    pub preview_b_hex: String,
    pub preview_c_hex: String,
    pub ascii_a: String,
    pub ascii_b: String,
    pub ascii_c: String,
    pub flags_a_3c: Option<u32>,
    pub flags_b_3c: Option<u32>,
    pub flags_c_3c: Option<u32>,
}

#[derive(Serialize, Default, Clone)]
pub struct Mat1Debug {
    pub chunk_size: usize,
    pub count: usize,
    pub offsets_table_start: usize,
    pub offsets: Vec<usize>,
    pub entries: Vec<Mat1DebugEntry>,
}

#[derive(Serialize, Default)]
pub struct Layout {
    pub version: u16,
    pub width: f32,
    pub height: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<TextureEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub root: Vec<Pane>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pictures: Vec<PictureBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mat1_debug: Option<Mat1Debug>,
}

fn read_uv_set(r: &mut Cursor<&[u8]>) -> Result<[f32; 4], String> {
    Ok([
        r.read_f32::<BigEndian>().map_err(|e| e.to_string())?,
        r.read_f32::<BigEndian>().map_err(|e| e.to_string())?,
        r.read_f32::<BigEndian>().map_err(|e| e.to_string())?,
        r.read_f32::<BigEndian>().map_err(|e| e.to_string())?,
    ])
}

fn read_fixed_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).trim().to_string()
}

fn read_i16x4_be(data: &[u8], off: usize) -> Option<[i16; 4]> {
    if off + 8 > data.len() {
        return None;
    }
    let mut r = Cursor::new(&data[off..off + 8]);
    Some([
        r.read_i16::<BigEndian>().ok()?,
        r.read_i16::<BigEndian>().ok()?,
        r.read_i16::<BigEndian>().ok()?,
        r.read_i16::<BigEndian>().ok()?,
    ])
}

fn read_u8x4(data: &[u8], off: usize) -> Option<[u8; 4]> {
    if off + 4 > data.len() {
        return None;
    }
    Some([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn read_u32_be_at(data: &[u8], off: usize) -> Option<u32> {
    if off + 4 > data.len() {
        return None;
    }
    (&data[off..off + 4]).read_u32::<BigEndian>().ok()
}

fn hex_preview(data: &[u8], start: usize, len: usize) -> String {
    if start >= data.len() {
        return String::new();
    }
    let end = (start + len).min(data.len());
    data[start..end]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii_preview(data: &[u8], start: usize, len: usize) -> String {
    if start >= data.len() {
        return String::new();
    }
    let end = (start + len).min(data.len());
    data[start..end]
        .iter()
        .map(|b| {
            let c = *b as char;
            if c.is_ascii_graphic() || c == ' ' { c } else { '.' }
        })
        .collect()
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layout")
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

fn texture_to_png_candidate(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".tpl") || lower.ends_with(".tex0") {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        format!("{stem}.png")
    } else {
        format!("{name}.png")
    }
}

fn read_c_string_at(chunk: &[u8], off: usize) -> Option<String> {
    if off >= chunk.len() {
        return None;
    }
    let mut end = off;
    while end < chunk.len() && chunk[end] != 0 {
        end += 1;
    }
    Some(String::from_utf8_lossy(&chunk[off..end]).to_string())
}

fn parse_txl1(chunk_data: &[u8]) -> Vec<TextureEntry> {
    let mut out = Vec::new();
    let mut cr = Cursor::new(chunk_data);

    if cr.seek(SeekFrom::Start(8)).is_err() {
        return out;
    }

    let count = match cr.read_u16::<BigEndian>() {
        Ok(v) => v as usize,
        Err(_) => return out,
    };

    let _pad = cr.read_u16::<BigEndian>().ok();
    let names_base = 0x0Cusize;

    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        let off = match cr.read_u32::<BigEndian>() {
            Ok(v) => v as usize,
            Err(_) => return out,
        };
        let _unk = cr.read_u32::<BigEndian>().ok();
        offsets.push(off);
    }

    for (index, off) in offsets.into_iter().enumerate() {
        let absolute = names_base + off;
        if let Some(name) = read_c_string_at(chunk_data, absolute) {
            if !name.is_empty() {
                out.push(TextureEntry {
                    index,
                    png_candidate: texture_to_png_candidate(&name),
                    name,
                });
            }
        }
    }

    out
}

fn parse_pane_common(r: &mut Cursor<&[u8]>, kind: &str) -> Result<Pane, String> {
    let flag = r.read_u8().map_err(|e| e.to_string())?;
    let _origin = r.read_u8().map_err(|e| e.to_string())?;
    let alpha = r.read_u8().map_err(|e| e.to_string())?;
    let _pad = r.read_u8().map_err(|e| e.to_string())?;

    let mut name_buf = [0u8; 16];
    r.read_exact(&mut name_buf).map_err(|e| e.to_string())?;
    let name = String::from_utf8_lossy(&name_buf)
        .trim_matches(char::from(0))
        .to_string();

    let mut user_buf = [0u8; 8];
    r.read_exact(&mut user_buf).map_err(|e| e.to_string())?;
    let user_data = String::from_utf8_lossy(&user_buf)
        .trim_matches(char::from(0))
        .to_string();

    let x = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let y = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let z = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let rot_x = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let rot_y = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let rot_z = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let scale_x = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let scale_y = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let width = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
    let height = r.read_f32::<BigEndian>().map_err(|e| e.to_string())?;

    Ok(Pane {
        name,
        user_data,
        visible: (flag & 1) != 0,
        width,
        height,
        x,
        y,
        z,
        scale_x,
        scale_y,
        rot_x,
        rot_y,
        rot_z,
        alpha,
        kind: kind.to_string(),
        ..Default::default()
    })
}

fn parse_pane_base(r: &mut Cursor<&[u8]>, tag: &[u8; 4]) -> Result<Pane, String> {
    let kind = std::str::from_utf8(tag).unwrap_or("unk");
    parse_pane_common(r, kind)
}

fn parse_pic1(chunk_data: &[u8]) -> Result<Pane, String> {
    if chunk_data.len() < 0x54 {
        return Err("pic1 too small".to_string());
    }

    let mut r = Cursor::new(chunk_data);
    r.seek(SeekFrom::Start(8)).map_err(|e| e.to_string())?;
    let mut pane = parse_pane_common(&mut r, "pic1")?;

    let mut m = Cursor::new(&chunk_data[0x4C..]);
    pane.top_left_color = Some([
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
    ]);
    pane.top_right_color = Some([
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
    ]);
    pane.bottom_left_color = Some([
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
    ]);
    pane.bottom_right_color = Some([
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
        m.read_u8().map_err(|e| e.to_string())?,
    ]);

    pane.material_index = Some(m.read_u16::<BigEndian>().map_err(|e| e.to_string())?);
    let uv_count = m.read_u8().map_err(|e| e.to_string())? as usize;
    let _pad = m.read_u8().map_err(|e| e.to_string())?;

    let mut uv_sets = Vec::with_capacity(uv_count);
    for _ in 0..uv_count {
        uv_sets.push(read_uv_set(&mut m)?);
    }
    pane.uv_sets = uv_sets;

    Ok(pane)
}

fn bind_materials_to_pictures(
    panes: &mut [Pane],
    materials: &[MaterialEntry],
    pictures: &mut Vec<PictureBinding>,
) {
    for pane in panes.iter_mut() {
        if pane.kind == "pic1" {
            if let Some(mat_idx) = pane.material_index {
                if let Some(mat) = materials.get(mat_idx as usize) {
                    pane.material_name = Some(mat.name.clone());
                    if let Some(tex) = mat.texture_maps.first() {
                        pane.texture_name = tex.texture_name.clone();
                        pane.png_candidate = tex.png_candidate.clone();
                    }
                }
            }

            pictures.push(PictureBinding {
                pane_name: pane.name.clone(),
                material_index: pane.material_index,
                material_name: pane.material_name.clone(),
                texture_name: pane.texture_name.clone(),
                png_candidate: pane.png_candidate.clone(),
                width: pane.width,
                height: pane.height,
                x: pane.x,
                y: pane.y,
                z: pane.z,
                scale_x: pane.scale_x,
                scale_y: pane.scale_y,
                alpha: pane.alpha,
            });
        }

        bind_materials_to_pictures(&mut pane.children, materials, pictures);
    }
}

fn parse_mat1(chunk_data: &[u8], textures: &[TextureEntry]) -> Vec<MaterialEntry> {
    let mut out = Vec::new();
    let mut cr = Cursor::new(chunk_data);

    if cr.seek(SeekFrom::Start(8)).is_err() {
        return out;
    }

    let count = match cr.read_u16::<BigEndian>() {
        Ok(v) => v as usize,
        Err(_) => return out,
    };

    let _pad = cr.read_u16::<BigEndian>().ok();

    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        if let Ok(o) = cr.read_u32::<BigEndian>() {
            offsets.push(o as usize);
        }
    }

    for (index, base) in offsets.into_iter().enumerate() {
        if base + 0x40 > chunk_data.len() {
            continue;
        }

        let name = read_fixed_string(&chunk_data[base..base + 20]);

        let fore_color = match read_i16x4_be(chunk_data, base + 0x14) {
            Some(v) => v,
            None => continue,
        };
        let back_color = match read_i16x4_be(chunk_data, base + 0x1C) {
            Some(v) => v,
            None => continue,
        };
        let color_reg_3 = match read_i16x4_be(chunk_data, base + 0x24) {
            Some(v) => v,
            None => continue,
        };

        let tev1 = match read_u8x4(chunk_data, base + 0x2C) {
            Some(v) => v,
            None => continue,
        };
        let tev2 = match read_u8x4(chunk_data, base + 0x30) {
            Some(v) => v,
            None => continue,
        };
        let tev3 = match read_u8x4(chunk_data, base + 0x34) {
            Some(v) => v,
            None => continue,
        };
        let tev4 = match read_u8x4(chunk_data, base + 0x38) {
            Some(v) => v,
            None => continue,
        };

        let flags = match (&chunk_data[base + 0x3C..base + 0x40]).read_u32::<BigEndian>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let has_material_color = ((flags >> 30) & 0x1) != 0;
        let has_channel_control = ((flags >> 28) & 0x1) != 0;
        let has_blend_mode = ((flags >> 27) & 0x1) != 0;
        let has_alpha_compare = ((flags >> 23) & 0x1) != 0;
        let tev_stage_count = ((flags >> 19) & 0xF) as usize;
        let indirect_stage_count = ((flags >> 17) & 0x3) as usize;
        let indirect_matrix_count = ((flags >> 15) & 0x3) as usize;
        let has_tev_swap_table = ((flags >> 14) & 0x1) != 0;
        let tex_coord_gen_count = ((flags >> 10) & 0xF) as usize;
        let texture_srt_count = ((flags >> 6) & 0xF) as usize;
        let texture_map_count = (flags & 0xF) as usize;

        let mut cursor = base + 0x40;
        let mut texture_maps = Vec::with_capacity(texture_map_count);

        for _ in 0..texture_map_count {
            if cursor + 4 > chunk_data.len() {
                break;
            }

            let texture_id = (&chunk_data[cursor..cursor + 2])
                .read_u16::<BigEndian>()
                .unwrap_or(0);
            let settings = (&chunk_data[cursor + 2..cursor + 4])
                .read_u16::<BigEndian>()
                .unwrap_or(0);

            let texture_name = textures.get(texture_id as usize).map(|t| t.name.clone());
            let png_candidate = texture_name
                .as_ref()
                .map(|name| texture_to_png_candidate(name));

            texture_maps.push(TextureMapEntry {
                texture_id,
                settings,
                texture_name,
                png_candidate,
            });

            cursor += 4;
        }

        out.push(MaterialEntry {
            index,
            name,
            offset_in_mat1: base,
            fore_color,
            back_color,
            color_reg_3,
            tev_colors: [tev1, tev2, tev3, tev4],
            flags,
            has_material_color,
            has_channel_control,
            has_blend_mode,
            has_alpha_compare,
            tev_stage_count,
            indirect_stage_count,
            indirect_matrix_count,
            has_tev_swap_table,
            tex_coord_gen_count,
            texture_srt_count,
            texture_map_count,
            texture_indices: texture_maps.iter().map(|m| m.texture_id as usize).collect(),
            texture_names: texture_maps
                .iter()
                .filter_map(|m| m.texture_name.clone())
                .collect(),
            texture_maps,
        });
    }

    out
}

fn parse_mat1_debug(chunk_data: &[u8]) -> Mat1Debug {
    let mut cr = Cursor::new(chunk_data);

    let mut debug = Mat1Debug {
        chunk_size: chunk_data.len(),
        ..Default::default()
    };

    if cr.seek(SeekFrom::Start(8)).is_err() {
        return debug;
    }

    let count = match cr.read_u16::<BigEndian>() {
        Ok(v) => v as usize,
        Err(_) => return debug,
    };
    debug.count = count;

    let _pad = cr.read_u16::<BigEndian>().ok();
    debug.offsets_table_start = 0x0C;

    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        if let Ok(o) = cr.read_u32::<BigEndian>() {
            offsets.push(o as usize);
        }
    }
    debug.offsets = offsets.clone();

    let base_a = 0usize;
    let base_b = 0x08 + count * 4;
    let base_c = 0x0C + count * 4;

    for (index, raw_offset) in offsets.into_iter().enumerate() {
        let a = base_a + raw_offset;
        let b = base_b + raw_offset;
        let c = base_c + raw_offset;

        debug.entries.push(Mat1DebugEntry {
            index,
            raw_offset,
            candidate_abs_a: a,
            candidate_abs_b: b,
            candidate_abs_c: c,
            preview_a_hex: hex_preview(chunk_data, a, 64),
            preview_b_hex: hex_preview(chunk_data, b, 64),
            preview_c_hex: hex_preview(chunk_data, c, 64),
            ascii_a: ascii_preview(chunk_data, a, 32),
            ascii_b: ascii_preview(chunk_data, b, 32),
            ascii_c: ascii_preview(chunk_data, c, 32),
            flags_a_3c: read_u32_be_at(chunk_data, a + 0x3C),
            flags_b_3c: read_u32_be_at(chunk_data, b + 0x3C),
            flags_c_3c: read_u32_be_at(chunk_data, c + 0x3C),
        });
    }

    debug
}

#[tauri::command]
pub fn convert_brlyt(app: AppHandle, brlyt_path: String, title: String) -> Result<String, String> {
    let path = PathBuf::from(&brlyt_path);

    let mut file = File::open(&path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    if buf.len() < 0x10 || &buf[0..4] != BRLYT_MAGIC {
        return Err("Not a BRLYT".to_string());
    }

    let mut r = Cursor::new(&buf);
    r.seek(SeekFrom::Start(12)).map_err(|e| e.to_string())?;
    let header_size = r.read_u16::<BigEndian>().map_err(|e| e.to_string())?;
    let num_chunks = r.read_u16::<BigEndian>().map_err(|e| e.to_string())?;

    let mut layout = Layout::default();
    let mut current_pos = header_size as usize;
    let mut stack: Vec<Pane> = Vec::new();
    let mut final_roots: Vec<Pane> = Vec::new();
    let mut mat1_chunk: Option<Vec<u8>> = None;

    info!("BRLYT header_size={}, num_chunks={}", header_size, num_chunks);

    for _ in 0..num_chunks {
        if current_pos + 8 > buf.len() {
            break;
        }

        let tag = &buf[current_pos..current_pos + 4];
        let size = (&buf[current_pos + 4..current_pos + 8])
            .read_u32::<BigEndian>()
            .map_err(|e| e.to_string())? as usize;

        if size < 8 || current_pos + size > buf.len() {
            break;
        }

        let tag_str = std::str::from_utf8(tag).unwrap_or("????");
        info!("chunk @ {:#X}: {} size={:#X}", current_pos, tag_str, size);

        let chunk_data = &buf[current_pos..current_pos + size];
        let mut cr = Cursor::new(chunk_data);
        cr.seek(SeekFrom::Start(8)).map_err(|e| e.to_string())?;

        match tag {
            b"lyt1" => {
                let _centered = cr.read_u8().map_err(|e| e.to_string())?;
                let _pad = cr.read_u8().map_err(|e| e.to_string())?;
                let _pad2 = cr.read_u16::<BigEndian>().map_err(|e| e.to_string())?;
                layout.width = cr.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
                layout.height = cr.read_f32::<BigEndian>().map_err(|e| e.to_string())?;
            }
            b"txl1" => {
                layout.textures = parse_txl1(chunk_data);
            }
            b"mat1" => {
                mat1_chunk = Some(chunk_data.to_vec());
            }
            b"pan1" | b"txt1" | b"wnd1" => {
                let mut tag_arr = [0u8; 4];
                tag_arr.copy_from_slice(tag);
                let pane = parse_pane_base(&mut cr, &tag_arr)?;
                stack.push(pane);
            }
            b"pic1" => {
                let pane = parse_pic1(chunk_data)?;
                stack.push(pane);
            }
            b"pas1" => {}
            b"pae1" => {
                if let Some(child) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(child);
                    } else {
                        final_roots.push(child);
                    }
                }
            }
            _ => {}
        }

        current_pos += size;
    }

    while let Some(p) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(p);
        } else {
            final_roots.push(p);
        }
    }

    if let Some(mat1) = mat1_chunk {
        layout.mat1_debug = Some(parse_mat1_debug(&mat1));
        layout.materials = parse_mat1(&mat1, &layout.textures);
    }

    layout.root = final_roots;
    bind_materials_to_pictures(&mut layout.root, &layout.materials, &mut layout.pictures);

    let out_root = json_output_root(&app, &title)?;
    let out_path = out_root.join(format!("{}.json", safe_stem(&path)));

    let json = serde_json::to_string_pretty(&layout).map_err(|e| e.to_string())?;
    fs::write(&out_path, json).map_err(|e| e.to_string())?;

    info!(
        "parsed: textures={}, materials={}, roots={}, pictures={}",
        layout.textures.len(),
        layout.materials.len(),
        layout.root.len(),
        layout.pictures.len()
    );
    info!("BRLYT converted: {:?} -> {:?}", path, out_path);

    Ok(out_path.display().to_string())
}