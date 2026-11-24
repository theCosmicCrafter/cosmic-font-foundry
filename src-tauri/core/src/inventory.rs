//! Core font inventory models, essential-font guards, and family grouping
//! with duplicate (SHA-256) and style-conflict detection.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use super::reader::{FileReadResult, LicenseTier, VariableAxis};

pub const EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// Windows core UI fonts that must never be deactivated or quarantined.
const ESSENTIAL_FAMILIES: [&str; 8] = [
    "segoe fluent icons",
    "segoe mdl2 assets",
    "marlett",
    "tahoma",
    "microsoft sans serif",
    "arial",
    "consolas",
    "segoe ui",
];

pub fn is_essential(family: &str) -> bool {
    let f = family.to_lowercase();
    ESSENTIAL_FAMILIES.iter().any(|i| f == *i) || f.starts_with("segoe ui")
}

pub fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Atomically writes JSON to `path` via a `.tmp` sibling and rename.
pub fn write_json_atomic<T: Serialize>(path: &Path, val: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(val).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Returns `dir.join(name)` if unused, otherwise appends ` (2)`, ` (3)`, … before the extension.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let base = Path::new(name);
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = base
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|cand| !cand.exists())
        .unwrap()
}

pub fn list_fonts(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(it) = fs::read_dir(&d) else { continue };
        for entry in it.flatten() {
            let p = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() && recursive => stack.push(p),
                Ok(t) if t.is_file() && is_font_file(&p) => out.push(p),
                _ => {}
            }
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Origin {
    #[serde(rename = "sys")]
    System,
    #[serde(rename = "lib")]
    Library,
    #[serde(rename = "usr")]
    User,
    #[serde(rename = "dir")]
    Folder,
}

/// A font file discovered in the OS, library, deactivated store, or watched folders.
#[derive(Clone, Debug)]
pub struct FontEntry {
    pub path: PathBuf,
    pub origin: Origin,
    pub active: bool,
    /// Registry value `(value_name, value_data)` if registered.
    pub value: Option<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FontStyle {
    pub name: String,
    pub weight: u16,
    pub italic: bool,
    pub file: String,
    /// Legacy Windows GDI family name (name ID 1).
    pub win_family: String,
    /// Full name (name ID 4), used with CSS `local()`.
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub postscript_name: String,
    #[serde(default)]
    pub axes: Vec<VariableAxis>,
    #[serde(default)]
    pub opentype_features: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DuplicateEntry {
    pub path: String,
    /// `"exact"` (identical SHA-256 across different files) or `"conflict"` (same family + style, different binary).
    pub kind: String,
    pub sha256: String,
    pub origin: Origin,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FontFamily {
    pub name: String,
    pub category: String,
    pub origin: Origin,
    pub active: bool,
    pub essential: bool,
    pub styles: Vec<FontStyle>,
    /// Primary Regular (or first) style file used for quick preview loading.
    pub file: String,
    pub files: Vec<String>,
    /// Read error if the font file is corrupt/unreadable.
    pub error: Option<String>,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub is_variable: bool,
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
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub postscript_name: String,
    #[serde(default)]
    pub duplicates: Vec<DuplicateEntry>,
    #[serde(default)]
    pub has_conflicts: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct InventoryDiagnostics {
    pub total_families: usize,
    pub exact_duplicates: usize,
    pub style_conflicts: usize,
    pub corrupt_files: usize,
}

pub fn summarize_diagnostics(families: &[FontFamily]) -> InventoryDiagnostics {
    let mut diag = InventoryDiagnostics {
        total_families: families.len(),
        ..Default::default()
    };
    for fam in families {
        if fam.error.is_some() {
            diag.corrupt_files += 1;
        }
        for dup in &fam.duplicates {
            if dup.kind == "exact" {
                diag.exact_duplicates += 1;
            } else if dup.kind == "conflict" {
                diag.style_conflicts += 1;
            }
        }
    }
    diag
}

/// Groups font entries into families while collecting variable axes, OpenType features,
/// script coverage, license metadata, SHA-256 duplicates, and family/style conflicts.
pub fn group_families(
    entries: &[FontEntry],
    read: &dyn Fn(&Path) -> Result<FileReadResult, String>,
) -> Vec<FontFamily> {
    let mut by_family: BTreeMap<String, FontFamily> = BTreeMap::new();
    let mut sorted: Vec<&FontEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| (e.origin, path_key(&e.path)));

    // Track (sha256 -> first path) to identify exact binary duplicates across different files.
    let mut seen_hashes: HashMap<String, String> = HashMap::new();
    // Track per-family style signature -> (file_key, dir_key, origin, sha256) to identify cross-folder/cross-origin style collisions.
    let mut seen_styles: HashMap<String, (String, String, Origin, String)> = HashMap::new();

    for entry in sorted {
        let file = entry.path.to_string_lossy().into_owned();
        let file_k = path_key(&entry.path);
        match read(&entry.path) {
            Err(err) => {
                let name = entry
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                by_family.insert(
                    format!("\u{0}{file_k}"),
                    FontFamily {
                        name,
                        category: "display".into(),
                        origin: entry.origin,
                        active: entry.active,
                        essential: false,
                        styles: vec![],
                        file: file.clone(),
                        files: vec![file],
                        error: Some(err),
                        scripts: vec![],
                        is_variable: false,
                        axes: vec![],
                        opentype_features: vec![],
                        foundry: None,
                        designer: None,
                        license: None,
                        license_url: None,
                        license_tier: LicenseTier::Unknown,
                        panose: [0; 10],
                        sha256: String::new(),
                        postscript_name: String::new(),
                        duplicates: vec![],
                        has_conflicts: false,
                    },
                );
            }
            Ok(result) => {
                let sha = result.sha256.clone();
                let is_exact_dup = match seen_hashes.get(&sha) {
                    Some(first_key) if first_key != &file_k => true,
                    Some(_) => false,
                    None => {
                        seen_hashes.insert(sha.clone(), file_k.clone());
                        false
                    }
                };

                for face in result.faces {
                    let fam_key = face.family.to_lowercase();
                    let f = by_family
                        .entry(fam_key.clone())
                        .or_insert_with(|| FontFamily {
                            name: face.family.clone(),
                            category: face.category.clone(),
                            origin: entry.origin,
                            active: false,
                            essential: is_essential(&face.family),
                            styles: vec![],
                            file: file.clone(),
                            files: vec![],
                            error: None,
                            scripts: vec![],
                            is_variable: false,
                            axes: vec![],
                            opentype_features: vec![],
                            foundry: face.foundry.clone(),
                            designer: face.designer.clone(),
                            license: face.license.clone(),
                            license_url: face.license_url.clone(),
                            license_tier: face.license_tier,
                            panose: face.panose,
                            sha256: sha.clone(),
                            postscript_name: face.postscript_name.clone(),
                            duplicates: vec![],
                            has_conflicts: false,
                        });

                    f.active |= entry.active;
                    if !f.files.contains(&file) {
                        f.files.push(file.clone());
                    }

                    // Merge scripts & OpenType features across all faces in the family.
                    let mut script_set: BTreeSet<String> = f.scripts.iter().cloned().collect();
                    script_set.extend(face.scripts.iter().cloned());
                    f.scripts = script_set.into_iter().collect();

                    let mut feat_set: BTreeSet<String> =
                        f.opentype_features.iter().cloned().collect();
                    feat_set.extend(face.opentype_features.iter().cloned());
                    f.opentype_features = feat_set.into_iter().collect();

                    if !face.axes.is_empty() {
                        f.is_variable = true;
                        for ax in &face.axes {
                            if !f.axes.iter().any(|existing| existing.tag == ax.tag) {
                                f.axes.push(ax.clone());
                            }
                        }
                    }

                    if f.foundry.is_none() {
                        f.foundry = face.foundry.clone();
                    }
                    if f.designer.is_none() {
                        f.designer = face.designer.clone();
                    }
                    if f.license.is_none() {
                        f.license = face.license.clone();
                    }
                    if f.license_url.is_none() {
                        f.license_url = face.license_url.clone();
                    }
                    if f.license_tier == LicenseTier::Unknown
                        && face.license_tier != LicenseTier::Unknown
                    {
                        f.license_tier = face.license_tier;
                    }
                    if f.panose == [0; 10] && face.panose != [0; 10] {
                        f.panose = face.panose;
                    }

                    let normal = !face.italic && face.weight == 400;
                    if normal && !f.styles.iter().any(|s| !s.italic && s.weight == 400) {
                        f.file = file.clone();
                        f.category = face.category.clone();
                        f.sha256 = sha.clone();
                        f.postscript_name = face.postscript_name.clone();
                        if face.panose != [0; 10] {
                            f.panose = face.panose;
                        }
                    }

                    let ext = entry
                        .path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_ascii_lowercase())
                        .unwrap_or_default();
                    let dir_key = entry.path.parent().map(path_key).unwrap_or_default();
                    let style_sig = format!(
                        "{}::{}::{}::{}::{}::{}::{}",
                        fam_key,
                        face.win_family.to_lowercase(),
                        face.postscript_name.to_lowercase(),
                        face.style.to_lowercase(),
                        face.weight,
                        face.italic,
                        ext
                    );
                    if let Some((prev_file_k, prev_dir_k, prev_origin, prev_sha)) =
                        seen_styles.get(&style_sig)
                    {
                        if prev_file_k != &file_k {
                            let is_exact = is_exact_dup || prev_sha == &sha;
                            // Different formats (.otf vs .ttf) already have distinct `style_sig`s;
                            // companion builds in the same folder & origin with different filenames/hashes
                            // (e.g. Lato vs LatoWeb) are not flagged as cross-folder/cross-origin collisions.
                            if is_exact || prev_origin != &entry.origin || prev_dir_k != &dir_key {
                                let kind = if is_exact { "exact" } else { "conflict" };
                                if !f
                                    .duplicates
                                    .iter()
                                    .any(|d| path_key(Path::new(&d.path)) == file_k)
                                {
                                    f.duplicates.push(DuplicateEntry {
                                        path: file.clone(),
                                        kind: kind.into(),
                                        sha256: sha.clone(),
                                        origin: entry.origin,
                                    });
                                    f.has_conflicts = true;
                                }
                            }
                        }
                    } else {
                        seen_styles.insert(
                            style_sig,
                            (file_k.clone(), dir_key, entry.origin, sha.clone()),
                        );
                        if is_exact_dup
                            && !f
                                .duplicates
                                .iter()
                                .any(|d| path_key(Path::new(&d.path)) == file_k)
                        {
                            f.duplicates.push(DuplicateEntry {
                                path: file.clone(),
                                kind: "exact".into(),
                                sha256: sha.clone(),
                                origin: entry.origin,
                            });
                            f.has_conflicts = true;
                        }
                    }

                    let already_has_style = f.styles.iter().any(|s| {
                        s.name == face.style && s.weight == face.weight && s.italic == face.italic
                    });
                    if !already_has_style {
                        f.styles.push(FontStyle {
                            name: face.style,
                            weight: face.weight,
                            italic: face.italic,
                            file: file.clone(),
                            win_family: face.win_family,
                            full_name: face.full_name,
                            postscript_name: face.postscript_name,
                            axes: face.axes,
                            opentype_features: face.opentype_features,
                        });
                    }
                }
            }
        }
    }

    let mut out: Vec<FontFamily> = by_family.into_values().collect();
    for fam in &mut out {
        fam.styles
            .sort_by_key(|s| (s.italic, (s.weight as i32 - 400).abs(), s.weight));
        // Deduplicate file list while preserving order.
        let mut seen = HashSet::new();
        fam.files.retain(|p| seen.insert(p.to_lowercase()));
    }
    out.sort_by_key(|f| f.name.to_lowercase());
    out
}
