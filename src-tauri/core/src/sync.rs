//! Portability, BYO-Sync (`cosmic-manifest.json`), project manifests (`.cosmicfonts.json`),
//! and client handoff ZIP packager (with generated `LICENSES.txt`).

use crc32fast::Hasher;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::inventory::FontFamily;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalCollection {
    pub id: String,
    pub name: String,
    pub fonts: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryState {
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub collections: Vec<LocalCollection>,
    #[serde(default)]
    pub tags: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestFontRef {
    pub family: String,
    #[serde(default)]
    pub postscript_name: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncCollection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub fonts: Vec<ManifestFontRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct CosmicManifest {
    pub version: u32,
    pub updated_at: u64,
    pub device_name: String,
    #[serde(default)]
    pub favorites: Vec<ManifestFontRef>,
    #[serde(default)]
    pub collections: Vec<SyncCollection>,
    #[serde(default)]
    pub tags: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub active_families: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub matched_favorites: usize,
    pub matched_collections: usize,
    pub unresolved_fonts: Vec<String>,
    pub manifest_path: String,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SyncResult {
    pub library: LibraryState,
    pub report: SyncReport,
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn to_manifest_ref(family_name: &str, families: &[FontFamily]) -> ManifestFontRef {
    if let Some(fam) = families
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(family_name))
    {
        ManifestFontRef {
            family: fam.name.clone(),
            postscript_name: fam.postscript_name.clone(),
            sha256: fam.sha256.clone(),
        }
    } else {
        ManifestFontRef {
            family: family_name.to_string(),
            postscript_name: String::new(),
            sha256: String::new(),
        }
    }
}

/// Deterministic font resolution: SHA-256 -> PostScript name -> family name.
pub fn resolve_manifest_ref(r: &ManifestFontRef, families: &[FontFamily]) -> Option<String> {
    if !r.sha256.is_empty() {
        if let Some(f) = families
            .iter()
            .find(|f| !f.sha256.is_empty() && f.sha256.eq_ignore_ascii_case(&r.sha256))
        {
            return Some(f.name.clone());
        }
    }
    if !r.postscript_name.is_empty() {
        if let Some(f) = families.iter().find(|f| {
            f.postscript_name.eq_ignore_ascii_case(&r.postscript_name)
                || f.styles
                    .iter()
                    .any(|s| s.postscript_name.eq_ignore_ascii_case(&r.postscript_name))
        }) {
            return Some(f.name.clone());
        }
    }
    if !r.family.is_empty() {
        if let Some(f) = families
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&r.family))
        {
            return Some(f.name.clone());
        }
    }
    None
}

pub fn manifest_from_library(
    lib: &LibraryState,
    families: &[FontFamily],
    device_name: &str,
    updated_at: u64,
) -> CosmicManifest {
    let favorites = lib
        .favorites
        .iter()
        .map(|f| to_manifest_ref(f, families))
        .collect();
    let collections = lib
        .collections
        .iter()
        .map(|c| SyncCollection {
            id: c.id.clone(),
            name: c.name.clone(),
            pinned: c.pinned,
            fonts: c
                .fonts
                .iter()
                .map(|f| to_manifest_ref(f, families))
                .collect(),
        })
        .collect();
    let active_families = families
        .iter()
        .filter(|f| f.active && !f.essential && f.error.is_none())
        .map(|f| f.name.clone())
        .collect();
    CosmicManifest {
        version: 1,
        updated_at,
        device_name: device_name.to_string(),
        favorites,
        collections,
        tags: lib.tags.clone(),
        active_families,
    }
}

pub fn merge_manifest_into_library(
    remote: &CosmicManifest,
    local: &LibraryState,
    families: &[FontFamily],
) -> (LibraryState, SyncReport) {
    let mut favs: Vec<String> = local.favorites.clone();
    let mut fav_seen: HashSet<String> = favs.iter().map(|s| s.to_lowercase()).collect();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    let mut matched_favorites = 0usize;

    for item in &remote.favorites {
        if let Some(name) = resolve_manifest_ref(item, families) {
            matched_favorites += 1;
            if fav_seen.insert(name.to_lowercase()) {
                favs.push(name);
            }
        } else if !item.family.is_empty() {
            unresolved.insert(item.family.clone());
        }
    }

    let mut collections = local.collections.clone();
    let mut matched_collections = 0usize;
    for rc in &remote.collections {
        let mut resolved_fonts = Vec::new();
        for rf in &rc.fonts {
            if let Some(name) = resolve_manifest_ref(rf, families) {
                resolved_fonts.push(name);
            } else if !rf.family.is_empty() {
                unresolved.insert(rf.family.clone());
            }
        }

        if let Some(existing) = collections
            .iter_mut()
            .find(|c| c.id == rc.id || c.name.eq_ignore_ascii_case(&rc.name))
        {
            matched_collections += 1;
            existing.pinned |= rc.pinned;
            let mut seen: HashSet<String> =
                existing.fonts.iter().map(|f| f.to_lowercase()).collect();
            for rf in resolved_fonts {
                if seen.insert(rf.to_lowercase()) {
                    existing.fonts.push(rf);
                }
            }
        } else {
            matched_collections += 1;
            collections.push(LocalCollection {
                id: rc.id.clone(),
                name: rc.name.clone(),
                fonts: resolved_fonts,
                pinned: rc.pinned,
            });
        }
    }

    let mut tags = local.tags.clone();
    for (fam, rtags) in &remote.tags {
        let entry = tags.entry(fam.clone()).or_default();
        for t in rtags {
            if !entry.iter().any(|e| e.eq_ignore_ascii_case(t)) {
                entry.push(t.clone());
            }
        }
    }

    let merged = LibraryState {
        favorites: favs,
        collections,
        tags,
    };
    let report = SyncReport {
        matched_favorites,
        matched_collections,
        unresolved_fonts: unresolved.into_iter().collect(),
        manifest_path: String::new(),
        updated_at: remote.updated_at,
    };
    (merged, report)
}

pub fn write_manifest_atomic(path: &Path, manifest: &CosmicManifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn sync_manifest_folder(
    sync_dir: &Path,
    local: &LibraryState,
    families: &[FontFamily],
    device_name: &str,
    push_local: bool,
) -> Result<SyncResult, String> {
    let manifest_path = sync_dir.join("cosmic-manifest.json");
    let mut current_lib = local.clone();
    let mut report = SyncReport {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        updated_at: now_unix(),
        ..Default::default()
    };

    if manifest_path.is_file() {
        let bytes = fs::read(&manifest_path).map_err(|e| e.to_string())?;
        let remote: CosmicManifest =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid manifest: {e}"))?;
        let (merged, mut r) = merge_manifest_into_library(&remote, &current_lib, families);
        current_lib = merged;
        r.manifest_path = report.manifest_path.clone();
        report = r;
    }

    if push_local || !manifest_path.is_file() {
        let ts = now_unix();
        let out = manifest_from_library(&current_lib, families, device_name, ts);
        write_manifest_atomic(&manifest_path, &out)?;
        report.updated_at = ts;
    }

    Ok(SyncResult {
        library: current_lib,
        report,
    })
}

pub fn export_profile_json(
    dest: &Path,
    local: &LibraryState,
    families: &[FontFamily],
    device_name: &str,
) -> Result<(), String> {
    let manifest = manifest_from_library(local, families, device_name, now_unix());
    write_manifest_atomic(dest, &manifest)
}

pub fn import_profile_json(
    src: &Path,
    local: &LibraryState,
    families: &[FontFamily],
) -> Result<SyncResult, String> {
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    let remote: CosmicManifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid profile bundle: {e}"))?;
    let (library, mut report) = merge_manifest_into_library(&remote, local, families);
    report.manifest_path = src.to_string_lossy().into_owned();
    Ok(SyncResult { library, report })
}

// ---- Project-folder manifest (`.cosmicfonts.json`) ----

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectManifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fonts: Vec<String>,
    #[serde(default)]
    pub auto_activate: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DetectedProject {
    pub folder: String,
    pub manifest_path: String,
    pub name: String,
    pub fonts: Vec<String>,
    pub auto_activate: bool,
}

pub fn scan_project_manifests(watched_folders: &[PathBuf]) -> Vec<DetectedProject> {
    let mut out = Vec::new();
    for dir in watched_folders {
        let fallback_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());

        let manifest_path = dir.join(".cosmicfonts.json");
        if manifest_path.is_file() {
            if let Ok(bytes) = fs::read(&manifest_path) {
                if let Ok(pm) = serde_json::from_slice::<ProjectManifest>(&bytes) {
                    out.push(DetectedProject {
                        folder: dir.to_string_lossy().into_owned(),
                        manifest_path: manifest_path.to_string_lossy().into_owned(),
                        name: pm.name.unwrap_or_else(|| fallback_name.clone()),
                        fonts: pm.fonts,
                        auto_activate: pm.auto_activate,
                    });
                    continue;
                }
            }
        }

        // Also detect Adobe Illustrator / InDesign `Document fonts` convention.
        let doc_fonts = dir.join("Document fonts");
        if doc_fonts.is_dir() {
            let mut fams = BTreeSet::new();
            for font_file in super::inventory::list_fonts(&doc_fonts, true) {
                if let Ok(faces) = super::reader::read_faces(&font_file) {
                    if let Some(first) = faces.first() {
                        fams.insert(first.family.clone());
                    }
                }
            }
            if !fams.is_empty() {
                out.push(DetectedProject {
                    folder: dir.to_string_lossy().into_owned(),
                    manifest_path: doc_fonts.to_string_lossy().into_owned(),
                    name: format!("{fallback_name} (Document fonts)"),
                    fonts: fams.into_iter().collect(),
                    auto_activate: false,
                });
            }
        }
    }
    out
}

// ---- Standard PKZIP Archive Writer + Collection ZIP Packager ----

struct ZipEntryRecord {
    name: Vec<u8>,
    crc: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

#[derive(Default)]
pub struct ZipBuilder {
    buf: Vec<u8>,
    records: Vec<ZipEntryRecord>,
}

impl ZipBuilder {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            records: Vec::new(),
        }
    }

    pub fn add_file(&mut self, archive_path: &str, data: &[u8]) -> Result<(), String> {
        let name = archive_path.replace('\\', "/").into_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| "archive path too long")?;
        let uncompressed_size =
            u32::try_from(data.len()).map_err(|_| "file exceeds 4 GiB zip limit")?;

        let mut hasher = Hasher::new();
        hasher.update(data);
        let crc = hasher.finalize();

        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).map_err(|e| e.to_string())?;
        let compressed = encoder.finish().map_err(|e| e.to_string())?;
        let compressed_size =
            u32::try_from(compressed.len()).map_err(|_| "compressed size exceeds 4 GiB")?;

        let local_header_offset =
            u32::try_from(self.buf.len()).map_err(|_| "archive exceeds 4 GiB")?;

        // Local file header
        self.buf.extend_from_slice(&0x04034b50u32.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.buf.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 filename flag
        self.buf.extend_from_slice(&8u16.to_le_bytes()); // Deflate
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        self.buf.extend_from_slice(&0x5421u16.to_le_bytes()); // mod date
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&compressed_size.to_le_bytes());
        self.buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        self.buf.extend_from_slice(&name_len.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        self.buf.extend_from_slice(&name);
        self.buf.extend_from_slice(&compressed);

        self.records.push(ZipEntryRecord {
            name,
            crc,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        });
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        let cd_offset = u32::try_from(self.buf.len()).map_err(|_| "archive exceeds 4 GiB")?;
        for rec in &self.records {
            let name_len = rec.name.len() as u16;
            self.buf.extend_from_slice(&0x02014b50u32.to_le_bytes());
            self.buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
            self.buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
            self.buf.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 flag
            self.buf.extend_from_slice(&8u16.to_le_bytes()); // Deflate
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
            self.buf.extend_from_slice(&0x5421u16.to_le_bytes()); // mod date
            self.buf.extend_from_slice(&rec.crc.to_le_bytes());
            self.buf
                .extend_from_slice(&rec.compressed_size.to_le_bytes());
            self.buf
                .extend_from_slice(&rec.uncompressed_size.to_le_bytes());
            self.buf.extend_from_slice(&name_len.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // disk start
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            self.buf.extend_from_slice(&0x20u32.to_le_bytes()); // external attrs (archive)
            self.buf
                .extend_from_slice(&rec.local_header_offset.to_le_bytes());
            self.buf.extend_from_slice(&rec.name);
        }
        let cd_size = u32::try_from(self.buf.len())
            .map_err(|_| "archive exceeds 4 GiB")?
            .saturating_sub(cd_offset);
        let count = u16::try_from(self.records.len()).map_err(|_| "too many zip entries")?;

        // End of central directory
        self.buf.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&count.to_le_bytes());
        self.buf.extend_from_slice(&count.to_le_bytes());
        self.buf.extend_from_slice(&cd_size.to_le_bytes());
        self.buf.extend_from_slice(&cd_offset.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        Ok(self.buf)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PackageResult {
    pub zip_path: String,
    pub font_files: usize,
    pub families_included: usize,
    pub missing_families: Vec<String>,
}

pub fn package_collection_zip(
    dest_zip: &Path,
    collection_name: &str,
    font_names: &[String],
    families: &[FontFamily],
) -> Result<PackageResult, String> {
    let mut zip = ZipBuilder::new();
    let mut used_names = HashSet::new();
    let mut font_files = 0usize;
    let mut families_included = 0usize;
    let mut missing_families = Vec::new();
    let mut licenses =
        format!("Cosmic Font Foundry — Client Handoff Bundle\nCollection: {collection_name}\n\n");

    for name in font_names {
        let Some(fam) = families.iter().find(|f| f.name.eq_ignore_ascii_case(name)) else {
            missing_families.push(name.clone());
            continue;
        };
        families_included += 1;
        licenses.push_str(&format!("=== {} ===\n", fam.name));
        let tier_label = match fam.license_tier {
            super::reader::LicenseTier::FreeCommercial => "Free / Commercial-Safe",
            super::reader::LicenseTier::PersonalOnly => {
                "Personal Use Only (DO NOT SHIP COMMERCIALLY)"
            }
            super::reader::LicenseTier::Commercial => "Commercial / Proprietary",
            super::reader::LicenseTier::Unknown => "Unknown (verify before commercial use)",
        };
        licenses.push_str(&format!("License Tier: {tier_label}\n"));
        if !fam.sha256.is_empty() {
            licenses.push_str(&format!("SHA-256: {}\n", fam.sha256));
        }
        if let Some(ref foundry) = fam.foundry {
            licenses.push_str(&format!("Foundry: {foundry}\n"));
        }
        if let Some(ref designer) = fam.designer {
            licenses.push_str(&format!("Designer: {designer}\n"));
        }
        if let Some(ref license) = fam.license {
            licenses.push_str(&format!("License: {license}\n"));
        } else {
            licenses.push_str(
                "License: Not embedded in font metadata (verify before commercial use)\n",
            );
        }
        if let Some(ref url) = fam.license_url {
            licenses.push_str(&format!("License URL: {url}\n"));
        }
        licenses.push('\n');

        for path_str in &fam.files {
            let src = Path::new(path_str);
            let Ok(bytes) = fs::read(src) else { continue };
            let raw_name = src
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "font.ttf".into());
            let mut archive_name = raw_name.clone();
            if !used_names.insert(archive_name.to_lowercase()) {
                let stem = Path::new(&raw_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "font".into());
                let ext = Path::new(&raw_name)
                    .extension()
                    .map(|s| format!(".{}", s.to_string_lossy()))
                    .unwrap_or_default();
                for idx in 2.. {
                    let candidate = format!("{stem}_{idx}{ext}");
                    if used_names.insert(candidate.to_lowercase()) {
                        archive_name = candidate;
                        break;
                    }
                }
            }
            zip.add_file(&format!("fonts/{archive_name}"), &bytes)?;
            font_files += 1;
        }
    }

    zip.add_file("LICENSES.txt", licenses.as_bytes())?;
    zip.add_file("FONTS-NOTICE.txt", licenses.as_bytes())?;
    let bytes = zip.finish()?;
    if let Some(parent) = dest_zip.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(dest_zip, bytes).map_err(|e| e.to_string())?;

    Ok(PackageResult {
        zip_path: dest_zip.to_string_lossy().into_owned(),
        font_files,
        families_included,
        missing_families,
    })
}

/// Exports a ready-to-drop Web & Design Kit ZIP with font files, `@font-face` CSS,
/// a modular type scale, Tailwind v4 `@theme` tokens, and `FONTS-NOTICE.txt`.
pub fn export_design_kit_zip(
    dest_zip: &Path,
    heading_family: &str,
    body_family: &str,
    base_px: f32,
    ratio: f32,
    families: &[FontFamily],
) -> Result<PackageResult, String> {
    let mut zip = ZipBuilder::new();
    let mut used_names = HashSet::new();
    let mut font_files = 0usize;
    let mut families_included = 0usize;
    let mut missing_families = Vec::new();
    let mut css = String::from("/* Generated by Cosmic Font Foundry */\n");
    let mut notice = String::from("Cosmic Font Foundry — Design Kit License Notice\n\n");

    let mut targets = vec![heading_family.to_string()];
    if !body_family.eq_ignore_ascii_case(heading_family) {
        targets.push(body_family.to_string());
    }

    for name in &targets {
        let Some(fam) = families.iter().find(|f| f.name.eq_ignore_ascii_case(name)) else {
            missing_families.push(name.clone());
            continue;
        };
        families_included += 1;
        let tier_label = match fam.license_tier {
            super::reader::LicenseTier::FreeCommercial => "Free / Commercial-Safe",
            super::reader::LicenseTier::PersonalOnly => {
                "Personal Use Only (DO NOT SHIP COMMERCIALLY)"
            }
            super::reader::LicenseTier::Commercial => "Commercial / Proprietary",
            super::reader::LicenseTier::Unknown => "Unknown (verify before commercial use)",
        };
        notice.push_str(&format!("=== {} ===\nTier: {tier_label}\n", fam.name));
        if let Some(ref l) = fam.license {
            notice.push_str(&format!("License: {l}\n"));
        }
        if let Some(ref u) = fam.license_url {
            notice.push_str(&format!("URL: {u}\n"));
        }
        notice.push('\n');

        for st in &fam.styles {
            let src = Path::new(&st.file);
            let Ok(bytes) = fs::read(src) else { continue };
            let raw_name = src
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "font.ttf".into());
            let mut archive_name = raw_name.clone();
            if !used_names.insert(archive_name.to_lowercase()) {
                let stem = Path::new(&raw_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "font".into());
                let ext = Path::new(&raw_name)
                    .extension()
                    .map(|s| format!(".{}", s.to_string_lossy()))
                    .unwrap_or_default();
                for idx in 2.. {
                    let candidate = format!("{stem}_{idx}{ext}");
                    if used_names.insert(candidate.to_lowercase()) {
                        archive_name = candidate;
                        break;
                    }
                }
            }
            zip.add_file(&format!("fonts/{archive_name}"), &bytes)?;
            font_files += 1;

            let fmt = if archive_name.to_lowercase().ends_with(".otf") {
                "opentype"
            } else {
                "truetype"
            };
            css.push_str(&format!(
                "@font-face {{\n  font-family: \"{}\";\n  src: url(\"./fonts/{}\") format(\"{}\");\n  font-weight: {};\n  font-style: {};\n  font-display: swap;\n}}\n\n",
                fam.name,
                archive_name,
                fmt,
                st.weight,
                if st.italic { "italic" } else { "normal" }
            ));
        }
    }

    let safe_base = if base_px > 8.0 { base_px } else { 16.0 };
    let safe_ratio = if ratio > 1.05 { ratio } else { 1.25 };
    let steps = [
        ("xs", -2),
        ("sm", -1),
        ("base", 0),
        ("lg", 1),
        ("xl", 2),
        ("2xl", 3),
        ("3xl", 4),
    ];
    css.push_str(":root {\n");
    css.push_str(&format!(
        "  --font-heading: \"{}\", sans-serif;\n",
        heading_family
    ));
    css.push_str(&format!(
        "  --font-body: \"{}\", sans-serif;\n",
        body_family
    ));
    for (label, exp) in steps {
        let px = safe_base * safe_ratio.powi(exp);
        css.push_str(&format!("  --fs-{label}: {:.2}px;\n", px));
    }
    css.push_str("}\n");

    let tailwind = format!(
        "@import \"tailwindcss\";\n\n@theme {{\n  --font-heading: \"{}\", sans-serif;\n  --font-body: \"{}\", sans-serif;\n}}\n",
        heading_family, body_family
    );

    zip.add_file("fonts.css", css.as_bytes())?;
    zip.add_file("tailwind-typography.css", tailwind.as_bytes())?;
    zip.add_file("FONTS-NOTICE.txt", notice.as_bytes())?;

    let bytes = zip.finish()?;
    if let Some(parent) = dest_zip.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(dest_zip, bytes).map_err(|e| e.to_string())?;

    Ok(PackageResult {
        zip_path: dest_zip.to_string_lossy().into_owned(),
        font_files,
        families_included,
        missing_families,
    })
}

fn le16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn le32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn decompress_zip_entry(method: u16, comp: &[u8]) -> Result<Vec<u8>, String> {
    match method {
        0 => Ok(comp.to_vec()),
        8 => {
            let mut dec = DeflateDecoder::new(comp);
            let mut out = Vec::new();
            dec.read_to_end(&mut out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        other => Err(format!("unsupported zip compression method {other}")),
    }
}

/// Extracts `.ttf` and `.otf` font binaries from an in-memory PKZIP archive.
pub fn extract_zip_fonts(zip_bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    // Locate End of Central Directory (PK\x05\x06) to handle archives with data descriptors reliably.
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
            let method = le16(zip_bytes, pos + 10).unwrap_or(0);
            let comp_size = le32(zip_bytes, pos + 20).unwrap_or(0) as usize;
            let name_len = le16(zip_bytes, pos + 28).unwrap_or(0) as usize;
            let extra_len = le16(zip_bytes, pos + 30).unwrap_or(0) as usize;
            let comment_len = le16(zip_bytes, pos + 32).unwrap_or(0) as usize;
            let local_off = le32(zip_bytes, pos + 42).unwrap_or(0) as usize;
            let name_slice = zip_bytes.get(pos + 46..pos + 46 + name_len).unwrap_or(&[]);
            let raw_name = String::from_utf8_lossy(name_slice).into_owned();
            pos += 46 + name_len + extra_len + comment_len;

            let lower = raw_name.to_ascii_lowercase();
            if !(lower.ends_with(".ttf") || lower.ends_with(".otf"))
                || raw_name.contains("__MACOSX")
            {
                continue;
            }
            if zip_bytes.get(local_off..local_off + 4) != Some(b"PK\x03\x04") {
                continue;
            }
            let l_name_len = le16(zip_bytes, local_off + 26).unwrap_or(0) as usize;
            let l_extra_len = le16(zip_bytes, local_off + 28).unwrap_or(0) as usize;
            let data_start = local_off + 30 + l_name_len + l_extra_len;
            let Some(comp_slice) = zip_bytes.get(data_start..data_start + comp_size) else {
                continue;
            };
            if let Ok(bytes) = decompress_zip_entry(method, comp_slice) {
                let fname = Path::new(&raw_name)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or(raw_name);
                out.push((fname, bytes));
            }
        }
        return Ok(out);
    }

    // Fallback: scan local file headers sequentially.
    let mut pos = 0usize;
    while pos + 30 <= zip_bytes.len() && zip_bytes.get(pos..pos + 4) == Some(b"PK\x03\x04") {
        let method = le16(zip_bytes, pos + 8).unwrap_or(0);
        let comp_size = le32(zip_bytes, pos + 18).unwrap_or(0) as usize;
        let name_len = le16(zip_bytes, pos + 26).unwrap_or(0) as usize;
        let extra_len = le16(zip_bytes, pos + 28).unwrap_or(0) as usize;
        let name_slice = zip_bytes.get(pos + 30..pos + 30 + name_len).unwrap_or(&[]);
        let raw_name = String::from_utf8_lossy(name_slice).into_owned();
        let data_start = pos + 30 + name_len + extra_len;
        let Some(comp_slice) = zip_bytes.get(data_start..data_start + comp_size) else {
            break;
        };
        let lower = raw_name.to_ascii_lowercase();
        if (lower.ends_with(".ttf") || lower.ends_with(".otf")) && !raw_name.contains("__MACOSX") {
            if let Ok(bytes) = decompress_zip_entry(method, comp_slice) {
                let fname = Path::new(&raw_name)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or(raw_name);
                out.push((fname, bytes));
            }
        }
        pos = data_start + comp_size;
    }
    Ok(out)
}
