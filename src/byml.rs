use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum Value {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f32),
    Array(Vec<Value>),
    Dict(BTreeMap<String, Value>),
    Null,
}

impl Value {
    pub fn as_dict(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Dict(d) => Some(d),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Value::Float(n) => Some(*n),
            Value::Int(n) => Some(*n as f32),
            _ => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_dict().and_then(|d| d.get(key))
    }
}

struct Reader<'a> {
    data: &'a [u8],
    be: bool,
    names: Vec<String>,
    strings: Vec<String>,
}

impl<'a> Reader<'a> {
    fn u24(&self, o: usize) -> usize {
        if self.be {
            ((self.data[o] as usize) << 16)
                | ((self.data[o + 1] as usize) << 8)
                | self.data[o + 2] as usize
        } else {
            self.data[o] as usize
                | ((self.data[o + 1] as usize) << 8)
                | ((self.data[o + 2] as usize) << 16)
        }
    }
    fn u32(&self, o: usize) -> u32 {
        let b = [
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ];
        if self.be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    }
    fn i32_of(&self, v: u32) -> i64 {
        if self.be {
            i32::from_be_bytes(v.to_be_bytes()) as i64
        } else {
            i32::from_le_bytes(v.to_le_bytes()) as i64
        }
    }
    fn f32_of(&self, v: u32) -> f32 {
        if self.be {
            f32::from_be_bytes(v.to_be_bytes())
        } else {
            f32::from_le_bytes(v.to_le_bytes())
        }
    }
    fn i64_at(&self, o: usize) -> i64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[o..o + 8]);
        if self.be {
            i64::from_be_bytes(b)
        } else {
            i64::from_le_bytes(b)
        }
    }
    fn u64_at(&self, o: usize) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[o..o + 8]);
        if self.be {
            u64::from_be_bytes(b)
        } else {
            u64::from_le_bytes(b)
        }
    }
    fn f64_at(&self, o: usize) -> f64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[o..o + 8]);
        if self.be {
            f64::from_be_bytes(b)
        } else {
            f64::from_le_bytes(b)
        }
    }
    fn string_table(&self, off: usize) -> Result<Vec<String>, String> {
        if off == 0 {
            return Ok(Vec::new());
        }
        if off + 4 > self.data.len() {
            return Err("string table is outside the file".into());
        }
        let count = self.u24(off + 1);
        let mut strings = Vec::with_capacity(count);
        for i in 0..count {
            let rel = self.u32(off + 4 + 4 * i) as usize;
            let s = off + rel;
            if s >= self.data.len() {
                return Err("string is outside the file".into());
            }
            let e = self.data[s..]
                .iter()
                .position(|&b| b == 0)
                .map(|n| s + n)
                .ok_or_else(|| "string is not terminated".to_string())?;
            strings.push(String::from_utf8_lossy(&self.data[s..e]).into_owned());
        }
        Ok(strings)
    }
    fn decode(&self, typ: u8, val: u32, depth: u32) -> Result<Value, String> {
        if depth > 48 {
            return Err("document is nested too deeply".into());
        }
        match typ {
            0xa0 => {
                let i = val as usize;
                self.strings
                    .get(i)
                    .cloned()
                    .map(Value::String)
                    .ok_or_else(|| format!("string index {i} is out of range"))
            }
            0xd0 => Ok(Value::Bool(val != 0)),
            0xd1 => Ok(Value::Int(self.i32_of(val))),
            0xd2 => Ok(Value::Float(self.f32_of(val))),
            0xd3 => Ok(Value::Int(val as i64)),
            0xd4 => Ok(Value::Int(self.i64_at(val as usize))),
            0xd5 => {
                let n = self.u64_at(val as usize);
                i64::try_from(n)
                    .map(Value::Int)
                    .map_err(|_| format!("unsigned value {n} does not fit"))
            }
            0xd6 => Ok(Value::Float(self.f64_at(val as usize) as f32)),
            0xff => Ok(Value::Null),
            0xc1 => self.dict_node(val as usize, depth + 1),
            0xc0 => self.array_node(val as usize, depth + 1),
            other => Err(format!("unsupported value type {other:#x}")),
        }
    }
    fn dict_node(&self, off: usize, depth: u32) -> Result<Value, String> {
        let count = self.u24(off + 1);
        let mut out = BTreeMap::new();
        for i in 0..count {
            let e = off + 4 + i * 8;
            if e + 8 > self.data.len() {
                return Err("dictionary entry is outside the file".into());
            }
            let (key_i, typ, val) = if self.be {
                (self.u24(e + 1), self.data[e], self.u32(e + 4))
            } else {
                (self.u24(e), self.data[e + 3], self.u32(e + 4))
            };
            let key = self
                .names
                .get(key_i)
                .cloned()
                .ok_or_else(|| format!("name index {key_i} is out of range"))?;
            out.insert(key, self.decode(typ, val, depth)?);
        }
        Ok(Value::Dict(out))
    }
    fn array_node(&self, off: usize, depth: u32) -> Result<Value, String> {
        let count = self.u24(off + 1);
        let types = off + 4;
        let values = types + ((count + 3) & !3);
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let typ = self.data[types + i];
            let val = self.u32(values + 4 * i);
            out.push(self.decode(typ, val, depth)?);
        }
        Ok(Value::Array(out))
    }
}

pub fn parse(data: &[u8]) -> Result<Value, String> {
    if data.len() < 16 {
        return Err("file is too small to be a document".into());
    }
    let be = match &data[..2] {
        b"BY" => true,
        b"YB" => false,
        _ => return Err("not a BYML document".into()),
    };
    let mut reader = Reader {
        data,
        be,
        names: Vec::new(),
        strings: Vec::new(),
    };
    let hash_off = reader.u32(4) as usize;
    let str_off = reader.u32(8) as usize;
    let root_off = reader.u32(12) as usize;
    reader.names = reader.string_table(hash_off)?;
    reader.strings = reader.string_table(str_off)?;
    if root_off >= data.len() {
        return Err("document root is outside the file".into());
    }
    match data[root_off] {
        0xc1 => reader.dict_node(root_off, 0),
        0xc0 => reader.array_node(root_off, 0),
        other => Err(format!("document root type {other:#x} is not a list or dictionary")),
    }
}
