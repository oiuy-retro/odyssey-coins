pub struct SarcFile {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn read(data: &[u8]) -> Result<Vec<SarcFile>, String> {
    if data.len() < 0x20 || &data[..4] != b"SARC" {
        return Err("not a SARC archive".into());
    }
    let be = match &data[6..8] {
        b"\xFE\xFF" => true,
        b"\xFF\xFE" => false,
        _ => return Err("SARC byte order is not recognized".into()),
    };
    let u16 = |o: usize| -> u16 {
        let b = [data[o], data[o + 1]];
        if be {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        }
    };
    let u32 = |o: usize| -> u32 {
        let b = [data[o], data[o + 1], data[o + 2], data[o + 3]];
        if be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };
    let header_len = u16(4) as usize;
    let data_off = u32(0x0c) as usize;
    if header_len + 12 > data.len() || &data[header_len..header_len + 4] != b"SFAT" {
        return Err("SARC is missing its file table".into());
    }
    let sfat_header = u16(header_len + 4) as usize;
    let count = u16(header_len + 6) as usize;
    let nodes = header_len + sfat_header;
    let sfnt = nodes + count * 16;
    if sfnt + 8 > data.len() || &data[sfnt..sfnt + 4] != b"SFNT" {
        return Err("SARC is missing its name table".into());
    }
    let sfnt_header = u16(sfnt + 4) as usize;
    let name_base = sfnt + sfnt_header;
    let mut files = Vec::with_capacity(count);
    for i in 0..count {
        let o = nodes + i * 16;
        let name_raw = u32(o + 4);
        let start = u32(o + 8) as usize;
        let end = u32(o + 12) as usize;
        let rel = (name_raw & 0x00ff_ffff) as usize;
        let s = name_base + rel * 4;
        if s >= data.len() {
            return Err("SARC file name is outside the archive".into());
        }
        let e = data[s..]
            .iter()
            .position(|&b| b == 0)
            .map(|n| s + n)
            .ok_or_else(|| "SARC file name is not terminated".to_string())?;
        let name = String::from_utf8_lossy(&data[s..e]).into_owned();
        let from = data_off + start;
        let to = data_off + end;
        if to > data.len() || from > to {
            return Err(format!("SARC file {name} is outside the archive"));
        }
        files.push(SarcFile {
            name,
            data: data[from..to].to_vec(),
        });
    }
    Ok(files)
}
