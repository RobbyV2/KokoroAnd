use crate::{Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const STYLE_DIM: usize = 256;
pub const STYLE_ROWS: usize = 510;

pub fn load(path: &Path) -> Result<HashMap<String, Vec<f32>>> {
    let file = File::open(path).map_err(|e| Error::Voice(format!("{}: {e}", path.display())))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Voice(e.to_string()))?;
    let mut voices = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::Voice(e.to_string()))?;
        let name = match entry.name().strip_suffix(".npy") {
            Some(n) => n.to_string(),
            None => continue,
        };
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Voice(format!("{name}: {e}")))?;
        voices.insert(name.clone(), parse_npy(&name, &bytes)?);
    }
    match voices.is_empty() {
        true => Err(Error::Voice("no voices in archive".into())),
        false => Ok(voices),
    }
}

fn parse_npy(name: &str, bytes: &[u8]) -> Result<Vec<f32>> {
    let bad = |m: &str| Error::Voice(format!("{name}: {m}"));
    match bytes.get(..6) {
        Some(b"\x93NUMPY") => {}
        _ => return Err(bad("bad npy magic")),
    }
    let major = *bytes.get(6).ok_or_else(|| bad("truncated"))?;
    let (header_len, header_start) = match major {
        1 => {
            let l = bytes.get(8..10).ok_or_else(|| bad("truncated"))?;
            (u16::from_le_bytes([l[0], l[1]]) as usize, 10)
        }
        _ => {
            let l = bytes.get(8..12).ok_or_else(|| bad("truncated"))?;
            (u32::from_le_bytes([l[0], l[1], l[2], l[3]]) as usize, 12)
        }
    };
    let data_start = header_start + header_len;
    let header = std::str::from_utf8(
        bytes
            .get(header_start..data_start)
            .ok_or_else(|| bad("truncated"))?,
    )
    .map_err(|_| bad("bad header"))?;
    match header.contains("'<f4'") && !header.contains("True") {
        true => {}
        false => return Err(bad("expect little-endian f32 C-order")),
    }
    let data = bytes.get(data_start..).ok_or_else(|| bad("truncated"))?;
    match data.len() == STYLE_ROWS * STYLE_DIM * 4 {
        true => Ok(data
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()),
        false => Err(bad("unexpected style shape")),
    }
}
