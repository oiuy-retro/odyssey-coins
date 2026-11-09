use std::collections::BTreeMap;

/// Checkpoint display names from a stage message file. Labels look like
/// `Checkpoint_obj1524`. The text itself comes from the file the user supplied.
pub fn checkpoint_names(data: &[u8]) -> Result<BTreeMap<String, String>, String> {
    if data.len() < 0x20 || &data[..8] != b"MsgStdBn" {
        return Err("not a message file".into());
    }
    if &data[8..10] != b"\xff\xfe" {
        return Err("message file is not little-endian".into());
    }
    let mut off = 0x20usize;
    let mut lbl = None;
    let mut txt = None;
    while off + 16 <= data.len() {
        let magic = &data[off..off + 4];
        if magic != b"LBL1" && magic != b"TXT2" && magic != b"ATR1" && magic != b"TSY1" {
            break;
        }
        let size = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap()) as usize;
        let body = data
            .get(off + 16..off + 16 + size)
            .ok_or("message section is truncated")?;
        if magic == b"LBL1" {
            lbl = Some(body);
        } else if magic == b"TXT2" {
            txt = Some(body);
        }
        off = (off + 16 + size + 15) & !15;
    }
    let lbl = lbl.ok_or("message file has no labels")?;
    let txt = txt.ok_or("message file has no text")?;
    let groups = u32_at(lbl, 0)?;
    let mut p = 4usize;
    let mut labels = Vec::new();
    for _ in 0..groups {
        let count = u32_at(lbl, p)? as usize;
        let goff = u32_at(lbl, p + 4)? as usize;
        p += 8;
        if count == 0 {
            continue;
        }
        let mut q = goff;
        for _ in 0..count {
            let nlen = *lbl.get(q).ok_or("label is truncated")? as usize;
            let name = std::str::from_utf8(
                lbl.get(q + 1..q + 1 + nlen)
                    .ok_or("label name is truncated")?,
            )
            .map_err(|_| "label name is not text".to_string())?
            .to_string();
            let idx = u32_at(lbl, q + 1 + nlen)?;
            labels.push((name, idx));
            q += 1 + nlen + 4;
        }
    }
    let nstr = u32_at(txt, 0)? as usize;
    let mut offsets = Vec::with_capacity(nstr);
    for i in 0..nstr {
        offsets.push(u32_at(txt, 4 + 4 * i)? as usize);
    }
    let mut out = BTreeMap::new();
    for (name, idx) in labels {
        let Some(rest) = name.strip_prefix("Checkpoint_") else {
            continue;
        };
        let text = text_at(txt, offsets.get(idx as usize).copied())?;
        if !text.is_empty() {
            out.insert(rest.to_string(), text);
        }
    }
    Ok(out)
}

fn u32_at(data: &[u8], o: usize) -> Result<u32, String> {
    let b = data
        .get(o..o + 4)
        .ok_or("message file ended early")?;
    Ok(u32::from_le_bytes(b.try_into().unwrap()))
}

fn text_at(txt: &[u8], offset: Option<usize>) -> Result<String, String> {
    let mut o = offset.ok_or("message text index is out of range")?;
    let mut chars = String::new();
    while o + 1 < txt.len() {
        let c = u16::from_le_bytes([txt[o], txt[o + 1]]);
        o += 2;
        if c == 0 {
            break;
        }
        if c == 0x0e {
            // Skip a short control sequence. Checkpoint names do not use these.
            if o + 1 < txt.len() {
                let kind = txt[o + 1] as usize;
                o += 2;
                let skip = match kind {
                    0x00 | 0x01 | 0x02 | 0x03 | 0x04 => 2,
                    _ => 0,
                };
                o += skip;
            }
            continue;
        }
        if let Some(ch) = char::from_u32(c as u32) {
            if ch == '\n' || ch == '\r' {
                chars.push(' ');
            } else if !ch.is_control() {
                chars.push(ch);
            }
        }
    }
    Ok(chars.trim().to_string())
}
