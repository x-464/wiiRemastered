use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn extract_tag_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);

    let start = block.find(&open)? + open.len();
    let end_rel = block[start..].find(&close)?;
    let end = start + end_rel;

    Some(&block[start..end])
}

fn xml_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[tauri::command]
pub fn get_title_from_id(app: AppHandle, game_id: String) -> Result<String, String> {
    let id = game_id.trim().to_ascii_uppercase();

    if id.len() != 6 {
        return Err("Game ID must be 6 characters".to_string());
    }

    let xml_path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
        .join("gametdb.xml");

    if !xml_path.is_file() {
        return Err(format!("gametdb.xml not found at {}", xml_path.display()));
    }

    let xml = fs::read_to_string(&xml_path).map_err(|e| e.to_string())?;

    let mut search_from = 0usize;

    while let Some(game_start_rel) = xml[search_from..].find("<game") {
        let game_start = search_from + game_start_rel;

        let open_end_rel = xml[game_start..]
            .find('>')
            .ok_or_else(|| "Malformed XML: <game> tag not closed".to_string())?;
        let content_start = game_start + open_end_rel + 1;

        let game_end_rel = xml[content_start..]
            .find("</game>")
            .ok_or_else(|| "Malformed XML: </game> not found".to_string())?;
        let game_end = content_start + game_end_rel;

        let game_block = &xml[content_start..game_end];

        let found_id = extract_tag_text(game_block, "id");
        if let Some(found_id) = found_id {
            if found_id.trim().eq_ignore_ascii_case(&id) {
                if let Some(title) = extract_tag_text(game_block, "title") {
                    return Ok(xml_unescape(title.trim()));
                } else {
                    return Err(format!("Game found for ID {}, but no <title> tag exists", id));
                }
            }
        }

        search_from = game_end + "</game>".len();
    }

    Err(format!("No GameTDB entry found for ID {}", id))
}