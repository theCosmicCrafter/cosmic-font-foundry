//! Pure-Rust WOFF 1.0 encoder for SFNT (`.ttf` / `.otf`) font files.

use flate2::{write::ZlibEncoder, Compression};
use std::io::Write;

const WOFF_HEADER_LEN: usize = 44;
const WOFF_DIR_ENTRY_LEN: usize = 20;
const SFNT_HEADER_LEN: usize = 12;
const SFNT_DIR_ENTRY_LEN: usize = 16;

fn be16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

fn be32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

struct PreparedTable {
    tag: [u8; 4],
    checksum: u32,
    orig_len: u32,
    data: Vec<u8>,
}

/// Converts a single SFNT (`.ttf` or `.otf`) binary into a WOFF 1.0 web font container.
pub fn sfnt_to_woff(sfnt: &[u8]) -> Result<Vec<u8>, String> {
    if sfnt.len() < SFNT_HEADER_LEN {
        return Err("font file too small to contain SFNT header".into());
    }
    let flavor = &sfnt[0..4];
    let valid_flavor = flavor == [0x00, 0x01, 0x00, 0x00] || flavor == b"OTTO" || flavor == b"true";
    if !valid_flavor {
        return Err("only single-face TTF/OTF SFNT fonts can be converted to WOFF".into());
    }

    let num_tables = be16(sfnt, 4).ok_or("missing SFNT numTables")? as usize;
    if num_tables == 0 {
        return Err("SFNT font has zero tables".into());
    }
    let dir_end = SFNT_HEADER_LEN
        .checked_add(num_tables * SFNT_DIR_ENTRY_LEN)
        .ok_or("SFNT directory overflow")?;
    if sfnt.len() < dir_end {
        return Err("truncated SFNT table directory".into());
    }

    let mut total_sfnt_size: u64 = dir_end as u64;
    let mut prepared = Vec::with_capacity(num_tables);

    for i in 0..num_tables {
        let entry_off = SFNT_HEADER_LEN + i * SFNT_DIR_ENTRY_LEN;
        let tag: [u8; 4] = sfnt[entry_off..entry_off + 4]
            .try_into()
            .map_err(|_| "invalid table tag")?;
        let checksum = be32(sfnt, entry_off + 4).ok_or("missing table checksum")?;
        let offset = be32(sfnt, entry_off + 8).ok_or("missing table offset")? as usize;
        let orig_len_u32 = be32(sfnt, entry_off + 12).ok_or("missing table length")?;
        let orig_len = orig_len_u32 as usize;

        let table_end = offset
            .checked_add(orig_len)
            .ok_or("SFNT table bounds overflow")?;
        let raw = sfnt
            .get(offset..table_end)
            .ok_or_else(|| format!("truncated SFNT table {}", String::from_utf8_lossy(&tag)))?;

        total_sfnt_size = total_sfnt_size
            .checked_add(align4(orig_len) as u64)
            .ok_or("SFNT size overflow")?;

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
        enc.write_all(raw).map_err(|e| e.to_string())?;
        let compressed = enc.finish().map_err(|e| e.to_string())?;

        let data = if compressed.len() < raw.len() {
            compressed
        } else {
            raw.to_vec()
        };

        prepared.push(PreparedTable {
            tag,
            checksum,
            orig_len: orig_len_u32,
            data,
        });
    }

    let total_sfnt_u32 =
        u32::try_from(total_sfnt_size).map_err(|_| "font exceeds 4 GB WOFF limit")?;

    let mut data_offset = WOFF_HEADER_LEN + num_tables * WOFF_DIR_ENTRY_LEN;
    let mut offsets = Vec::with_capacity(num_tables);
    for t in &prepared {
        offsets.push(data_offset as u32);
        data_offset += align4(t.data.len());
    }
    let total_woff_len = u32::try_from(data_offset).map_err(|_| "WOFF exceeds 4 GB limit")?;

    let mut out = Vec::with_capacity(data_offset);
    // 44-byte WOFF header
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(flavor);
    out.extend_from_slice(&total_woff_len.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&total_sfnt_u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // metaLength
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength
    out.extend_from_slice(&0u32.to_be_bytes()); // privOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // privLength

    // 20-byte TableDirectory entries
    for (t, off) in prepared.iter().zip(offsets.iter()) {
        out.extend_from_slice(&t.tag);
        out.extend_from_slice(&off.to_be_bytes());
        out.extend_from_slice(&(t.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&t.orig_len.to_be_bytes());
        out.extend_from_slice(&t.checksum.to_be_bytes());
    }

    // 4-byte padded table blocks
    for t in &prepared {
        out.extend_from_slice(&t.data);
        let pad = align4(t.data.len()) - t.data.len();
        out.extend(std::iter::repeat_n(0u8, pad));
    }

    debug_assert_eq!(out.len(), data_offset);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn encodes_fixture_ttf_to_valid_woff() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/GochiHand-Regular.ttf");
        let sfnt = std::fs::read(&fixture).expect("fixture font readable");
        let woff = sfnt_to_woff(&sfnt).expect("woff conversion should succeed");

        assert_eq!(&woff[0..4], b"wOFF");
        assert_eq!(&woff[4..8], &sfnt[0..4]);
        let woff_len = be32(&woff, 8).unwrap() as usize;
        assert_eq!(woff_len, woff.len());
        assert_eq!(woff_len % 4, 0);
        let num_tables = be16(&woff, 12).unwrap();
        assert_eq!(num_tables, be16(&sfnt, 4).unwrap());
        assert!(woff.len() < sfnt.len());
    }

    #[test]
    fn rejects_invalid_sfnt() {
        assert!(sfnt_to_woff(b"not a font").is_err());
        assert!(sfnt_to_woff(b"ttcf\x00\x01\x00\x00\x00\x00\x00\x00").is_err());
    }
}
