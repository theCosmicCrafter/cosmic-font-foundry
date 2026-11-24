//! Central Font Vault management: storage capacity stats, SHA-256 deduplicated import,
//! and family-organized archival outside the Windows Fonts folder.

use crate::fonts::{self, is_font_file, list_fonts, path_key, reader, Environment};
use cosmic_core::sync::extract_zip_fonts;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct VaultConfig {
    path: PathBuf,
}

#[derive(Serialize, Clone, Debug)]
pub struct VaultStats {
    pub path: String,
    pub exists: bool,
    pub total_files: usize,
    pub total_families: usize,
    pub total_bytes: u64,
    pub disk_free_bytes: Option<u64>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct VaultImportReport {
    pub vault_path: String,
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub recycled: usize,
    pub bytes_copied: u64,
    pub errors: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct VaultDuplicateCandidate {
    pub source_path: String,
    pub vault_path: String,
    pub family: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct VaultCleanupScan {
    pub scanned_roots: Vec<String>,
    pub candidates: Vec<VaultDuplicateCandidate>,
    pub total_reclaimable_bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct RecycleReport {
    pub recycled: usize,
    pub reclaimed_bytes: u64,
    pub skipped_protected: usize,
    pub errors: Vec<String>,
}

pub fn default_vault_dir(data_dir: &Path) -> PathBuf {
    if let Some(home) = std::env::var_os("USERPROFILE").filter(|s| !s.is_empty()) {
        return PathBuf::from(home)
            .join("Cosmic Font Foundry")
            .join("Vault");
    }
    data_dir.join("Vault")
}

pub fn load_vault_path(data_dir: &Path) -> PathBuf {
    let cfg_path = data_dir.join("vault-config.json");
    if let Ok(bytes) = fs::read(&cfg_path) {
        if let Ok(cfg) = serde_json::from_slice::<VaultConfig>(&bytes) {
            if !cfg.path.as_os_str().is_empty() {
                return cfg.path;
            }
        }
    }
    default_vault_dir(data_dir)
}

pub fn save_vault_path(data_dir: &Path, path: &Path) -> Result<(), String> {
    let cfg = VaultConfig {
        path: path.to_path_buf(),
    };
    cosmic_core::inventory::write_json_atomic(&data_dir.join("vault-config.json"), &cfg)
}

fn sanitize_folder_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "Uncategorized".into()
    } else {
        trimmed.to_string()
    }
}

fn unique_dest(dir: &Path, filename: &str) -> PathBuf {
    cosmic_core::inventory::unique_path(dir, filename)
}

#[cfg(windows)]
fn disk_free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    let wide: Vec<u16> = probe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut avail: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut avail,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        Some(avail)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn disk_free_bytes(_path: &Path) -> Option<u64> {
    None
}

pub fn compute_vault_stats(vault_dir: &Path) -> VaultStats {
    compute_vault_stats_cached(vault_dir, None)
}

pub fn compute_vault_stats_cached(
    vault_dir: &Path,
    cache: Option<&crate::fonts::cache::Cache>,
) -> VaultStats {
    let exists = vault_dir.is_dir();
    let mut total_files = 0usize;
    let mut total_bytes = 0u64;
    let mut families: HashSet<String> = HashSet::new();

    if exists {
        for file in list_fonts(vault_dir, true) {
            total_files += 1;
            if let Ok(meta) = fs::metadata(&file) {
                total_bytes = total_bytes.saturating_add(meta.len());
            }
            let cached_family = cache
                .and_then(|c| c.faces(&file))
                .and_then(|faces| faces.into_iter().next())
                .map(|f| f.family);
            if let Some(fam) = cached_family {
                families.insert(fam.to_lowercase());
            } else if let Ok(faces) = reader::read_faces(&file) {
                if let Some(first) = faces.first() {
                    families.insert(first.family.to_lowercase());
                }
            }
        }
    }

    VaultStats {
        path: vault_dir.to_string_lossy().into_owned(),
        exists,
        total_files,
        total_families: families.len(),
        total_bytes,
        disk_free_bytes: disk_free_bytes(vault_dir),
    }
}

pub fn import_to_vault(
    vault_dir: &Path,
    sources: &[PathBuf],
    move_files: bool,
) -> Result<VaultImportReport, String> {
    import_to_vault_with_options(vault_dir, sources, move_files, false, None, &|_| {
        Err("recycle not enabled".into())
    })
}

/// Checks whether a path is inside an OS, Windows Fonts, application-critical, Vault, or active project folder.
pub fn is_protected_path(path: &Path, vault_dir: &Path, env: Option<&Environment>) -> bool {
    let k = path_key(path);
    let vault_k = path_key(vault_dir);
    if k == vault_k || k.starts_with(&format!("{vault_k}\\")) {
        return true;
    }

    if let Some(e) = env {
        for protected in [&e.system_fonts, &e.user_fonts, &e.data_dir] {
            let pk = path_key(protected);
            if !pk.is_empty() && (k == pk || k.starts_with(&format!("{pk}\\"))) {
                return true;
            }
        }
    }

    for var in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
        if let Some(val) = std::env::var_os(var).filter(|s| !s.is_empty()) {
            let pk = path_key(Path::new(&val));
            if k == pk || k.starts_with(&format!("{pk}\\")) {
                return true;
            }
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|s| !s.is_empty()) {
        for sub in [r"Microsoft\Windows\Fonts", "Programs"] {
            let pk = path_key(&PathBuf::from(&local).join(sub));
            if k == pk || k.starts_with(&format!("{pk}\\")) {
                return true;
            }
        }
    }

    let mut cur = path.parent();
    while let Some(dir) = cur {
        if dir.join(".cosmicfonts.json").is_file() {
            return true;
        }
        if dir
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("document fonts"))
        {
            return true;
        }
        cur = dir.parent();
    }
    false
}

pub fn default_cleanup_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").filter(|s| !s.is_empty()) {
        let base = PathBuf::from(home);
        for sub in ["Downloads", "Desktop", "Documents"] {
            let p = base.join(sub);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots
}

pub fn is_zip_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

fn list_zips(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(it) = fs::read_dir(&d) else { continue };
        for entry in it.flatten() {
            let p = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(t) if t.is_file() && is_zip_file(&p) => out.push(p),
                _ => {}
            }
        }
    }
    out
}

