use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider};

use crate::import::import_svg_glyph;
use crate::model::{compute_bbox, FontMetrics, FoundryProject, GlyphOutline};
use crate::outline_hash::fingerprint_font_bytes;
use crate::typography::auto_space_project;
pub use cosmic_core::reader::{classify_license_class as classify_license, LicenseClass};

const MAX_DIR_WALK_DEPTH: usize = 16;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TriagedFont {
    pub source_entry: String,
    pub family: String,
    pub subfamily: String,
    pub license_class: LicenseClass,
    pub license_summary: String,
    pub outline_digest: String,
    pub is_duplicate: bool,
    pub byte_len: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArchiveTriageReport {
    pub scanned_files: usize,
    pub rejected_unsafe: usize,
    pub duplicate_count: usize,
    pub fonts: Vec<TriagedFont>,
}

fn is_allowed_font_ext(ext: &str) -> bool {
    matches!(ext, "ttf" | "otf" | "woff" | "woff2" | "ttc" | "otc")
}

fn is_unsafe_ext(ext: &str) -> bool {
    matches!(
        ext,
        "exe"
            | "bat"
            | "sh"
            | "ps1"
            | "dll"
            | "js"
            | "vbs"
            | "py"
            | "cmd"
            | "com"
            | "msi"
            | "so"
            | "dylib"
            | "scr"
            | "bin"
            | "hta"
            | "jar"
            | "lnk"
    )
}

fn file_ext_lower(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_unsafe_zip_entry(raw_name: &str) -> bool {
    if raw_name.starts_with('/') || raw_name.starts_with('\\') {
        return true;
    }
    // Reject Windows drive-letter paths (e.g. "C:\...")
    let bytes = raw_name.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    if raw_name.split(['/', '\\']).any(|seg| seg == "..") {
        return true;
    }
    let ext = file_ext_lower(raw_name);
    if is_unsafe_ext(&ext) {
        return true;
    }
    // Non-font binary extensions that are neither font nor plain documentation/metadata.
    !ext.is_empty()
        && !is_allowed_font_ext(&ext)
        && !matches!(
            ext.as_str(),
            "txt"
                | "md"
                | "markdown"
                | "json"
                | "xml"
                | "html"
                | "htm"
                | "css"
                | "pdf"
                | "svg"
                | "yaml"
                | "yml"
                | "toml"
                | "csv"
                | "license"
        )
}

fn le16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn le32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

struct ZipScanStats {
    scanned_files: usize,
    rejected_unsafe: usize,
    unsafe_font_basenames: HashSet<String>,
}

fn scan_zip_entries(zip_bytes: &[u8]) -> ZipScanStats {
    let mut stats = ZipScanStats {
        scanned_files: 0,
        rejected_unsafe: 0,
        unsafe_font_basenames: HashSet::new(),
    };

    let mut raw_names: Vec<String> = Vec::new();

    if let Some(eocd) = zip_bytes
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .filter(|&p| p + 22 <= zip_bytes.len())
    {
        let total = le16(zip_bytes, eocd + 10).unwrap_or(0) as usize;
        let cd_off = le32(zip_bytes, eocd + 16).unwrap_or(0) as usize;
        let mut pos = cd_off;
        for _ in 0..total {
            if zip_bytes.get(pos..pos + 4) != Some(b"PK\x01\x02") || pos + 46 > zip_bytes.len() {
                break;
            }
            let name_len = le16(zip_bytes, pos + 28).unwrap_or(0) as usize;
            let extra_len = le16(zip_bytes, pos + 30).unwrap_or(0) as usize;
            let comment_len = le16(zip_bytes, pos + 32).unwrap_or(0) as usize;
            if let Some(name_slice) = zip_bytes.get(pos + 46..pos + 46 + name_len) {
                raw_names.push(String::from_utf8_lossy(name_slice).into_owned());
            }
            pos += 46 + name_len + extra_len + comment_len;
        }
    } else {
        let mut pos = 0usize;
        while pos + 30 <= zip_bytes.len() && zip_bytes.get(pos..pos + 4) == Some(b"PK\x03\x04") {
            let comp_size = le32(zip_bytes, pos + 18).unwrap_or(0) as usize;
            let name_len = le16(zip_bytes, pos + 26).unwrap_or(0) as usize;
            let extra_len = le16(zip_bytes, pos + 28).unwrap_or(0) as usize;
            if let Some(name_slice) = zip_bytes.get(pos + 30..pos + 30 + name_len) {
                raw_names.push(String::from_utf8_lossy(name_slice).into_owned());
            }
            let next = pos
                .saturating_add(30)
                .saturating_add(name_len)
                .saturating_add(extra_len)
                .saturating_add(comp_size);
            if next <= pos {
                break;
            }
            pos = next;
        }
    }

    for raw_name in raw_names {
        if raw_name.ends_with('/') || raw_name.ends_with('\\') {
            continue;
        }
        stats.scanned_files += 1;
        if is_unsafe_zip_entry(&raw_name) {
            stats.rejected_unsafe += 1;
            if let Some(base) = raw_name
                .rsplit(['/', '\\'])
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                stats.unsafe_font_basenames.insert(base.to_string());
            }
        }
    }

    stats
}

fn localized_name(font: &FontRef<'_>, id: StringId) -> Option<String> {
    font.localized_strings(id)
        .english_or_first()
        .map(|s| s.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let normalized: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let mut out: String = normalized
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect();
        out.push_str("...");
        out
    }
}

fn triage_font_entry(source_entry: String, bytes: &[u8]) -> TriagedFont {
    let fallback_family = Path::new(&source_entry)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let (family, subfamily, license_class, license_summary) =
        if let Ok(font) = FontRef::from_index(bytes, 0) {
            let family = localized_name(&font, StringId::TYPOGRAPHIC_FAMILY_NAME)
                .or_else(|| localized_name(&font, StringId::FAMILY_NAME))
                .unwrap_or_else(|| fallback_family.clone());
            let subfamily = localized_name(&font, StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
                .or_else(|| localized_name(&font, StringId::SUBFAMILY_NAME))
                .unwrap_or_else(|| "Regular".to_string());

            let license_desc = localized_name(&font, StringId::LICENSE_DESCRIPTION);
            let copyright = localized_name(&font, StringId::COPYRIGHT_NOTICE);
            let license_url = localized_name(&font, StringId::LICENSE_URL);

            let mut license_class = license_desc
                .as_deref()
                .map(classify_license)
                .unwrap_or(LicenseClass::Unknown);
            if license_class == LicenseClass::Unknown {
                let combined = [
                    license_desc.as_deref().unwrap_or(""),
                    copyright.as_deref().unwrap_or(""),
                    license_url.as_deref().unwrap_or(""),
                ]
                .join(" ");
                license_class = classify_license(&combined);
            }

            let raw_summary = license_desc
                .or(copyright)
                .or(license_url)
                .unwrap_or_else(|| "No embedded license metadata".to_string());
            let license_summary = truncate_summary(&raw_summary, 180);

            (family, subfamily, license_class, license_summary)
        } else {
            (
                fallback_family,
                "Regular".to_string(),
                LicenseClass::Unknown,
                "Compressed or non-SFNT container".to_string(),
            )
        };

    let outline_digest = match fingerprint_font_bytes(bytes, &source_entry) {
        Ok(fp) => fp.digest,
        Err(_) => "none".to_string(),
    };

    TriagedFont {
        source_entry,
        family,
        subfamily,
        license_class,
        license_summary,
        outline_digest,
        is_duplicate: false,
        byte_len: bytes.len(),
    }
}

fn collect_dir_files(
    dir: &Path,
    depth: usize,
    scanned_files: &mut usize,
    rejected_unsafe: &mut usize,
    font_paths: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > MAX_DIR_WALK_DEPTH {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("failed to read directory '{}': {e}", dir.display()))?
        .filter_map(|res| res.ok().map(|e| e.path()))
        .collect();
    entries.sort();

    for entry in entries {
        let Ok(meta) = fs::symlink_metadata(&entry) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_dir_files(
                &entry,
                depth + 1,
                scanned_files,
                rejected_unsafe,
                font_paths,
            )?;
        } else if meta.is_file() {
            *scanned_files += 1;
            let ext = entry
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            if is_allowed_font_ext(&ext) {
                font_paths.push(entry);
            } else if is_unsafe_ext(&ext) {
                *rejected_unsafe += 1;
            }
        }
    }
    Ok(())
}

fn finalize_duplicates(fonts: &mut [TriagedFont]) -> usize {
    let mut seen_digests: HashSet<String> = HashSet::new();
    let mut duplicate_count = 0usize;
    for font in fonts.iter_mut() {
        if font.outline_digest == "none" {
            continue;
        }
        if !seen_digests.insert(font.outline_digest.clone()) {
            font.is_duplicate = true;
            duplicate_count += 1;
        }
    }
    duplicate_count
}

/// Scans a `.zip` archive, directory, or single font file for license compliance,
/// unsafe payloads, and canonical outline duplicates.
pub fn triage_path(path: &Path) -> Result<ArchiveTriageReport, String> {
    if !path.exists() {
        return Err(format!("path does not exist: '{}'", path.display()));
    }

    let mut scanned_files = 0usize;
    let mut rejected_unsafe = 0usize;
    let mut fonts: Vec<TriagedFont> = Vec::new();

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    if path.is_file() && ext == "zip" {
        let zip_bytes =
            fs::read(path).map_err(|e| format!("failed to read zip '{}': {e}", path.display()))?;
        let scan = scan_zip_entries(&zip_bytes);
        scanned_files = scan.scanned_files;
        rejected_unsafe = scan.rejected_unsafe;

        let extracted = cosmic_core::sync::extract_zip_fonts(&zip_bytes)?;
        for (entry_name, bytes) in extracted {
            if scan.unsafe_font_basenames.contains(&entry_name) {
                continue;
            }
            fonts.push(triage_font_entry(entry_name, &bytes));
        }
    } else if path.is_dir() {
        let mut font_paths = Vec::new();
        collect_dir_files(
            path,
            0,
            &mut scanned_files,
            &mut rejected_unsafe,
            &mut font_paths,
        )?;
        for fp in font_paths {
            if let Ok(bytes) = fs::read(&fp) {
                let display = fp
                    .strip_prefix(path)
                    .unwrap_or(&fp)
                    .to_string_lossy()
                    .into_owned();
                fonts.push(triage_font_entry(display, &bytes));
            }
        }
    } else if path.is_file() {
        scanned_files = 1;
        if is_unsafe_ext(&ext) {
            rejected_unsafe = 1;
        } else if is_allowed_font_ext(&ext) {
            let bytes = fs::read(path)
                .map_err(|e| format!("failed to read font '{}': {e}", path.display()))?;
            let display = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("font")
                .to_string();
            fonts.push(triage_font_entry(display, &bytes));
        }
    }

    let duplicate_count = finalize_duplicates(&mut fonts);

    Ok(ArchiveTriageReport {
        scanned_files,
        rejected_unsafe,
        duplicate_count,
        fonts,
    })
}

fn parse_codepoint_from_stem(stem: &str) -> Option<u32> {
    let trimmed = stem.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Single Unicode scalar (check first so "0.svg" maps to '0' = 48, not hex 0).
    let mut chars = trimmed.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return Some(ch as u32);
    }

    if trimmed.eq_ignore_ascii_case("space") {
        return Some(0x20);
    }

    // U+XXXX / u+XXXX
    if let Some(rest) = trimmed
        .strip_prefix("U+")
        .or_else(|| trimmed.strip_prefix("u+"))
    {
        if !rest.is_empty() && rest.len() <= 6 && rest.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(cp) = u32::from_str_radix(rest, 16) {
                if char::from_u32(cp).is_some() {
                    return Some(cp);
                }
            }
        }
    }

    // uniXXXX / UNIXXXX
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("uni") {
        if (4..=6).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(cp) = u32::from_str_radix(rest, 16) {
                if char::from_u32(cp).is_some() {
                    return Some(cp);
                }
            }
        }
    }

    // 0xXXXX
    if let Some(rest) = lower.strip_prefix("0x") {
        if !rest.is_empty() && rest.len() <= 6 && rest.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(cp) = u32::from_str_radix(rest, 16) {
                if char::from_u32(cp).is_some() {
                    return Some(cp);
                }
            }
        }
    }

    // Hex stem like "0042" or "00E9" (4..=6 hex digits starting with '0' or containing A-F).
    if (4..=6).contains(&trimmed.len())
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
        && (trimmed.starts_with('0') || trimmed.chars().any(|c| c.is_ascii_alphabetic()))
    {
        if let Ok(cp) = u32::from_str_radix(trimmed, 16) {
            if char::from_u32(cp).is_some() {
                return Some(cp);
            }
        }
    }

    // Pure multi-digit decimal > 31 (e.g. "65" -> 'A').
    if (2..=6).contains(&trimmed.len()) && trimmed.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(cp) = trimmed.parse::<u32>() {
            if cp > 31 && char::from_u32(cp).is_some() {
                return Some(cp);
            }
        }
    }

    None
}

