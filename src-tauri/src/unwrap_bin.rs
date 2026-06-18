use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct ImageSource {
    pub source_path: String,
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
    let tail = data
        .get(offset..)
        .ok_or_else(|| format!("String offset out of bounds at {:#X}", offset))?;

    let end = tail
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| format!("Missing null terminator for string at {:#X}", offset))?;

    String::from_utf8(tail[..end].to_vec())
        .map_err(|e| format!("Invalid UTF-8 string at {:#X}: {}", offset, e))
}

fn strip_imd5(data: &[u8]) -> Result<&[u8], String> {
    if data.len() < 0x20 {
        return Err("File too small for IMD5 header".into());
    }

    if &data[0..4] != b"IMD5" {
        return Err("IMD5 header not found".into());
    }

    let payload_size = read_u32_be(data, 0x04)? as usize;
    let payload_start = 0x20usize;
    let payload_end = payload_start + payload_size;

    let payload = data
        .get(payload_start..payload_end)
        .ok_or_else(|| "IMD5 payload extends beyond file size".to_string())?;

    Ok(payload)
}

fn decompress_lz77(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 {
        return Err("LZ77 data too small".into());
    }

    if data[0] != 0x10 {
        return Err(format!("Expected LZ77 type 0x10, found {:#X}", data[0]));
    }

    let out_size =
        (data[1] as usize) |
        ((data[2] as usize) << 8) |
        ((data[3] as usize) << 16);

    let mut out = Vec::with_capacity(out_size);
    let mut pos = 4usize;

    while out.len() < out_size {
        let flags = *data
            .get(pos)
            .ok_or_else(|| "Unexpected EOF reading LZ77 flags".to_string())?;
        pos += 1;

        for bit in 0..8 {
            if out.len() >= out_size {
                break;
            }

            let compressed = (flags & (0x80 >> bit)) != 0;

            if !compressed {
                let byte = *data
                    .get(pos)
                    .ok_or_else(|| "Unexpected EOF reading LZ77 literal".to_string())?;
                pos += 1;
                out.push(byte);
            } else {
                let b1 = *data
                    .get(pos)
                    .ok_or_else(|| "Unexpected EOF reading LZ77 backref byte 1".to_string())?;
                let b2 = *data
                    .get(pos + 1)
                    .ok_or_else(|| "Unexpected EOF reading LZ77 backref byte 2".to_string())?;
                pos += 2;

                let length = ((b1 >> 4) as usize) + 3;
                let disp = ((((b1 & 0x0F) as usize) << 8) | b2 as usize) + 1;

                if disp > out.len() {
                    return Err("Invalid LZ77 back-reference".into());
                }

                let copy_start = out.len() - disp;
                for i in 0..length {
                    let value = out[copy_start + i];
                    out.push(value);
                    if out.len() >= out_size {
                        break;
                    }
                }
            }
        }
    }

    Ok(out)
}