/// Expands folders, `.zip` archives, and loose font files into `(path, from_zip)` pairs.
/// The returned `TempDir` must be kept alive until callers finish reading extracted files.
pub fn expand_font_and_zip_sources(
    sources: &[PathBuf],
) -> (Vec<(PathBuf, bool)>, Option<tempfile::TempDir>, Vec<String>) {
    let mut candidates: Vec<(PathBuf, bool)> = Vec::new();
    let mut zip_paths: Vec<PathBuf> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for src in sources {
        if src.is_dir() {
            candidates.extend(list_fonts(src, true).into_iter().map(|p| (p, false)));
            zip_paths.extend(list_zips(src));
        } else if src.is_file() && is_zip_file(src) {
            zip_paths.push(src.clone());
        } else if src.is_file() && is_font_file(src) {
            candidates.push((src.clone(), false));
        }
    }

    let mut temp_guard: Option<tempfile::TempDir> = None;
    for (zip_idx, zpath) in zip_paths.iter().enumerate() {
        let zname = zpath
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive.zip".into());
        let bytes = match fs::read(zpath) {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{zname}: {e}"));
                continue;
            }
        };
        let extracted = match extract_zip_fonts(&bytes) {
            Ok(list) => list,
            Err(e) => {
                errors.push(format!("{zname}: {e}"));
                continue;
            }
        };
        if extracted.is_empty() {
            continue;
        }
        if temp_guard.is_none() {
            match tempfile::tempdir() {
                Ok(dir) => temp_guard = Some(dir),
                Err(e) => {
                    errors.push(format!("{zname}: {e}"));
                    break;
                }
            }
        }
        let Some(root) = temp_guard.as_ref().map(|d| d.path()) else {
            continue;
        };
        for (entry_idx, (fname, font_bytes)) in extracted.into_iter().enumerate() {
            let entry_dir = root.join(format!("z{zip_idx}_{entry_idx}"));
            if let Err(e) = fs::create_dir_all(&entry_dir) {
                errors.push(format!("{zname}/{fname}: {e}"));
                continue;
            }
            let out_path = entry_dir.join(&fname);
            match fs::write(&out_path, &font_bytes) {
                Ok(()) => candidates.push((out_path, true)),
                Err(e) => errors.push(format!("{zname}/{fname}: {e}")),
            }
        }
    }

    (candidates, temp_guard, errors)
}