/// Batch-assembles a folder of `.svg` glyph files (named by character, `U+XXXX`,
/// `uniXXXX`, `0042`, or decimal codepoint) into a spaced `FoundryProject`.
pub fn import_svg_folder(
    dir: &Path,
    family_name: &str,
    upm: u16,
) -> Result<FoundryProject, String> {
    if !dir.is_dir() {
        return Err(format!("not a directory: '{}'", dir.display()));
    }

    let upm_clamped = upm.max(100);
    let mut svg_paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("failed to read SVG folder '{}': {e}", dir.display()))?
        .filter_map(|res| res.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        })
        .collect();
    svg_paths.sort();

    let mut glyphs: BTreeMap<String, GlyphOutline> = BTreeMap::new();
    let mut loaded_count = 0usize;

    for svg_path in svg_paths {
        let Some(stem) = svg_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(cp) = parse_codepoint_from_stem(stem) else {
            continue;
        };
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        let Ok(svg_text) = fs::read_to_string(&svg_path) else {
            continue;
        };
        let Ok(contours) = import_svg_glyph(&svg_text, upm_clamped) else {
            continue;
        };
        if contours.is_empty() {
            continue;
        }

        let bbox = compute_bbox(&contours);
        let width = (bbox[2] - bbox[0]).max(0.0);
        let sidebearing = (upm_clamped as f64) * 0.06;
        let advance_width = (width + sidebearing * 2.0)
            .max((upm_clamped as f64) * 0.25)
            .round() as i32;
        let lsb = bbox[0].round() as i32;

        let key = if cp == 0x20 {
            "space".to_string()
        } else {
            ch.to_string()
        };
        let name = if cp == 0x20 {
            "space".to_string()
        } else if ch.is_ascii_alphanumeric() {
            ch.to_string()
        } else {
            format!("uni{cp:04X}")
        };

        glyphs.insert(
            key,
            GlyphOutline {
                codepoint: cp,
                name,
                advance_width,
                lsb,
                contours,
            },
        );
        loaded_count += 1;
    }

    if loaded_count == 0 {
        return Err(format!(
            "no valid .svg glyphs loaded from '{}'",
            dir.display()
        ));
    }

    let has_space = glyphs.values().any(|g| g.codepoint == 0x20);
    if !has_space {
        glyphs.insert(
            "space".to_string(),
            GlyphOutline {
                codepoint: 0x20,
                name: "space".to_string(),
                advance_width: (upm_clamped as i32 / 3).max(200),
                lsb: 0,
                contours: Vec::new(),
            },
        );
    }

    let clean_family = family_name.trim();
    let final_family = if clean_family.is_empty() {
        "Imported SVG Font".to_string()
    } else {
        clean_family.to_string()
    };

    let metrics = FontMetrics {
        upm: upm_clamped,
        ascender: ((upm_clamped as f64) * 0.80).round() as i16,
        descender: -(((upm_clamped as f64) * 0.20).round() as i16),
        x_height: ((upm_clamped as f64) * 0.52).round() as i16,
        cap_height: ((upm_clamped as f64) * 0.72).round() as i16,
        line_gap: 0,
    };

    let mut project = FoundryProject {
        version: 3,
        family_name: final_family,
        style_name: "Regular".to_string(),
        designer: "Cosmic Foundry".to_string(),
        license: "OFL-1.1".to_string(),
        metrics,
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    };

    auto_space_project(&mut project, (upm_clamped as f64) * 0.06);
    Ok(project)
}
