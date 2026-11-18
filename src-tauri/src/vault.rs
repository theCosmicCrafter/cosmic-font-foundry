//! Central Font Vault management: storage capacity stats, SHA-256 deduplicated import,
//! and family-organized archival outside the Windows Fonts folder.

use crate::fonts::{self, is_font_file, list_fonts, reader};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
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
    pub bytes_copied: u64,
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
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let cfg = VaultConfig {
        path: path.to_path_buf(),
    };
    let tmp = data_dir.join("vault-config.tmp");
    let target = data_dir.join("vault-config.json");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &target).map_err(|e| e.to_string())
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
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("font");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 1..10_000 {
        let name = if ext.is_empty() {
            format!("{stem}-{i}")
        } else {
            format!("{stem}-{i}.{ext}")
        };
        let next = dir.join(name);
        if !next.exists() {
            return next;
        }
    }
    dir.join(filename)
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
            if let Ok(faces) = reader::read_faces(&file) {
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
    fs::create_dir_all(vault_dir).map_err(|e| e.to_string())?;

    let mut existing_hashes: HashSet<String> = HashSet::new();
    for existing in list_fonts(vault_dir, true) {
        if let Ok(sha) = reader::sha256_file(&existing) {
            existing_hashes.insert(sha);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    for src in sources {
        if src.is_dir() {
            candidates.extend(list_fonts(src, true));
        } else if src.is_file() && is_font_file(src) {
            candidates.push(src.clone());
        }
    }

    let mut report = VaultImportReport {
        vault_path: vault_dir.to_string_lossy().into_owned(),
        ..Default::default()
    };

    for src in candidates {
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
        let transfer = if move_files {
            fonts::changes::move_file(&src, &dest)
        } else {
            fs::copy(&src, &dest).map(|_| ()).map_err(|e| e.to_string())
        };
        match transfer {
            Ok(()) => {
                existing_hashes.insert(sha);
                report.imported += 1;
                report.bytes_copied = report.bytes_copied.saturating_add(size);
            }
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }

    Ok(report)
}
