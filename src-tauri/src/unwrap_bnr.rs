use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct BinLocation {
    pub bin_path: String,
}

fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or_else(|| format!("Out of bounds u32 at {:#X}", offset))?
        .try_into()
        .map_err(|_| format!("Failed to read u32 at {:#X}", offset))?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u16_be(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or_else(|| format!("Out of bounds u16 at {:#X}", offset))?
        .try_into()
        .map_err(|_| format!("Failed to read u16 at {:#X}", offset))?;
    Ok(u16::from_be_bytes(bytes))
}

fn cstring_at(data: &[u8], offset: usize) -> Result<String, String> {
    let end = data[offset..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| format!("Missing null terminator for string at {:#X}", offset))?;
    String::from_utf8(data[offset..offset + end].to_vec())
        .map_err(|e| format!("Invalid UTF-8 string at {:#X}: {}", offset, e))
}

#[tauri::command]
pub fn unwrap_bnr(bnr_path: String) -> Result<Vec<BinLocation>, String> {
    let file = fs::read(&bnr_path).map_err(|e| e.to_string())?;

    if file.len() < 0x604 {
        return Err("File too small".into());
    }

    if &file[0x40..0x44] != b"IMET" {
        return Err("IMET magic not found at 0x40".into());
    }

    let u8_start = 0x600usize;
    let u8_magic = read_u32_be(&file, u8_start)?;
    if u8_magic != 0x55AA382D {
        return Err(format!(
            "Expected U8 magic at 0x600, found {:#X}",
            u8_magic
        ));
    }

    let rootnode_offset = read_u32_be(&file, u8_start + 0x04)? as usize;
    let header_size = read_u32_be(&file, u8_start + 0x08)? as usize;
    let _data_offset = read_u32_be(&file, u8_start + 0x0C)? as usize;

    let rootnode_abs = u8_start + rootnode_offset;
    let root_type = read_u16_be(&file, rootnode_abs)?;
    if root_type != 0x0100 {
        return Err(format!("Root node is not a directory: {:#X}", root_type));
    }

    let total_nodes = read_u32_be(&file, rootnode_abs + 8)? as usize;
    if total_nodes == 0 {
        return Err("U8 archive has zero nodes".into());
    }

    let string_table_abs = rootnode_abs + total_nodes * 12;
    let header_end_abs = u8_start + rootnode_offset + header_size;

    if string_table_abs > file.len() || header_end_abs > file.len() {
        return Err("Invalid U8 header bounds".into());
    }

    let out_root = {
        let bnr = PathBuf::from(&bnr_path);
        let parent = bnr.parent().unwrap_or(Path::new("."));
        parent.join("opening_bnr_extracted")
    };

    fs::create_dir_all(&out_root).map_err(|e| e.to_string())?;

    let mut locations = Vec::new();
    let mut dir_stack: Vec<(usize, PathBuf)> = vec![(total_nodes, out_root.clone())];

    for i in 1..total_nodes {
        while let Some((end_index, _)) = dir_stack.last() {
            if i >= *end_index {
                dir_stack.pop();
            } else {
                break;
            }
        }

        let node_abs = rootnode_abs + i * 12;
        let type_and_name = read_u32_be(&file, node_abs)?;
        let node_type = ((type_and_name >> 24) & 0xFF) as u8;
        let name_offset = (type_and_name & 0x00FF_FFFF) as usize;
        let data_offset = read_u32_be(&file, node_abs + 4)? as usize;
        let size = read_u32_be(&file, node_abs + 8)? as usize;

        let name = cstring_at(&file, string_table_abs + name_offset)?;
        let current_dir = dir_stack
            .last()
            .map(|(_, p)| p.clone())
            .ok_or_else(|| "Directory stack underflow".to_string())?;

        let out_path = current_dir.join(&name);

        match node_type {
            1 => {
                fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
                dir_stack.push((size, out_path));
            }
            0 => {
                let file_abs = u8_start + data_offset;
                let file_end = file_abs + size;

                let bytes = file
                    .get(file_abs..file_end)
                    .ok_or_else(|| format!("File node out of bounds: {}", name))?;

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }

                fs::write(&out_path, bytes).map_err(|e| e.to_string())?;

                if name.eq_ignore_ascii_case("banner.bin")
                    || name.eq_ignore_ascii_case("icon.bin")
                    || name.eq_ignore_ascii_case("sound.bin")
                {
                    locations.push(BinLocation {
                        bin_path: out_path.display().to_string(),
                    });
                }
            }
            _ => {
                return Err(format!("Unknown U8 node type {} for node {}", node_type, i));
            }
        }
    }

    if locations.is_empty() {
        return Err("No .bin files were extracted from opening.bnr".into());
    }

    Ok(locations)
}