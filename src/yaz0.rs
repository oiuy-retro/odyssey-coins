pub fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 16 || &data[..4] != b"Yaz0" {
        return Err("not a Yaz0 file".into());
    }
    let dec_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut out = Vec::with_capacity(dec_size);
    let mut src = 16usize;
    while out.len() < dec_size && src < data.len() {
        let mut code = data[src];
        src += 1;
        for _ in 0..8 {
            if out.len() >= dec_size || src >= data.len() {
                break;
            }
            if code & 0x80 != 0 {
                out.push(data[src]);
                src += 1;
            } else {
                if src + 1 >= data.len() {
                    break;
                }
                let b1 = data[src];
                let b2 = data[src + 1];
                src += 2;
                let dist = (((b1 & 0x0f) as usize) << 8) | b2 as usize;
                let mut copy_len = (b1 >> 4) as usize;
                if copy_len == 0 {
                    if src >= data.len() {
                        break;
                    }
                    copy_len = data[src] as usize + 0x12;
                    src += 1;
                } else {
                    copy_len += 2;
                }
                if dist + 1 > out.len() {
                    return Err("Yaz0 backreference runs past the start".into());
                }
                for _ in 0..copy_len {
                    let b = out[out.len() - dist - 1];
                    out.push(b);
                    if out.len() >= dec_size {
                        break;
                    }
                }
            }
            code = code.wrapping_shl(1);
        }
    }
    if out.len() < dec_size {
        return Err("Yaz0 data ended early".into());
    }
    out.truncate(dec_size);
    Ok(out)
}

pub fn maybe_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() >= 4 && &data[..4] == b"Yaz0" {
        decompress(data)
    } else {
        Ok(data.to_vec())
    }
}
