use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Blob{
    data: Vec<u8>,
    size: usize,
}

// read 4 bytes from the file, and converts them to a big-endian u32
fn read_be_u32<R: Read>(file: &mut R) -> io::Result<u32>{
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

fn read_c_string(data: &[u8], start: usize) -> String {
    let end = data[start..]
    .iter()
    .position(|&b| b == 0)
    .map(|pos| start + pos)
    .unwrap_or(data.len());

    String::from_utf8_lossy(&data[start..end]).to_string()
}

#[tauri::command]
pub fn find_data_partition<R: Read + Seek>(mut file: &mut R) -> io::Result<u64> {
    // seek to the disc metadata area
    file.seek(SeekFrom::Start(0x40000))?;

    // read how many partitions are in the first volume of the disk
    let partition_num = read_be_u32(&mut file)?;

    // read where the partition table is stored
    let partition_table_offset = (read_be_u32(&mut file)? as u64) << 2;

    // preparing var for DATA partition start offset
    let mut data_partition_offset = None;

    // seeks to the partition table
    file.seek(SeekFrom::Start(partition_table_offset))?;

    // reads every partitions offset and type
    for _ in 0..partition_num{
        let partition_offset = (read_be_u32(&mut file)? as u64) << 2;
        let partition_type = read_be_u32(&mut file)?;

        // finds the data partition, the one with game data and FST
        if partition_type == 0 {
            data_partition_offset = Some(partition_offset);
            break;
        }
    }

    // makes sure the partition was actually found
    let partition_offset = data_partition_offset.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "DATA partition not found"))?;

    // actual DATA area begins 0x20000 bytes into the partition, so move to that
    let partition_data_offset = partition_offset + 0x20000;
    // seek to the start of DATA area
    file.seek(SeekFrom::Start(partition_data_offset))?;

    // returning the blob
    return Ok(partition_data_offset)
}

pub fn get_bnr<R: Read + Seek>(mut file: &mut R, partition_data_offset: u64) -> io::Result<Blob> {

    // go to DATA partition
    file.seek(SeekFrom::Start(partition_data_offset + 0x424))?;

    // find the FST length
    let fst_offset = read_be_u32(&mut file)?;

    // turn the length into bytes
    let fst_offset_bytes = fst_offset as u64 * 4;

    // seek to FST size
    file.seek(SeekFrom::Start(partition_data_offset + 0x428))?;

    // store FST size
    let fst_size = read_be_u32(&mut file)?;

    // seek to the FST
    // file.seek(SeekFrom::Start(fst_offset_bytes))?;
    file.seek(SeekFrom::Start(partition_data_offset + fst_offset_bytes))?;

    // read the FST
    let mut fst_data = vec![0u8; fst_size as usize];

    eprintln!("partition_data_offset: {:#X}", partition_data_offset);
    eprintln!("fst_offset: {:#X}", fst_offset);
    eprintln!("fst_offset_bytes: {:#X}", fst_offset_bytes);
    eprintln!("fst_size: {:#X}", fst_size);
    eprintln!("seeking to: {:#X}", partition_data_offset + fst_offset_bytes);

    file.read_exact(&mut fst_data)?;

    eprintln!("{:#X}", fst_data.len());

    // get fst_header and other data
    let fst_header = &fst_data[0..0x20];
    let _fst_magic = u32::from_be_bytes([fst_header[0], fst_header[1], fst_header[2], fst_header[3]]);
    let _file_offset_factor = u32::from_be_bytes([fst_header[4], fst_header[5], fst_header[6], fst_header[7]]);
    let num_entries = u32::from_be_bytes([fst_header[8], fst_header[9], fst_header[10], fst_header[11]]);
    
    let header_size: usize = 0;
    const ENTRY_SIZE: usize = 12;

    eprintln!("fst_offset_bytes: {:#X}", fst_offset_bytes);
    eprintln!("fst_size: {:#X}", fst_size);
    eprintln!("num_entries: {}", num_entries);

    // loop over the FSTs files
    for i in 0..num_entries{
        let entry_start = header_size + i as usize * ENTRY_SIZE;
        let entry_slice = &fst_data[entry_start..entry_start + ENTRY_SIZE];

        let entry_type = entry_slice[0];
        let name_offset = ((entry_slice[1] as u32) << 16) | ((entry_slice[2] as u32) << 8) | (entry_slice[3] as u32);
        let data_offset = u32::from_be_bytes([entry_slice[4], entry_slice[5], entry_slice[6], entry_slice[7]]);
        let size        = u32::from_be_bytes([entry_slice[8], entry_slice[9], entry_slice[10], entry_slice[11]]);

        let string_table_start = num_entries as usize * ENTRY_SIZE;
        let name_start = string_table_start + name_offset as usize;

        let filename = read_c_string(&fst_data, name_start);


        println!("string_table_start: {:#X}", string_table_start);

        if filename == "opening.bnr"{
            println!(
                "[{}] type={} name={} name_off={:#X} data_off={:#X} size={:#X}",
                i, entry_type, filename, name_offset, data_offset, size
            );
        }
    }

    return Ok(Blob {
        data: Vec::new(),
        size: 0 as usize,
    })

}


#[tauri::command]
pub fn get_bnr_from_iso(path: String) -> Result<String, String> {
    // let mut file = File::open(&path).map_err(|e| e.to_string())?;

    // let partition_data_offset = find_data_partition(&mut file)
    //     .map_err(|e| e.to_string())?;

    // let blob = get_bnr(&mut file, partition_data_offset)
    //     .map_err(|e| e.to_string())?;

    // Ok(format!("Read {} Bytes", blob.size))

       let mut file = File::open(&path).map_err(|e| e.to_string())?;

    let partition_data_offset = find_data_partition(&mut file)
        .map_err(|e| e.to_string())?;

    file.seek(SeekFrom::Start(partition_data_offset + 0x424))
        .map_err(|e| e.to_string())?;
    let fst_offset = read_be_u32(&mut file).map_err(|e| e.to_string())?;
    let fst_offset_bytes = fst_offset as u64 * 4;

    file.seek(SeekFrom::Start(partition_data_offset + 0x428))
        .map_err(|e| e.to_string())?;
    let fst_size = read_be_u32(&mut file).map_err(|e| e.to_string())?;

    Ok(format!(
        "partition_data_offset={:#X}, fst_offset={:#X}, fst_offset_bytes={:#X}, fst_size={:#X}",
        partition_data_offset, fst_offset, fst_offset_bytes, fst_size
    ))
}