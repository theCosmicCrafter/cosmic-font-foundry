//! Font metadata & table reader (.ttf/.otf/.ttc/.otc).
//! Seeks directly to required OpenType tables (name, OS/2, fvar, GSUB, cmap) and computes SHA-256.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::category::{self, CategoryData};

/// Safety limit per table (32 MiB) to guard against malformed or malicious font files.
const MAX_TABLE_BYTES: u32 = 32 * 1024 * 1024;
const MAX_FACES: u32 = 64;
const MAX_FONT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_NAME_RECORDS: usize = 256;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct VariableAxis {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    FreeCommercial,
    PersonalOnly,
    Commercial,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseClass {
    Ofl,
    Apache,
    Mit,
    PublicDomain,
    PersonalOnly,
    Commercial,
    Unknown,
}

impl LicenseClass {
    pub fn to_tier(self) -> LicenseTier {
        match self {
            Self::Ofl | Self::Apache | Self::Mit | Self::PublicDomain => {
                LicenseTier::FreeCommercial
            }
            Self::PersonalOnly => LicenseTier::PersonalOnly,
            Self::Commercial => LicenseTier::Commercial,
            Self::Unknown => LicenseTier::Unknown,
        }
    }
}

fn contains_word(haystack_lower: &str, word: &str) -> bool {
    haystack_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == word)
}

/// Classifies an embedded license or copyright notice string into a granular `LicenseClass`.
pub fn classify_license_class(text: &str) -> LicenseClass {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return LicenseClass::Unknown;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.contains("sil open font license")
        || lower.contains("open font license")
        || lower.contains("scripts.sil.org/ofl")
        || contains_word(&lower, "ofl")
    {
        return LicenseClass::Ofl;
    }
    if lower.contains("apache") {
        return LicenseClass::Apache;
    }
    if lower.contains("mit license")
        || lower.contains("permission is hereby granted")
        || contains_word(&lower, "mit")
    {
        return LicenseClass::Mit;
    }
    if lower.contains("cc0")
        || lower.contains("public domain")
        || lower.contains("unlicense")
        || lower.contains("creative commons zero")
    {
        return LicenseClass::PublicDomain;
    }
    if lower.contains("personal use")
        || lower.contains("demo only")
        || lower.contains("evaluation only")
        || lower.contains("non-commercial")
        || lower.contains("noncommercial")
        || lower.contains("for personal")
        || lower.contains("free for personal")
        || lower.contains("not for commercial")
        || contains_word(&lower, "trial")
    {
        return LicenseClass::PersonalOnly;
    }
    if lower.contains("all rights reserved")
        || lower.contains("commercial license")
        || lower.contains("commercial use")
        || lower.contains("proprietary")
        || lower.contains("eula")
        || lower.contains("copyright")
    {
        return LicenseClass::Commercial;
    }

    LicenseClass::Unknown
}

/// Classifies OpenType license text/URL into a practical commercial-use tier.
pub fn classify_license(license: Option<&str>, license_url: Option<&str>) -> LicenseTier {
    let combined = format!(
        "{} {}",
        license.unwrap_or_default(),
        license_url.unwrap_or_default()
    );
    let lower = combined.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return LicenseTier::Unknown;
    }
    let class = classify_license_class(&lower);
    match class {
        LicenseClass::PersonalOnly => return LicenseTier::PersonalOnly,
        LicenseClass::Ofl
        | LicenseClass::Apache
        | LicenseClass::Mit
        | LicenseClass::PublicDomain => {
            return LicenseTier::FreeCommercial;
        }
        _ => {}
    }
    if ["ubuntu font", "gpl", "free for commercial"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        return LicenseTier::FreeCommercial;
    }
    if matches!(class, LicenseClass::Commercial)
        || [
            "commercial",
            "monotype",
            "linotype",
            "myfonts",
            "fontspring",
            "adobe",
        ]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        return LicenseTier::Commercial;
    }
    LicenseTier::Unknown
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FontFace {
    /// Typographic family (name ID 16, falling back to 1).
    pub family: String,
    /// Legacy Windows family (name ID 1).
    pub win_family: String,
    /// Style / subfamily (name ID 17, falling back to 2).
    pub style: String,
    /// Full font name (name ID 4).
    pub full_name: String,
    /// PostScript name (name ID 6).
    #[serde(default)]
    pub postscript_name: String,
    pub weight: u16,
    pub italic: bool,
    pub category: String,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub axes: Vec<VariableAxis>,
    #[serde(default)]
    pub opentype_features: Vec<String>,
    #[serde(default)]
    pub foundry: Option<String>,
    #[serde(default)]
    pub designer: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default)]
    pub license_tier: LicenseTier,
    #[serde(default)]
    pub panose: [u8; 10],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileReadResult {
    pub faces: Vec<FontFace>,
    pub sha256: String,
}

