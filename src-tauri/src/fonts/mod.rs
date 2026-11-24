//! Windows font inventory: combines `cosmic_core` pure-Rust parsing/caching with
//! Windows Registry (`HKLM`/`HKCU`), TrustedInstaller/Office detection, and deactivation stores.

pub mod changes;
pub mod registry;
pub mod system;

pub use cosmic_core::{cache, category, inventory, reader};
pub use inventory::{
    is_essential, is_font_file, list_fonts, path_key, summarize_diagnostics, DuplicateEntry,
    FontEntry, FontFamily, FontStyle, InventoryDiagnostics, Origin,
};

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// Execution environment paths and OS hooks. Overridden with temp folders & test registry keys in tests.
#[derive(Clone, Debug)]
pub struct Environment {
    pub system_fonts: PathBuf,
    pub user_fonts: PathBuf,
    pub system_key: String,
    pub user_key: String,
    pub data_dir: PathBuf,
    /// Disabled in unit tests (`false` skips `AddFontResourceExW` and `WM_FONTCHANGE`).
    pub touch_windows: bool,
    /// Predicate deciding whether a system font file belongs to Windows (`TrustedInstaller`).
    pub is_windows_owned: fn(&Path) -> bool,
    /// Lowercase filenames installed by Click-to-Run Microsoft Office (treated as system-owned).
    pub app_bundled_fonts: HashSet<String>,
}

impl Environment {
    pub fn real(data_dir: PathBuf) -> Self {
        use windows_sys::Win32::UI::Shell::{FOLDERID_Fonts, FOLDERID_LocalAppData};
        let system_fonts = crate::win::known_folder(&FOLDERID_Fonts).unwrap_or_else(|| {
            std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
                .join("Fonts")
        });
        let local = crate::win::known_folder(&FOLDERID_LocalAppData)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
            .unwrap_or_default();
        Environment {
            system_fonts,
            user_fonts: local.join(r"Microsoft\Windows\Fonts"),
            system_key: registry::FONTS_KEY.into(),
            user_key: registry::FONTS_KEY.into(),
            data_dir,
            touch_windows: true,
            is_windows_owned: windows_owner_check,
            app_bundled_fonts: office_bundled_fonts(),
        }
    }

    pub fn deactivated_dir(&self) -> PathBuf {
        self.data_dir.join("deactivated")
    }

    pub fn system_backup_dir(&self) -> PathBuf {
        self.data_dir.join("system-backup")
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.data_dir.join("quarantine")
    }
}

/// A deactivated font file moved out of Windows into Cosmic Font Foundry's managed store.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedFont {
    pub file: PathBuf,
    pub original: PathBuf,
    pub value: String,
    pub data: String,
    pub origin: Origin,
}

fn read_json<T: for<'a> Deserialize<'a> + Default>(path: &Path) -> T {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, val: &T) -> Result<(), String> {
    inventory::write_json_atomic(path, val)
}

pub fn load_deactivated(env: &Environment) -> Vec<SavedFont> {
    read_json(&env.data_dir.join("deactivated.json"))
}

pub fn save_deactivated(env: &Environment, list: &[SavedFont]) -> Result<(), String> {
    write_json(&env.data_dir.join("deactivated.json"), &list)
}

pub fn load_system_backups(env: &Environment) -> Vec<SavedFont> {
    read_json(&env.data_dir.join("system-backup.json"))
}

pub fn save_system_backups(env: &Environment, list: &[SavedFont]) -> Result<(), String> {
    write_json(&env.data_dir.join("system-backup.json"), &list)
}

pub fn windows_owner_check(path: &Path) -> bool {
    crate::win::is_trusted_installer(path).unwrap_or(true)
}

pub fn load_recorded_windows_fonts(env: &Environment) -> HashSet<String> {
    read_json(&env.data_dir.join("windows-fonts.json"))
}

pub fn save_recorded_windows_fonts(env: &Environment, set: &HashSet<String>) -> Result<(), String> {
    write_json(&env.data_dir.join("windows-fonts.json"), set)
}

fn office_bundled_fonts() -> HashSet<String> {
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86};
    let mut set = HashSet::new();
    for id in [&FOLDERID_ProgramFiles, &FOLDERID_ProgramFilesX86] {
        let Some(pf) = crate::win::known_folder(id) else {
            continue;
        };
        for path in list_fonts(&pf.join(r"Microsoft Office\root\vfs\Fonts\private"), false) {
            if let Some(name) = path.file_name() {
                set.insert(name.to_string_lossy().to_lowercase());
            }
        }
    }
    set
}