fn extract_u8_to_dir(u8_data: &[u8], output_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if u8_data.len() < 0x20 {
        return Err("U8 data too small".into());
    }

    let magic = read_u32_be(u8_data, 0x00)?;
    if magic != 0x55AA382D {
        return Err(format!("U8 magic not found, got {:#X}", magic));
    }

    let rootnode_offset = read_u32_be(u8_data, 0x04)? as usize;
    let header_size = read_u32_be(u8_data, 0x08)? as usize;
    let data_offset = read_u32_be(u8_data, 0x0C)? as usize;

    let rootnode_abs = rootnode_offset;
    let root_type = read_u16_be(u8_data, rootnode_abs)?;
    if root_type != 0x0100 {
        return Err(format!("Root node is not a directory: {:#X}", root_type));
    }

    let total_nodes = read_u32_be(u8_data, rootnode_abs + 8)? as usize;
    if total_nodes == 0 {
        return Err("U8 archive has zero nodes".into());
    }

    let string_table_abs = rootnode_abs + total_nodes * 12;
    let header_end_abs = rootnode_offset + header_size;

    if string_table_abs > u8_data.len() || header_end_abs > u8_data.len() {
        return Err(format!(
            "Invalid U8 bounds: string_table_abs={:#X}, header_end_abs={:#X}, len={:#X}",
            string_table_abs,
            header_end_abs,
            u8_data.len()
        ));
    }

    println!(
        "[U8] rootnode_offset={:#X}, header_size={:#X}, data_offset={:#X}, total_nodes={}, string_table_abs={:#X}, output_dir={}",
        rootnode_offset,
        header_size,
        data_offset,
        total_nodes,
        string_table_abs,
        output_dir.display()
    );

    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed creating output dir {}: {}", output_dir.display(), e))?;

    let mut extracted_files = Vec::new();

    // Directory size is the first node index not included in that directory,
    // counting the root node as node 1 on WiiBrew.
    // Since this loop iterates i = 1..total_nodes with zero-based array indexing,
    // using total_nodes for the synthetic root works for traversal.
    let mut dir_stack: Vec<(usize, PathBuf)> = vec![(total_nodes, output_dir.to_path_buf())];

    for i in 1..total_nodes {
        while let Some((end_index, path)) = dir_stack.last() {
            if i >= *end_index {
                println!("[U8] pop dir_stack: i={}, end_index={}, path={}", i, end_index, path.display());
                dir_stack.pop();
            } else {
                break;
            }
        }

        let node_abs = rootnode_abs + i * 12;
        let type_and_name = read_u32_be(u8_data, node_abs)?;
        let node_type = ((type_and_name >> 24) & 0xFF) as u8;
        let name_offset = (type_and_name & 0x00FF_FFFF) as usize;
        let data_offset_or_parent = read_u32_be(u8_data, node_abs + 4)? as usize;
        let size = read_u32_be(u8_data, node_abs + 8)? as usize;

        let name = cstring_at(u8_data, string_table_abs + name_offset)?;
        let current_dir = dir_stack
            .last()
            .map(|(_, p)| p.clone())
            .ok_or_else(|| format!("Directory stack underflow at node {}", i))?;

        let out_path = current_dir.join(&name);

        println!(
            "[U8] node={} type={} name={} name_offset={:#X} field2={:#X} size={:#X} current_dir={} out_path={}",
            i,
            node_type,
            name,
            name_offset,
            data_offset_or_parent,
            size,
            current_dir.display(),
            out_path.display()
        );

        match node_type {
            1 => {
                fs::create_dir_all(&out_path)
                    .map_err(|e| format!("1Failed creating dir {}: {}", out_path.display(), e))?;

                println!(
                    "[U8] mkdir node={} path={} subtree_end_index={}",
                    i,
                    out_path.display(),
                    size
                );

                dir_stack.push((size, out_path));
            }
            0 => {
                let file_start = data_offset_or_parent;
                let file_end = file_start + size;

                let bytes = u8_data
                    .get(file_start..file_end)
                    .ok_or_else(|| {
                        format!(
                            "U8 file node out of bounds: name={}, start={:#X}, end={:#X}, len={:#X}",
                            name,
                            file_start,
                            file_end,
                            u8_data.len()
                        )
                    })?;

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("2Failed creating parent dir {}: {}", parent.display(), e)
                    })?;
                }

                fs::write(&out_path, bytes)
                    .map_err(|e| format!("3Failed writing {}: {}", out_path.display(), e))?;

                println!(
                    "[U8] wrote file node={} path={} bytes={}",
                    i,
                    out_path.display(),
                    bytes.len()
                );

                extracted_files.push(out_path);
            }
            _ => {
                return Err(format!(
                    "Unknown U8 node type {} for node {} (name={})",
                    node_type, i, name
                ));
            }
        }
    }

    println!("[U8] extracted {} files", extracted_files.len());

    Ok(extracted_files)
}

fn unwrap_lz77_container(data: &[u8]) -> Result<&[u8], String> {
    if data.len() < 8 {
        return Err("LZ77 container too small".into());
    }

    let magic = read_u32_be(data, 0)?;
    if magic != 0x4C5A3737 {
        return Err(format!("Expected LZ77 magic, found {:#X}", magic));
    }

    let info = read_u32_be(data, 4)?;
    let method = ((info >> 24) & 0xFF) as u8;

    if method != 0x10 {
        return Err(format!("Unsupported LZ77 method {:#X}", method));
    }

    Ok(&data[4..])
}

#[tauri::command]
pub fn unwrap_bin(bin_path: String) -> Result<Vec<ImageSource>, String> {
    let path = PathBuf::from(&bin_path);

    if !path.is_file() {
        return Err("Provided bin path is not a file".into());
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.bin")
        .to_ascii_lowercase();

    if file_name != "banner.bin" && file_name != "icon.bin" {
        return Err("This function only supports banner.bin or icon.bin".into());
    }

    let data = fs::read(&path).map_err(|e| e.to_string())?;
    let imd5_payload = strip_imd5(&data)?;

    let inner_u8 = {
        let magic = read_u32_be(imd5_payload, 0).unwrap_or(0);

        if magic == 0x55AA382D {
            imd5_payload.to_vec()
        } else if magic == 0x4C5A3737 {
            let lz_stream = unwrap_lz77_container(imd5_payload)?;
            decompress_lz77(lz_stream)?
        } else if imd5_payload.first() == Some(&0x10) {
            decompress_lz77(imd5_payload)?
        } else {
            return Err(format!(
                "Unsupported payload after IMD5. First 4 bytes: {:02X} {:02X} {:02X} {:02X}",
                imd5_payload.get(0).copied().unwrap_or(0),
                imd5_payload.get(1).copied().unwrap_or(0),
                imd5_payload.get(2).copied().unwrap_or(0),
                imd5_payload.get(3).copied().unwrap_or(0),
            ));
        }
    };

    let u8_magic = read_u32_be(&inner_u8, 0)?;
    if u8_magic != 0x55AA382D {
        return Err(format!(
            "Inner payload is not U8, found magic {:#X}",
            u8_magic
        ));
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("bin");
    let parent = path.parent().unwrap_or(Path::new("."));
    let output_dir = parent.join(format!("{}_inner_u8", stem));

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir).map_err(|e| e.to_string())?;
    }

    let extracted_files = extract_u8_to_dir(&inner_u8, &output_dir)?;

    if extracted_files.is_empty() {
        return Err(format!(
            "No files were extracted from {}",
            path.display()
        ));
    }

    Ok(extracted_files
        .into_iter()
        .map(|p| ImageSource {
            source_path: p.display().to_string(),
        })
        .collect())
}