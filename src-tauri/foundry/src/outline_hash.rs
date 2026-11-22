use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

const PROBE_CHARS: [char; 8] = ['A', 'g', 'R', 'S', '0', 'H', 'O', 'e'];
const FALLBACK_GLYPH_LIMIT: usize = 8;

const FNV1A_BASIS_1: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME_1: u64 = 0x0000_0100_0000_01b3;
const FNV1A_BASIS_2: u64 = 0xaf63_bd4c_8601_b7df;
const FNV1A_PRIME_2: u64 = 0x0000_0100_0000_01b3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutlineFingerprint {
    pub digest: String,
    pub glyphs_hashed: usize,
    pub family_hint: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DuplicateGroup {
    pub digest: String,
    pub glyphs_hashed: usize,
    pub files: Vec<OutlineFingerprint>,
}

struct CanonicalHashPen {
    scale: f64,
    buf: Vec<u8>,
    cmd_count: usize,
}

impl CanonicalHashPen {
    fn new(scale: f64) -> Self {
        Self {
            scale,
            buf: Vec::with_capacity(256),
            cmd_count: 0,
        }
    }

    fn quantize(&self, val: f32) -> i32 {
        let scaled = (val as f64) * self.scale;
        if scaled.is_finite() {
            scaled.round() as i32
        } else {
            0
        }
    }

    fn push_coords(&mut self, tag: u8, vals: &[f32]) {
        self.buf.push(tag);
        for &v in vals {
            self.buf.extend_from_slice(&self.quantize(v).to_le_bytes());
        }
        self.cmd_count += 1;
    }
}

impl OutlinePen for CanonicalHashPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push_coords(b'M', &[x, y]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push_coords(b'L', &[x, y]);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.push_coords(b'Q', &[cx0, cy0, x, y]);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.push_coords(b'C', &[cx0, cy0, cx1, cy1, x, y]);
    }

    fn close(&mut self) {
        self.buf.push(b'Z');
        self.cmd_count += 1;
    }
}

struct Fnv128 {
    h1: u64,
    h2: u64,
}

impl Fnv128 {
    fn new() -> Self {
        Self {
            h1: FNV1A_BASIS_1,
            h2: FNV1A_BASIS_2,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.h1 ^= b as u64;
            self.h1 = self.h1.wrapping_mul(FNV1A_PRIME_1);

            let b2 = b.rotate_left(3) ^ 0xa5;
            self.h2 ^= b2 as u64;
            self.h2 = self.h2.wrapping_mul(FNV1A_PRIME_2);
        }
    }

    fn finish_hex(self) -> String {
        format!("{:016x}{:016x}", self.h1, self.h2)
    }
}

fn localized_family_hint(font: &FontRef<'_>) -> String {
    font.localized_strings(StringId::FAMILY_NAME)
        .english_or_first()
        .or_else(|| {
            font.localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
                .english_or_first()
        })
        .map(|s| s.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn draw_canonical_glyph(font: &FontRef<'_>, gid: GlyphId, scale: f64) -> Option<Vec<u8>> {
    let glyph = font.outline_glyphs().get(gid)?;
    let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
    let mut pen = CanonicalHashPen::new(scale);
    if glyph.draw(settings, &mut pen).is_err() || pen.cmd_count == 0 {
        return None;
    }
    Some(pen.buf)
}

/// Computes a deterministic 128-bit canonical geometric outline fingerprint
/// from raw SFNT font bytes, normalized to 1000 UPM and ignoring metadata tables.
pub fn fingerprint_font_bytes(bytes: &[u8], path_hint: &str) -> Result<OutlineFingerprint, String> {
    let font =
        FontRef::new(bytes).map_err(|e| format!("failed to parse font '{path_hint}': {e}"))?;
    let family_hint = localized_family_hint(&font);

    let upm = font
        .head()
        .map(|h| h.units_per_em())
        .unwrap_or_else(|_| {
            font.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
        })
        .max(1) as f64;
    let scale = 1000.0 / upm;

    let charmap = font.charmap();
    let mut probe_drawn: Vec<(u32, Vec<u8>)> = Vec::with_capacity(PROBE_CHARS.len());

    for ch in PROBE_CHARS {
        if let Some(gid) = charmap.map(ch) {
            if let Some(buf) = draw_canonical_glyph(&font, gid, scale) {
                probe_drawn.push((ch as u32, buf));
            }
        }
    }

    let selected: Vec<(u32, Vec<u8>)> = if probe_drawn.len() >= 2 {
        probe_drawn
    } else {
        let mut all_mappings: Vec<(u32, GlyphId)> = charmap.mappings().collect();
        all_mappings.sort_by_key(|(cp, _)| *cp);
        let mut seen = HashSet::new();
        let mut fallback = Vec::with_capacity(FALLBACK_GLYPH_LIMIT);
        for (cp, gid) in all_mappings {
            if !seen.insert(cp) {
                continue;
            }
            if let Some(buf) = draw_canonical_glyph(&font, gid, scale) {
                fallback.push((cp, buf));
                if fallback.len() >= FALLBACK_GLYPH_LIMIT {
                    break;
                }
            }
        }
        fallback
    };

    if selected.is_empty() {
        return Err(format!("no drawable glyph outlines found in '{path_hint}'"));
    }

    let mut hasher = Fnv128::new();
    for (cp, buf) in &selected {
        hasher.update(b"G");
        hasher.update(&cp.to_le_bytes());
        hasher.update(&(buf.len() as u32).to_le_bytes());
        hasher.update(buf);
    }

    Ok(OutlineFingerprint {
        digest: hasher.finish_hex(),
        glyphs_hashed: selected.len(),
        family_hint,
        path: path_hint.to_string(),
    })
}

/// Computes an `OutlineFingerprint` for a font file on disk.
pub fn fingerprint_font_file(path: &Path) -> Result<OutlineFingerprint, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let path_str = path.to_string_lossy();
    fingerprint_font_bytes(&bytes, &path_str)
}

/// Fingerprints all provided font paths and returns groups of files sharing
/// identical canonical outline digests (size >= 2), sorted by group size desc then digest.
pub fn find_outline_duplicates(paths: &[PathBuf]) -> Vec<DuplicateGroup> {
    let mut groups: BTreeMap<String, Vec<OutlineFingerprint>> = BTreeMap::new();
    for path in paths {
        if let Ok(fp) = fingerprint_font_file(path) {
            groups.entry(fp.digest.clone()).or_default().push(fp);
        }
    }

    let mut result: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(digest, mut files)| {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            let glyphs_hashed = files.first().map(|f| f.glyphs_hashed).unwrap_or(0);
            DuplicateGroup {
                digest,
                glyphs_hashed,
                files,
            }
        })
        .collect();

    result.sort_by(|a, b| {
        b.files
            .len()
            .cmp(&a.files.len())
            .then_with(|| a.digest.cmp(&b.digest))
    });
    result
}