pub fn import_to_vault_with_options(
    vault_dir: &Path,
    sources: &[PathBuf],
    move_files: bool,
    recycle_originals: bool,
    env: Option<&Environment>,
    recycle_fn: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<VaultImportReport, String> {
    fs::create_dir_all(vault_dir).map_err(|e| e.to_string())?;

    let mut existing_hashes: HashSet<String> = HashSet::new();
    for existing in list_fonts(vault_dir, true) {
        if let Ok(sha) = reader::sha256_file(&existing) {
            existing_hashes.insert(sha);
        }
    }

    let (candidates, _zip_tmp, mut expand_errors) = expand_font_and_zip_sources(sources);

    let mut report = VaultImportReport {
        vault_path: vault_dir.to_string_lossy().into_owned(),
        ..Default::default()
    };
    report.errors.append(&mut expand_errors);

    for (src, from_zip) in candidates {
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "font".into());
        let sha = match reader::sha256_file(&src) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        if existing_hashes.contains(&sha) {
            report.skipped_duplicates += 1;
            if recycle_originals && !from_zip && !is_protected_path(&src, vault_dir, env) {
                match recycle_fn(&src) {
                    Ok(()) => report.recycled += 1,
                    Err(e) => report.errors.push(format!("{name}: {e}")),
                }
            }
            continue;
        }
        let faces = match reader::read_faces(&src) {
            Ok(f) => f,
            Err(e) => {
                report.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        let family_dir = vault_dir.join(sanitize_folder_name(&faces[0].family));
        if let Err(e) = fs::create_dir_all(&family_dir) {
            report.errors.push(format!("{name}: {e}"));
            continue;
        }
        let dest = unique_dest(&family_dir, &name);
        let size = fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        let transfer = if move_files && !from_zip {
            fonts::changes::move_file(&src, &dest)
        } else {
            fs::copy(&src, &dest).map(|_| ()).map_err(|e| e.to_string())
        };
        match transfer {
            Ok(()) => {
                existing_hashes.insert(sha.clone());
                report.imported += 1;
                report.bytes_copied = report.bytes_copied.saturating_add(size);
                if recycle_originals
                    && !from_zip
                    && !move_files
                    && !is_protected_path(&src, vault_dir, env)
                {
                    // Verify Vault copy hash matches before sending original to Recycle Bin.
                    match reader::sha256_file(&dest) {
                        Ok(dest_sha) if dest_sha == sha => match recycle_fn(&src) {
                            Ok(()) => report.recycled += 1,
                            Err(e) => report.errors.push(format!("{name}: {e}")),
                        },
                        Ok(_) => report.errors.push(format!(
                            "{name}: Vault verification mismatch; original kept"
                        )),
                        Err(e) => report.errors.push(format!("{name}: {e}")),
                    }
                }
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }

    Ok(report)
}

fn index_vault_hashes(vault_dir: &Path) -> HashMap<String, (PathBuf, String)> {
    let mut map = HashMap::new();
    if !vault_dir.is_dir() {
        return map;
    }
    for file in list_fonts(vault_dir, true) {
        let Ok(sha) = reader::sha256_file(&file) else {
            continue;
        };
        let family = reader::read_faces(&file)
            .ok()
            .and_then(|f| f.into_iter().next())
            .map(|f| f.family)
            .unwrap_or_else(|| "Unknown".into());
        map.entry(sha).or_insert((file, family));
    }
    map
}

pub fn scan_vault_duplicates(
    vault_dir: &Path,
    roots: &[PathBuf],
    env: Option<&Environment>,
) -> VaultCleanupScan {
    let vault_map = index_vault_hashes(vault_dir);
    let mut scan = VaultCleanupScan {
        scanned_roots: roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        ..Default::default()
    };
    if vault_map.is_empty() {
        return scan;
    }

    let mut seen_paths = HashSet::new();
    for root in roots {
        if !root.is_dir() || is_protected_path(root, vault_dir, env) {
            continue;
        }
        for file in list_fonts(root, true) {
            if is_protected_path(&file, vault_dir, env) || !seen_paths.insert(path_key(&file)) {
                continue;
            }
            let Ok(sha) = reader::sha256_file(&file) else {
                continue;
            };
            if let Some((vault_path, family)) = vault_map.get(&sha) {
                let size_bytes = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                scan.total_reclaimable_bytes =
                    scan.total_reclaimable_bytes.saturating_add(size_bytes);
                scan.candidates.push(VaultDuplicateCandidate {
                    source_path: file.to_string_lossy().into_owned(),
                    vault_path: vault_path.to_string_lossy().into_owned(),
                    family: family.clone(),
                    sha256: sha,
                    size_bytes,
                });
            }
        }
    }
    scan
}

pub fn recycle_vault_duplicates(
    vault_dir: &Path,
    paths: &[PathBuf],
    env: Option<&Environment>,
    recycle_fn: &dyn Fn(&Path) -> Result<(), String>,
) -> RecycleReport {
    let vault_map = index_vault_hashes(vault_dir);
    let mut report = RecycleReport::default();

    for p in paths {
        if is_protected_path(p, vault_dir, env) {
            report.skipped_protected += 1;
            continue;
        }
        if !p.is_file() {
            continue;
        }
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "font".into());
        let sha = match reader::sha256_file(p) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        if !vault_map.contains_key(&sha) {
            report
                .errors
                .push(format!("{name}: not verified in Vault; kept"));
            continue;
        }
        let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        match recycle_fn(p) {
            Ok(()) => {
                report.recycled += 1;
                report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(size);
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }
    report
}