pub fn origin_in_system_dir(
    env: &Environment,
    path: &Path,
    app_installed: &HashSet<String>,
    recorded_win: &HashSet<String>,
) -> Origin {
    let key = path_key(path);
    if app_installed.contains(&key) {
        Origin::Library
    } else if recorded_win.contains(&key)
        || (env.is_windows_owned)(path)
        || is_bundled_app_font(env, path)
    {
        Origin::System
    } else {
        Origin::User
    }
}

fn is_bundled_app_font(env: &Environment, path: &Path) -> bool {
    path.file_name().is_some_and(|n| {
        env.app_bundled_fonts
            .contains(&n.to_string_lossy().to_lowercase())
    })
}

pub fn load_app_installed(env: &Environment) -> HashSet<String> {
    read_json(&env.data_dir.join("app-installed.json"))
}

pub fn save_app_installed(env: &Environment, set: &HashSet<String>) -> Result<(), String> {
    write_json(&env.data_dir.join("app-installed.json"), set)
}

pub fn load_install_records(env: &Environment) -> std::collections::HashMap<String, String> {
    read_json(&env.data_dir.join("install-records.json"))
}

pub fn save_install_records(
    env: &Environment,
    map: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    write_json(&env.data_dir.join("install-records.json"), map)
}

fn resolve_reg_path(data: &str, dir: &Path) -> PathBuf {
    let p = PathBuf::from(data);
    if p.is_absolute() {
        p
    } else {
        dir.join(p)
    }
}

/// Discovers all font entries across HKLM, HKCU, font directories, deactivated backups, and watched folders.
pub fn collect_entries(env: &Environment, watched_folders: &[PathBuf]) -> Vec<FontEntry> {
    let mut out: Vec<FontEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let app_installed = load_app_installed(env);
    let recorded_win = load_recorded_windows_fonts(env);

    for saved in load_system_backups(env) {
        seen.insert(path_key(&saved.original));
    }

    let mut push = |entry: FontEntry, list: &mut Vec<FontEntry>| {
        if seen.insert(path_key(&entry.path)) {
            list.push(entry);
        }
    };

    for (name, data) in registry::read_values(registry::hklm(), &env.system_key) {
        let path = resolve_reg_path(&data, &env.system_fonts);
        if is_font_file(&path) && path.is_file() {
            let origin = origin_in_system_dir(env, &path, &app_installed, &recorded_win);
            push(
                FontEntry {
                    path,
                    origin,
                    active: true,
                    value: Some((name, data)),
                },
                &mut out,
            );
        }
    }
    for path in list_fonts(&env.system_fonts, false) {
        let origin = origin_in_system_dir(env, &path, &app_installed, &recorded_win);
        push(
            FontEntry {
                path,
                origin,
                active: true,
                value: None,
            },
            &mut out,
        );
    }

    for (name, data) in registry::read_values(registry::hkcu(), &env.user_key) {
        let path = resolve_reg_path(&data, &env.user_fonts);
        if is_font_file(&path) && path.is_file() {
            let origin = if app_installed.contains(&path_key(&path)) {
                Origin::Library
            } else {
                Origin::User
            };
            push(
                FontEntry {
                    path,
                    origin,
                    active: true,
                    value: Some((name, data)),
                },
                &mut out,
            );
        }
    }
    for path in list_fonts(&env.user_fonts, false) {
        let origin = if app_installed.contains(&path_key(&path)) {
            Origin::Library
        } else {
            Origin::User
        };
        push(
            FontEntry {
                path,
                origin,
                active: false,
                value: None,
            },
            &mut out,
        );
    }

    for saved in load_deactivated(env)
        .into_iter()
        .chain(load_system_backups(env))
    {
        if saved.file.is_file() {
            push(
                FontEntry {
                    path: saved.file,
                    origin: saved.origin,
                    active: false,
                    value: Some((saved.value, saved.data)),
                },
                &mut out,
            );
        }
    }

    for folder in watched_folders {
        for path in list_fonts(folder, true) {
            push(
                FontEntry {
                    path,
                    origin: Origin::Folder,
                    active: true,
                    value: None,
                },
                &mut out,
            );
        }
    }

    out
}