fn be16(b: &[u8], i: usize) -> Option<u16> {
    b.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_at(f: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut v = vec![0; len];
    f.seek(SeekFrom::Start(offset))?;
    f.read_exact(&mut v)?;
    Ok(v)
}

fn read_tables(
    f: &mut File,
    start: u64,
    wanted: &[&[u8; 4]],
) -> Result<Vec<Option<Vec<u8>>>, String> {
    let hdr = read_at(f, start, 12).map_err(|e| e.to_string())?;
    let num_tables = be16(&hdr, 4).ok_or("short SFNT header")? as usize;
    if num_tables == 0 || num_tables > 512 {
        return Err("invalid SFNT table directory".into());
    }
    let dir = read_at(f, start + 12, num_tables * 16).map_err(|e| e.to_string())?;
    let mut out = vec![None; wanted.len()];
    for rec in dir.as_chunks::<16>().0 {
        let Some(idx) = wanted.iter().position(|t| &rec[0..4] == *t) else {
            continue;
        };
        let (off, len) = (be32(rec, 8).unwrap_or(0), be32(rec, 12).unwrap_or(0));
        if len == 0 || len > MAX_TABLE_BYTES {
            continue;
        }
        out[idx] = Some(read_at(f, off as u64, len as usize).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn face_offsets(f: &mut File) -> Result<Vec<u64>, String> {
    let hdr = read_at(f, 0, 12).map_err(|_| "not a font file".to_string())?;
    match &hdr[0..4] {
        b"ttcf" => {
            let n = be32(&hdr, 8).unwrap_or(0).min(MAX_FACES) as usize;
            let offs = read_at(f, 12, n * 4).map_err(|e| e.to_string())?;
            Ok(offs
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_be_bytes(*c) as u64)
                .collect())
        }
        [0, 1, 0, 0] | b"OTTO" | b"true" => Ok(vec![0]),
        _ => Err("not a font file".into()),
    }
}

fn pick_name(t: &ttf_parser::name::Table, id: u16) -> Option<String> {
    let mut best: Option<(u8, String)> = None;
    for n in t.names.into_iter().take(MAX_NAME_RECORDS) {
        if n.name_id != id {
            continue;
        }
        let Some(s) = n.to_string() else { continue };
        let s = s.trim().to_string();
        if s.is_empty() {
            continue;
        }
        let score = match (n.platform_id, n.language_id) {
            (ttf_parser::PlatformId::Windows, 0x409) => 3,
            (ttf_parser::PlatformId::Windows, _) => 2,
            _ => 1,
        };
        if best.as_ref().is_none_or(|(m, _)| score > *m) {
            best = Some((score, s));
        }
    }
    best.map(|(_, s)| s)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn default_axis_label(tag: &str) -> String {
    match tag {
        "wght" => "Weight".into(),
        "wdth" => "Width".into(),
        "opsz" => "Optical Size".into(),
        "slnt" => "Slant".into(),
        "ital" => "Italic".into(),
        other => other.to_uppercase(),
    }
}

fn parse_axes(
    fvar_bytes: Option<&[u8]>,
    name_table: &ttf_parser::name::Table,
) -> Vec<VariableAxis> {
    let Some(bytes) = fvar_bytes else {
        return vec![];
    };
    let Some(fvar) = ttf_parser::fvar::Table::parse(bytes) else {
        return vec![];
    };
    fvar.axes
        .into_iter()
        .map(|a| {
            let tag_bytes = a.tag.to_bytes();
            let tag = String::from_utf8_lossy(&tag_bytes).trim().to_string();
            let name = pick_name(name_table, a.name_id).unwrap_or_else(|| default_axis_label(&tag));
            VariableAxis {
                tag,
                name,
                min: a.min_value,
                default: a.def_value,
                max: a.max_value,
                hidden: a.hidden,
            }
        })
        .collect()
}

/// Reads OpenType GSUB FeatureList tags directly without decoding all lookups.
fn parse_gsub_features(gsub_bytes: Option<&[u8]>) -> Vec<String> {
    let Some(data) = gsub_bytes else {
        return vec![];
    };
    if data.len() < 10 {
        return vec![];
    }
    let Some(feat_list_off) = be16(data, 6).map(|v| v as usize) else {
        return vec![];
    };
    if feat_list_off + 2 > data.len() {
        return vec![];
    }
    let Some(count) = be16(data, feat_list_off).map(|v| v as usize) else {
        return vec![];
    };
    let records_start = feat_list_off + 2;
    let max_records = (data.len() - records_start) / 6;
    let n = count.min(max_records);
    let mut tags = BTreeSet::new();
    for i in 0..n {
        let pos = records_start + i * 6;
        if let Some(slice) = data.get(pos..pos + 4) {
            if slice.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
                let tag = String::from_utf8_lossy(slice).trim().to_string();
                if !tag.is_empty() {
                    tags.insert(tag);
                }
            }
        }
    }
    tags.into_iter().collect()
}

fn detect_scripts(u: &[u32; 4], codepages: Option<u32>) -> Vec<String> {
    let mut scripts = Vec::new();
    if u[0] & 0b11 != 0 {
        scripts.push("Latin".into());
    }
    if u[0] & (1 << 2) != 0 || u[0] & (1 << 3) != 0 {
        scripts.push("Latin Ext".into());
    }
    if u[0] & (1 << 7) != 0 {
        scripts.push("Greek".into());
    }
    if u[0] & (1 << 9) != 0 {
        scripts.push("Cyrillic".into());
    }
    if u[0] & (1 << 11) != 0 {
        scripts.push("Hebrew".into());
    }
    if u[0] & (1 << 13) != 0 {
        scripts.push("Arabic".into());
    }
    if u[0] & (1 << 15) != 0 {
        scripts.push("Devanagari".into());
    }
    if u[0] & (1 << 24) != 0 {
        scripts.push("Thai".into());
    }
    if u[1] & (1 << (49 - 32)) != 0 || u[1] & (1 << (50 - 32)) != 0 || u[1] & (1 << (59 - 32)) != 0
    {
        scripts.push("CJK".into());
    }
    if u[0] & (1 << 28) != 0 || u[1] & (1 << (56 - 32)) != 0 {
        scripts.push("Korean".into());
    }
    if let Some(cp) = codepages {
        if cp & (1 << 8) != 0 && !scripts.iter().any(|s| s == "Latin Ext") {
            scripts.push("Vietnamese".into());
        }
        if cp & (1 << 17) != 0 && !scripts.iter().any(|s| s == "Japanese") {
            scripts.push("Japanese".into());
        }
    }
    if u[1] & (1 << (57 - 32)) != 0 {
        scripts.push("Symbols".into());
    }
    scripts
}

pub fn file_sha256(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn read_file(path: &Path) -> Result<FileReadResult, String> {
    let sha256 = file_sha256(path)?;
    let faces = read_faces(path)?;
    Ok(FileReadResult { faces, sha256 })
}

fn check_font_file_size(f: &File) -> Result<(), String> {
    let meta = f.metadata().map_err(|e| e.to_string())?;
    if meta.len() > MAX_FONT_BYTES {
        return Err("font file exceeds 100 MiB safety limit".into());
    }
    Ok(())
}

pub fn read_faces(path: &Path) -> Result<Vec<FontFace>, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    check_font_file_size(&f)?;
    let mut out = Vec::new();
    for offset in face_offsets(&mut f)? {
        let t = read_tables(&mut f, offset, &[b"name", b"OS/2", b"fvar", b"GSUB"])?;
        let name = t[0]
            .as_deref()
            .and_then(ttf_parser::name::Table::parse)
            .ok_or("missing name table")?;
        let os2 = t[1].as_deref();

        let win_family = pick_name(&name, 1).unwrap_or_else(|| stem(path));
        let family = pick_name(&name, 16).unwrap_or_else(|| win_family.clone());
        let style = pick_name(&name, 17)
            .or_else(|| pick_name(&name, 2))
            .unwrap_or_else(|| "Regular".into());
        let full_name = pick_name(&name, 4).unwrap_or_else(|| format!("{family} {style}"));
        let postscript_name = pick_name(&name, 6).unwrap_or_else(|| full_name.replace(' ', "-"));

        let foundry = pick_name(&name, 8);
        let designer = pick_name(&name, 9);
        let license = pick_name(&name, 13);
        let license_url = pick_name(&name, 14);

        let mut data = CategoryData {
            family: family.clone(),
            ..Default::default()
        };
        let (mut weight, mut italic) = (400, style.to_lowercase().contains("italic"));
        if let Some(o) = os2 {
            weight = be16(o, 4).filter(|p| (1..=1000).contains(p)).unwrap_or(400);
            let sel = be16(o, 62).unwrap_or(0);
            italic = sel & 1 != 0 || sel & (1 << 9) != 0;
            data.family_class = be16(o, 30).unwrap_or(0) as i16;
            if let Some(p) = o.get(32..42) {
                data.panose.copy_from_slice(p);
            }
            for (i, r) in data.unicode_ranges.iter_mut().enumerate() {
                *r = be32(o, 42 + i * 4).unwrap_or(0);
            }
            if be16(o, 0).unwrap_or(0) >= 1 {
                data.codepages = be32(o, 78);
            }
        }
        let scripts = detect_scripts(&data.unicode_ranges, data.codepages);
        let axes = parse_axes(t[2].as_deref(), &name);
        let opentype_features = parse_gsub_features(t[3].as_deref());
        let category = category::classify(&data).into();

        let license_tier = classify_license(license.as_deref(), license_url.as_deref());

        out.push(FontFace {
            family,
            win_family,
            style,
            full_name,
            postscript_name,
            weight,
            italic,
            category,
            scripts,
            axes,
            opentype_features,
            foundry,
            designer,
            license,
            license_url,
            license_tier,
            panose: data.panose,
        });
    }
    if out.is_empty() {
        return Err("font file contains no faces".into());
    }
    Ok(out)
}

/// Returns true Unicode codepoints mapped in the first face's cmap table (up to `limit`).
pub fn characters(path: &Path, limit: usize) -> Result<Vec<u32>, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    check_font_file_size(&f)?;
    let start = *face_offsets(&mut f)?.first().ok_or("empty font")?;
    let t = read_tables(&mut f, start, &[b"cmap"])?;
    let cmap = t[0]
        .as_deref()
        .and_then(ttf_parser::cmap::Table::parse)
        .ok_or("missing cmap table")?;
    let mut set = BTreeSet::new();
    for sub in cmap.subtables {
        if !sub.is_unicode() {
            continue;
        }
        sub.codepoints(|c| {
            if let Some(ch) = char::from_u32(c) {
                if !ch.is_control() && !ch.is_whitespace() && !(0xE000..=0xF8FF).contains(&c) {
                    set.insert(c);
                }
            }
        });
    }
    if set.is_empty() {
        for sub in cmap.subtables {
            sub.codepoints(|c| {
                if (0xF020..=0xF0FF).contains(&c) {
                    set.insert(c);
                }
            });
        }
    }
    Ok(set.into_iter().take(limit).collect())
}

/// Computes the lowercase hex SHA-256 digest of an in-memory byte slice.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Computes the lowercase hex SHA-256 digest of a file on disk.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_font_files_exceeding_max_font_bytes() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.as_file().set_len(MAX_FONT_BYTES + 1).unwrap();
        let err = read_faces(tmp.path()).unwrap_err();
        assert!(err.contains("100 MiB"), "unexpected error: {err}");
        let char_err = characters(tmp.path(), 64).unwrap_err();
        assert!(
            char_err.contains("100 MiB"),
            "unexpected char error: {char_err}"
        );
    }
}
