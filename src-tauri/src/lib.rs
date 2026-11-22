mod agent;
pub mod fonts;
mod google;
mod startup;
mod tray;
pub mod vault;
pub mod win;

use cosmic_core::{migrate, similarity, sync};
use fonts::{
    cache::Cache, changes, summarize_diagnostics, Environment, FontFamily, InventoryDiagnostics,
    Origin,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

const SETTINGS_FILE: &str = "settings.json";
const LIBRARY_FILE: &str = "library.json";

/// Cosmic Font Foundry window backgrounds (matched to `--win` in `src/app.css` to eliminate startup flash).
const BG_LIGHT: (u8, u8, u8, u8) = (0xfd, 0xf9, 0xf4, 255);
const BG_DARK: (u8, u8, u8, u8) = (0x09, 0x08, 0x0c, 255);

fn err(e: impl ToString) -> String {
    e.to_string()
}

pub(crate) struct AppState {
    pub(crate) env: Environment,
    pub(crate) cache: Mutex<Cache>,
    pub(crate) folders: Mutex<Vec<PathBuf>>,
    loaded_folder_fonts: Mutex<HashSet<String>>,
    pub(crate) disabled_folder_fonts: Mutex<HashSet<String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    scan_cancel: Arc<AtomicBool>,
    ai_cancel: Arc<AtomicBool>,
    ai_pull_cancel: Arc<AtomicBool>,
}

impl AppState {
    fn cache_path(&self) -> PathBuf {
        self.env.data_dir.join("fonts-cache.json")
    }

    fn folders_path(&self) -> PathBuf {
        self.env.data_dir.join("folders.json")
    }

    fn collect_entries(&self) -> Vec<fonts::FontEntry> {
        let folders = self.folders.lock().unwrap().clone();
        let disabled = self.disabled_folder_fonts.lock().unwrap();
        let mut list = fonts::collect_entries(&self.env, &folders);
        for entry in list.iter_mut().filter(|e| e.origin == Origin::Folder) {
            entry.active = !disabled.contains(&fonts::path_key(&entry.path));
        }
        list
    }

    fn refresh(
        &self,
    ) -> (
        Vec<FontFamily>,
        usize,
        InventoryDiagnostics,
        Vec<sync::DetectedProject>,
    ) {
        let entries = self.collect_entries();
        let mut cache = self.cache.lock().unwrap();
        let parsed = cache.update(&entries);
        cache.save(&self.cache_path());
        let families = cache.families.clone();
        drop(cache);
        self.sync_watched_folders(&entries);
        let diagnostics = summarize_diagnostics(&families);
        let folders = self.folders.lock().unwrap().clone();
        let projects = sync::scan_project_manifests(&folders);
        (families, parsed, diagnostics, projects)
    }

    fn sync_watched_folders(&self, entries: &[fonts::FontEntry]) {
        let mut loaded = self.loaded_folder_fonts.lock().unwrap();
        let wanted: HashSet<String> = entries
            .iter()
            .filter(|e| e.origin == Origin::Folder && e.active)
            .map(|e| fonts::path_key(&e.path))
            .collect();
        let mut changed = false;
        for entry in entries.iter().filter(|e| e.origin == Origin::Folder) {
            let key = fonts::path_key(&entry.path);
            if wanted.contains(&key) && !loaded.contains(&key) && win::load_font(&entry.path, false)
            {
                loaded.insert(key);
                changed = true;
            }
        }
        for key in loaded
            .iter()
            .filter(|k| !wanted.contains(*k))
            .cloned()
            .collect::<Vec<_>>()
        {
            win::unload_font(Path::new(&key), false);
            loaded.remove(&key);
            changed = true;
        }
        if changed {
            win::broadcast_font_change();
        }
    }

    fn unload_all_folder_fonts(&self) {
        let mut loaded = self.loaded_folder_fonts.lock().unwrap();
        for key in loaded.drain() {
            win::unload_font(Path::new(&key), false);
        }
        win::broadcast_font_change();
    }
}

async fn bg<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[derive(Serialize)]
pub(crate) struct ToggleResponse {
    pub(crate) families: Vec<FontFamily>,
    pub(crate) outcome: changes::ToggleOutcome,
    pub(crate) diagnostics: InventoryDiagnostics,
}

#[tauri::command]
fn saved_fonts(state: State<AppState>) -> Vec<FontFamily> {
    state.cache.lock().unwrap().families.clone()
}

#[tauri::command]
fn read_library_store(app: AppHandle) -> sync::LibraryState {
    let Ok(dir) = app.path().app_data_dir() else {
        return sync::LibraryState::default();
    };
    fs::read(dir.join(LIBRARY_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[tauri::command]
async fn refresh_fonts(app: AppHandle) -> Result<Value, String> {
    bg(move || {
        let (families, parsed, diagnostics, projects) = app.state::<AppState>().refresh();
        Ok(json!({
            "families": families,
            "parsed": parsed,
            "diagnostics": diagnostics,
            "projects": projects
        }))
    })
    .await
}

pub(crate) fn toggle_families_sync(
    app: &AppHandle,
    families: Vec<String>,
    activate: bool,
    system_unlocked: bool,
) -> Result<ToggleResponse, String> {
    let state = app.state::<AppState>();
    let entries = state.collect_entries();
    let outcome = {
        let mut cache = state.cache.lock().unwrap();
        cache.update(&entries);
        let mut disabled = state.disabled_folder_fonts.lock().unwrap();
        changes::toggle_families(
            &state.env,
            &entries,
            &cache,
            &families,
            activate,
            system_unlocked,
            &mut disabled,
        )
    };
    let (updated_families, _, diagnostics, _) = state.refresh();
    let resp = ToggleResponse {
        families: updated_families,
        outcome,
        diagnostics,
    };
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

#[tauri::command]
async fn toggle_fonts(
    app: AppHandle,
    families: Vec<String>,
    activate: bool,
    system_unlocked: bool,
) -> Result<ToggleResponse, String> {
    bg(move || toggle_families_sync(&app, families, activate, system_unlocked)).await
}

#[tauri::command]
async fn install_fonts(
    app: AppHandle,
    paths: Vec<String>,
    install_for_all: bool,
) -> Result<ToggleResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            let (expanded, _zip_tmp, mut expand_errors) =
                vault::expand_font_and_zip_sources(&path_bufs);
            let expanded_paths: Vec<PathBuf> = expanded.into_iter().map(|(p, _)| p).collect();
            let mut outcome = install_files(&state, &expanded_paths, install_for_all);
            outcome.errors.append(&mut expand_errors);
            let (families, _, diagnostics, _) = state.refresh();
            Ok(ToggleResponse {
                families,
                outcome,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

#[derive(Serialize, Clone)]
struct VaultImportResponse {
    report: vault::VaultImportReport,
    stats: vault::VaultStats,
    folders: Vec<FolderInfo>,
    families: Vec<FontFamily>,
    diagnostics: InventoryDiagnostics,
}

fn ensure_vault_watched(app: &AppHandle, state: &AppState, vault_path: &Path) {
    let mut changed = false;
    {
        let mut folders = state.folders.lock().unwrap();
        if !folders
            .iter()
            .any(|x| fonts::path_key(x) == fonts::path_key(vault_path))
        {
            folders.push(vault_path.to_path_buf());
            changed = true;
        }
    }
    let _ = app.asset_protocol_scope().allow_directory(vault_path, true);
    if changed {
        save_folders(state);
        watch_folders(app);
    }
}

#[tauri::command]
async fn vault_stats(app: AppHandle) -> Result<vault::VaultStats, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let vault_path = vault::load_vault_path(&state.env.data_dir);
        Ok(vault::compute_vault_stats(&vault_path))
    })
    .await
}

#[tauri::command]
async fn choose_vault_folder(app: AppHandle) -> Result<Option<vault::VaultStats>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    bg(move || {
        let state = app.state::<AppState>();
        fs::create_dir_all(&path).map_err(err)?;
        vault::save_vault_path(&state.env.data_dir, &path)?;
        ensure_vault_watched(&app, &state, &path);
        let (families, _, _, _) = state.refresh();
        let _ = app.emit("inventory-updated", &families);
        Ok(Some(vault::compute_vault_stats(&path)))
    })
    .await
}

#[tauri::command]
async fn import_folder_to_vault(
    app: AppHandle,
    move_files: bool,
    recycle_originals: Option<bool>,
) -> Result<Option<VaultImportResponse>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let source_dir = picked.into_path().map_err(err)?;
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let vault_path = vault::load_vault_path(&state.env.data_dir);
            let report = vault::import_to_vault_with_options(
                &vault_path,
                &[source_dir],
                move_files,
                recycle_originals.unwrap_or(false),
                Some(&state.env),
                &win::recycle_file,
            )?;
            ensure_vault_watched(&app, &state, &vault_path);
            let (families, _, diagnostics, _) = state.refresh();
            let stats = vault::compute_vault_stats(&vault_path);
            let folders = folder_list(&state);
            Ok(VaultImportResponse {
                report,
                stats,
                folders,
                families,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(Some(resp))
}

#[tauri::command]
async fn scan_vault_cleanup(
    app: AppHandle,
    custom_folder: Option<bool>,
) -> Result<Option<vault::VaultCleanupScan>, String> {
    use tauri_plugin_dialog::DialogExt;
    let roots = if custom_folder.unwrap_or(false) {
        let Some(picked) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        vec![picked.into_path().map_err(err)?]
    } else {
        vault::default_cleanup_roots()
    };
    bg(move || {
        let state = app.state::<AppState>();
        let vault_path = vault::load_vault_path(&state.env.data_dir);
        Ok(Some(vault::scan_vault_duplicates(
            &vault_path,
            &roots,
            Some(&state.env),
        )))
    })
    .await
}

#[tauri::command]
async fn recycle_vault_duplicates(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<vault::RecycleReport, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let vault_path = vault::load_vault_path(&state.env.data_dir);
        let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let report = vault::recycle_vault_duplicates(
            &vault_path,
            &path_bufs,
            Some(&state.env),
            &win::recycle_file,
        );
        let (families, _, _, _) = state.refresh();
        let _ = app.emit("inventory-updated", &families);
        Ok(report)
    })
    .await
}

fn collect_family_paths(state: &AppState, families: &[String]) -> Vec<PathBuf> {
    let cache = state.cache.lock().unwrap();
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for name in families {
        if let Some(fam) = cache
            .families
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
        {
            for fpath in &fam.files {
                let p = PathBuf::from(fpath);
                if seen.insert(fonts::path_key(&p)) {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

#[tauri::command]
async fn install_family_to_windows(
    app: AppHandle,
    family: String,
    install_for_all: bool,
) -> Result<ToggleResponse, String> {
    install_families_to_windows(app, vec![family], install_for_all).await
}

#[tauri::command]
async fn install_families_to_windows(
    app: AppHandle,
    families: Vec<String>,
    install_for_all: bool,
) -> Result<ToggleResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let paths = collect_family_paths(&state, &families);
            if paths.is_empty() {
                return Err("No font files found for the selected families".into());
            }
            let outcome = install_files(&state, &paths, install_for_all);
            let (families, _, diagnostics, _) = state.refresh();
            Ok(ToggleResponse {
                families,
                outcome,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

#[tauri::command]
async fn copy_families_to_vault(
    app: AppHandle,
    families: Vec<String>,
) -> Result<VaultImportResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let paths = collect_family_paths(&state, &families);
            if paths.is_empty() {
                return Err("No font files found for the selected families".into());
            }
            let vault_path = vault::load_vault_path(&state.env.data_dir);
            let report = vault::import_to_vault(&vault_path, &paths, false)?;
            ensure_vault_watched(&app, &state, &vault_path);
            let (families, _, diagnostics, _) = state.refresh();
            let stats = vault::compute_vault_stats(&vault_path);
            let folders = folder_list(&state);
            Ok(VaultImportResponse {
                report,
                stats,
                folders,
                families,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

fn install_files(
    state: &AppState,
    paths: &[PathBuf],
    install_for_all: bool,
) -> changes::ToggleOutcome {
    let mut outcome = changes::ToggleOutcome::default();
    let mut ops = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !fonts::is_font_file(path) {
            outcome
                .errors
                .push(format!("{name}: not a supported font file"));
            continue;
        }
        let faces = match fonts::reader::read_faces(path) {
            Ok(f) => f,
            Err(e) => {
                outcome.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        let family = faces[0].family.clone();
        if install_for_all {
            let dest = changes::unique_path(&state.env.system_fonts, &name);
            let is_cff = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("otf"));
            let value = fonts::registry::format_value_name(
                &faces
                    .iter()
                    .map(|f| f.full_name.clone())
                    .collect::<Vec<_>>(),
                is_cff,
            );
            let data = dest
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            ops.push(fonts::system::SystemOp::Install {
                source: path.clone(),
                path: dest,
                value,
                data,
            });
            outcome.installed.push(family);
        } else {
            match changes::install_user(&state.env, path, &faces) {
                Ok(_) => {
                    outcome.changed += 1;
                    if !outcome.installed.contains(&family) {
                        outcome.installed.push(family);
                    }
                }
                Err(e) => outcome.errors.push(format!("{name}: {e}")),
            }
        }
    }
    if !ops.is_empty() {
        let before = outcome.changed;
        changes::apply_system_ops(&state.env, &ops, &mut outcome);
        if outcome.changed == before {
            outcome.installed.clear();
        }
    }
    if outcome.changed > 0 {
        win::broadcast_font_change();
    }
    outcome
}

#[tauri::command]
async fn restore_system_fonts(app: AppHandle) -> Result<ToggleResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let ops: Vec<_> = fonts::load_system_backups(&state.env)
                .into_iter()
                .map(|s| fonts::system::SystemOp::Activate {
                    backup: s.file,
                    path: s.original,
                    value: s.value,
                    data: s.data,
                })
                .collect();
            let mut outcome = changes::ToggleOutcome::default();
            if !ops.is_empty() {
                changes::apply_system_ops(&state.env, &ops, &mut outcome);
                win::broadcast_font_change();
            }
            let (families, _, diagnostics, _) = state.refresh();
            Ok(ToggleResponse {
                families,
                outcome,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

#[tauri::command]
fn backed_up_count(state: State<AppState>) -> usize {
    fonts::load_system_backups(&state.env).len()
}

#[tauri::command]
async fn font_characters(path: String) -> Result<Vec<u32>, String> {
    bg(move || fonts::reader::characters(Path::new(&path), 4000)).await
}

#[tauri::command]
async fn quarantine_font(app: AppHandle, path: String) -> Result<ToggleResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
            let state = app.state::<AppState>();
            let mut outcome = changes::ToggleOutcome::default();
            match changes::quarantine_corrupt_file(&state.env, Path::new(&path)) {
                Ok(_) => {
                    outcome.changed = 1;
                    win::broadcast_font_change();
                }
                Err(e) => outcome.errors.push(e),
            }
            let (families, _, diagnostics, _) = state.refresh();
            Ok(ToggleResponse {
                families,
                outcome,
                diagnostics,
            })
        }
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

// ---- Watched Folders & Project Manifests ----

fn save_folders(state: &AppState) {
    let paths: Vec<String> = state
        .folders
        .lock()
        .unwrap()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let _ = fs::create_dir_all(&state.env.data_dir);
    let _ = fs::write(
        state.folders_path(),
        serde_json::to_vec(&paths).unwrap_or_default(),
    );
}

fn watch_folders(app: &AppHandle) {
    use notify::Watcher;
    let state = app.state::<AppState>();
    let (tx, rx) = mpsc::channel::<()>();
    let watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        if let Ok(ev) = ev {
            if ev.paths.iter().any(|p| {
                fonts::is_font_file(p)
                    || p.file_name().is_some_and(|n| n == ".cosmicfonts.json")
                    || p.extension().is_none()
            }) {
                let _ = tx.send(());
            }
        }
    });
    let Ok(mut w) = watcher else { return };
    for folder in state.folders.lock().unwrap().iter() {
        let _ = w.watch(folder, notify::RecursiveMode::Recursive);
    }
    *state.watcher.lock().unwrap() = Some(w);
    let app = app.clone();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}
            let _ = app.emit("fonts-changed", ());
        }
    });
}

#[derive(Serialize, Clone)]
struct FolderInfo {
    path: String,
    name: String,
}

fn folder_list(state: &AppState) -> Vec<FolderInfo> {
    state
        .folders
        .lock()
        .unwrap()
        .iter()
        .map(|p| {
            let comps: Vec<String> = p
                .components()
                .rev()
                .take(2)
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();
            let name = comps.into_iter().rev().collect::<Vec<_>>().join("\\");
            FolderInfo {
                path: p.to_string_lossy().into_owned(),
                name,
            }
        })
        .collect()
}

#[tauri::command]
fn watched_folders(state: State<AppState>) -> Vec<FolderInfo> {
    folder_list(&state)
}

#[tauri::command]
async fn add_watched_folder(app: AppHandle) -> Result<Option<Vec<FolderInfo>>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    let state = app.state::<AppState>();
    {
        let mut folders = state.folders.lock().unwrap();
        if !folders
            .iter()
            .any(|x| fonts::path_key(x) == fonts::path_key(&path))
        {
            folders.push(path.clone());
        }
    }
    let _ = app.asset_protocol_scope().allow_directory(&path, true);
    save_folders(&state);
    watch_folders(&app);
    Ok(Some(folder_list(&state)))
}

#[tauri::command]
async fn remove_watched_folder(app: AppHandle, path: String) -> Result<Vec<FolderInfo>, String> {
    bg(move || {
        let state = app.state::<AppState>();
        state
            .folders
            .lock()
            .unwrap()
            .retain(|f| fonts::path_key(f) != fonts::path_key(Path::new(&path)));
        save_folders(&state);
        watch_folders(&app);
        let _ = state.refresh();
        Ok(folder_list(&state))
    })
    .await
}

#[tauri::command]
fn project_manifests(state: State<AppState>) -> Vec<sync::DetectedProject> {
    let folders = state.folders.lock().unwrap().clone();
    sync::scan_project_manifests(&folders)
}

#[derive(Serialize, Clone)]
pub(crate) struct DiscoveredFontFolder {
    path: String,
    label: String,
    origin_kind: String,
    font_count: usize,
    families: Vec<String>,
    files: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct OsFontReport {
    os_name: String,
    arch: String,
    system_fonts_dir: String,
    user_fonts_dir: String,
    system_font_count: usize,
    user_font_count: usize,
    discovered_folders: Vec<DiscoveredFontFolder>,
    scanned_dirs: usize,
    cancelled: bool,
}

#[derive(Serialize, Clone)]
struct ScanProgressEvent {
    scanned_dirs: usize,
    current_dir: String,
    found_folder: Option<DiscoveredFontFolder>,
    cancelled: bool,
}

const MAX_SCAN_DIRS: usize = 1500;

fn should_skip_scan_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.')
        || matches!(
            lower.as_str(),
            "node_modules"
                | "target"
                | ".git"
                | "windows"
                | "$recycle.bin"
                | "system volume information"
                | "dist"
                | "build"
                | "out"
                | "venv"
                | "env"
                | "__pycache__"
                | "vendor"
                | "packages"
                | "bin"
                | "obj"
                | "appdata"
                | "temp"
                | "tmp"
                | "cache"
        )
}

fn should_skip_thorough_dir(name: &str, depth: usize) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "$recycle.bin"
            | "system volume information"
            | "$windows.~bt"
            | "$windows.~ws"
            | "recovery"
            | "config.msi"
            | "msocache"
            | "winsxs"
            | ".git"
    ) || (depth == 0 && lower == "windows")
}

fn normalize_scan_root(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        PathBuf::from(format!("{trimmed}\\"))
    } else {
        PathBuf::from(trimmed)
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_dir_for_fonts(
    root: &Path,
    origin_kind: &str,
    max_depth: Option<usize>,
    max_dirs: Option<usize>,
    thorough: bool,
    excluded_roots: &[String],
    known_families: &std::collections::HashMap<String, String>,
    cancel: &AtomicBool,
    scanned_total: &mut usize,
    on_progress: &mut dyn FnMut(usize, &Path, Option<&DiscoveredFontFolder>),
    out: &mut Vec<DiscoveredFontFolder>,
) {
    use std::collections::BTreeSet;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited_dirs = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || max_dirs.is_some_and(|limit| visited_dirs >= limit) {
            break;
        }
        let dir_key = fonts::path_key(&dir);
        if excluded_roots
            .iter()
            .any(|ex| dir_key == *ex || dir_key.starts_with(&format!("{ex}\\")))
        {
            continue;
        }
        visited_dirs += 1;
        *scanned_total += 1;
        let Ok(entries) = fs::read_dir(&dir) else {
            if (*scanned_total).is_multiple_of(40) {
                on_progress(*scanned_total, &dir, None);
            }
            continue;
        };
        let mut files = Vec::new();
        let mut families = BTreeSet::new();
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                if max_depth.is_none_or(|limit| depth < limit) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let skip = if thorough {
                        should_skip_thorough_dir(&name, depth)
                    } else {
                        should_skip_scan_dir(&name)
                    };
                    if !skip {
                        stack.push((path, depth + 1));
                    }
                }
            } else if ft.is_file() && fonts::is_font_file(&path) {
                let pkey = fonts::path_key(&path);
                if let Some(fam) = known_families.get(&pkey) {
                    families.insert(fam.clone());
                } else if let Ok(faces) = fonts::reader::read_faces(&path) {
                    for face in faces {
                        families.insert(face.family);
                    }
                }
                files.push(path.to_string_lossy().into_owned());
            }
        }
        if !files.is_empty() {
            let label = dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| dir.to_string_lossy().into_owned());
            let found = DiscoveredFontFolder {
                path: dir.to_string_lossy().into_owned(),
                label,
                origin_kind: origin_kind.to_string(),
                font_count: files.len(),
                families: families.into_iter().collect(),
                files,
            };
            on_progress(*scanned_total, &dir, Some(&found));
            out.push(found);
        } else if (*scanned_total).is_multiple_of(40) || *scanned_total == 1 {
            on_progress(*scanned_total, &dir, None);
        }
    }
}

pub(crate) fn run_smart_scan(state: &AppState, custom_dir: Option<&str>) -> OsFontReport {
    run_smart_scan_with_progress(state, custom_dir, None)
}

fn run_smart_scan_with_progress(
    state: &AppState,
    custom_dir: Option<&str>,
    app: Option<&AppHandle>,
) -> OsFontReport {
    let (system_fonts_dir, user_fonts_dir) = (
        state.env.system_fonts.to_string_lossy().into_owned(),
        state.env.user_fonts.to_string_lossy().into_owned(),
    );
    let excluded_roots = vec![
        fonts::path_key(&state.env.system_fonts),
        fonts::path_key(&state.env.user_fonts),
        fonts::path_key(&state.env.data_dir),
    ];
    let (system_font_count, user_font_count, known_families) = {
        let cache = state.cache.lock().unwrap();
        let sys = cache
            .families
            .iter()
            .filter(|f| f.origin == Origin::System)
            .count();
        let usr = cache
            .families
            .iter()
            .filter(|f| f.origin == Origin::User || f.origin == Origin::Library)
            .count();
        let mut map = std::collections::HashMap::new();
        for fam in &cache.families {
            for fpath in &fam.files {
                map.insert(fonts::path_key(Path::new(fpath)), fam.name.clone());
            }
        }
        (sys, usr, map)
    };

    let cancel = &state.scan_cancel;
    let mut scanned_total = 0usize;
    let mut emit_progress =
        |scanned_dirs: usize, dir: &Path, found: Option<&DiscoveredFontFolder>| {
            if let Some(handle) = app {
                let _ = handle.emit(
                    "scan-progress",
                    ScanProgressEvent {
                        scanned_dirs,
                        current_dir: dir.to_string_lossy().into_owned(),
                        found_folder: found.cloned(),
                        cancelled: cancel.load(Ordering::Relaxed),
                    },
                );
            }
        };

    let mut discovered_folders = Vec::new();
    if let Some(dir) = custom_dir.filter(|s| !s.trim().is_empty()) {
        let p = normalize_scan_root(dir);
        if p.is_dir() {
            scan_dir_for_fonts(
                &p,
                "Custom Folder",
                None,
                None,
                true,
                &excluded_roots,
                &known_families,
                cancel,
                &mut scanned_total,
                &mut emit_progress,
                &mut discovered_folders,
            );
        }
    } else {
        let mut roots: Vec<(PathBuf, &str, usize)> = Vec::new();
        if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            roots.push((profile.join("Downloads"), "Downloads", 3));
            roots.push((profile.join("Desktop"), "Desktop", 3));
            roots.push((profile.join("Documents"), "Documents", 3));
            roots.push((profile.join("OneDrive"), "OneDrive", 3));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            roots.push((local.join("Adobe"), "Adobe", 3));
        }
        if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
            roots.push((
                roaming
                    .join("Adobe")
                    .join("CoreSync")
                    .join("plugins")
                    .join("livetype"),
                "Adobe Fonts",
                3,
            ));
        }
        let mut seen_paths = HashSet::new();
        for (root, kind, depth) in roots {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if root.is_dir() && seen_paths.insert(fonts::path_key(&root)) {
                scan_dir_for_fonts(
                    &root,
                    kind,
                    Some(depth),
                    Some(MAX_SCAN_DIRS),
                    false,
                    &excluded_roots,
                    &known_families,
                    cancel,
                    &mut scanned_total,
                    &mut emit_progress,
                    &mut discovered_folders,
                );
            }
        }
    }
    discovered_folders.sort_by_key(|f| std::cmp::Reverse(f.font_count));
    let cancelled = cancel.load(Ordering::Relaxed);

    let os_name = if cfg!(target_os = "windows") {
        "Windows".to_string()
    } else {
        std::env::consts::OS.to_string()
    };
    OsFontReport {
        os_name,
        arch: std::env::consts::ARCH.to_string(),
        system_fonts_dir,
        user_fonts_dir,
        system_font_count,
        user_font_count,
        discovered_folders,
        scanned_dirs: scanned_total,
        cancelled,
    }
}

#[tauri::command]
async fn scan_computer_fonts(
    app: AppHandle,
    custom_dir: Option<String>,
) -> Result<OsFontReport, String> {
    bg(move || {
        let state = app.state::<AppState>();
        state.scan_cancel.store(false, Ordering::SeqCst);
        Ok(run_smart_scan_with_progress(
            &state,
            custom_dir.as_deref(),
            Some(&app),
        ))
    })
    .await
}

#[tauri::command]
fn cancel_computer_scan(state: State<AppState>) {
    state.scan_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn watch_folder_path(app: AppHandle, path: String) -> Result<Vec<FolderInfo>, String> {
    bg(move || {
        let p = PathBuf::from(&path);
        if !p.is_dir() {
            return Err(format!("not a folder: {path}"));
        }
        let state = app.state::<AppState>();
        {
            let mut folders = state.folders.lock().unwrap();
            if !folders
                .iter()
                .any(|x| fonts::path_key(x) == fonts::path_key(&p))
            {
                folders.push(p.clone());
            }
        }
        let _ = app.asset_protocol_scope().allow_directory(&p, true);
        save_folders(&state);
        watch_folders(&app);
        let _ = state.refresh();
        Ok(folder_list(&state))
    })
    .await
}

// ---- Google Fonts Discover ----

#[tauri::command]
async fn google_catalog(app: AppHandle, force: bool) -> Result<Value, String> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    let list = google::fetch_catalog(&data_dir, force).await?;
    serde_json::to_value(list).map_err(err)
}

#[tauri::command]
async fn google_install(app: AppHandle, family: String) -> Result<ToggleResponse, String> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    let dest = data_dir
        .join("downloads")
        .join(family.replace(|c: char| !c.is_alphanumeric(), "_"));
    let app_prog = app.clone();
    let fam_prog = family.clone();
    let mut last_pct = -1.0;
    let downloaded = google::download_family(&data_dir, &family, &dest, move |pct| {
        if pct - last_pct >= 2.0 {
            last_pct = pct;
            let _ = app_prog.emit("download", json!({ "family": fam_prog, "pct": pct }));
        }
    })
    .await;

    let files = match downloaded {
        Ok(f) => f,
        Err(e) => {
            let _ = app.emit("download", json!({ "family": family, "error": true }));
            return Err(e);
        }
    };

    let app_bg = app.clone();
    let resp = bg(move || {
        let state = app_bg.state::<AppState>();
        let outcome = install_files(&state, &files, false);
        let _ = fs::remove_dir_all(&dest);
        let (families, _, diagnostics, _) = state.refresh();
        Ok(ToggleResponse {
            families,
            outcome,
            diagnostics,
        })
    })
    .await?;

    let _ = app.emit("download", json!({ "family": family, "pct": 100 }));
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

#[tauri::command]
fn custom_sources_list(app: AppHandle) -> Vec<google::CustomFontSource> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    google::list_custom_sources(&data_dir)
}

#[tauri::command]
async fn custom_source_add(
    app: AppHandle,
    name: String,
    url: String,
    kind: String,
) -> Result<Vec<google::CustomFontSource>, String> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    google::add_custom_source(&data_dir, name, url, kind).await
}

#[tauri::command]
fn custom_source_remove(app: AppHandle, id: String) -> Vec<google::CustomFontSource> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    google::remove_custom_source(&data_dir, &id)
}

#[tauri::command]
fn custom_source_toggle(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Vec<google::CustomFontSource> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    google::toggle_custom_source(&data_dir, &id, enabled)
}

#[tauri::command]
async fn custom_source_sync(
    app: AppHandle,
    id: String,
) -> Result<Vec<google::CustomFontSource>, String> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    google::sync_custom_source(&data_dir, &id).await
}

#[tauri::command]
async fn install_forged_font(
    app: AppHandle,
    file_name: String,
    bytes: Vec<u8>,
    install_for_all: bool,
) -> Result<ToggleResponse, String> {
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    let stem: String = file_name
        .trim_end_matches(".ttf")
        .trim_end_matches(".TTF")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let safe_stem = if stem.trim_matches('-').is_empty() {
        "ForgedFont".to_string()
    } else {
        stem
    };
    let dest_dir = data_dir.join("forged");
    let dest_file = dest_dir.join(format!("{safe_stem}.ttf"));
    let app_bg = app.clone();
    let resp = bg(move || {
        fs::create_dir_all(&dest_dir).map_err(err)?;
        fs::write(&dest_file, &bytes).map_err(err)?;
        let state = app_bg.state::<AppState>();
        let outcome = install_files(&state, std::slice::from_ref(&dest_file), install_for_all);
        let _ = fs::remove_file(&dest_file);
        let (families, _, diagnostics, _) = state.refresh();
        Ok(ToggleResponse {
            families,
            outcome,
            diagnostics,
        })
    })
    .await?;
    let _ = app.emit("inventory-updated", &resp.families);
    Ok(resp)
}

// ---- Cosmic Foundry Vector Studio & Search Engines ----

#[tauri::command]
async fn foundry_compile_modular(
    req: cosmic_foundry::model::ModularForgeRequest,
) -> Result<cosmic_foundry::model::CompiledFontResult, String> {
    bg(move || {
        let project = cosmic_foundry::modular::modular_to_project(&req);
        cosmic_foundry::compiler::compile_ttf(&project, false)
    })
    .await
}

#[tauri::command]
async fn foundry_modular_to_project(
    req: cosmic_foundry::model::ModularForgeRequest,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || Ok(cosmic_foundry::modular::modular_to_project(&req))).await
}

#[tauri::command]
async fn foundry_compile_project(
    project: cosmic_foundry::model::FoundryProject,
    clean_outlines: bool,
) -> Result<cosmic_foundry::model::CompiledFontResult, String> {
    bg(move || cosmic_foundry::compiler::compile_ttf(&project, clean_outlines)).await
}

#[tauri::command]
async fn foundry_compile_woff(ttf_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    bg(move || cosmic_foundry::compiler::compile_woff(&ttf_bytes)).await
}

#[tauri::command]
async fn foundry_fork_font(
    path: String,
    new_family: Option<String>,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(
        move || {
            cosmic_foundry::import::fork_font_file(Path::new(&path), new_family.as_deref(), 256)
        },
    )
    .await
}

#[tauri::command]
async fn foundry_inspect_glyph(
    path: String,
    codepoint: u32,
) -> Result<cosmic_foundry::model::GlyphInspectInfo, String> {
    bg(move || cosmic_foundry::import::inspect_glyph_outline(Path::new(&path), codepoint)).await
}

#[tauri::command]
async fn foundry_import_svg_glyph(
    svg: String,
    upm: u16,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || cosmic_foundry::import::import_svg_glyph(&svg, upm)).await
}

#[tauri::command]
async fn foundry_transform_glyph(
    contours: Vec<cosmic_foundry::model::Contour>,
    op: cosmic_foundry::model::GlyphTransformOp,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || Ok(cosmic_foundry::pathops::apply_transform(&contours, &op))).await
}

#[tauri::command]
async fn foundry_trace_image(
    bytes: Vec<u8>,
    opts: cosmic_foundry::trace::TraceOptions,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || cosmic_foundry::trace::trace_image_bytes(&bytes, &opts)).await
}

#[tauri::command]
async fn foundry_trace_sheet(
    bytes: Vec<u8>,
    cols: usize,
    rows: usize,
    chars: String,
    opts: cosmic_foundry::trace::TraceOptions,
) -> Result<std::collections::BTreeMap<String, Vec<cosmic_foundry::model::Contour>>, String> {
    bg(move || cosmic_foundry::trace::trace_glyph_sheet(&bytes, cols, rows, &chars, &opts)).await
}

#[tauri::command]
async fn foundry_measure_stems(
    contours: Vec<cosmic_foundry::model::Contour>,
    p0: [f64; 2],
    p1: [f64; 2],
) -> Result<Vec<cosmic_foundry::edit::StemMeasurement>, String> {
    bg(move || {
        Ok(cosmic_foundry::edit::measure_stem_thickness(
            &contours, p0, p1,
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_slice_contours(
    contours: Vec<cosmic_foundry::model::Contour>,
    p0: [f64; 2],
    p1: [f64; 2],
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || {
        Ok(cosmic_foundry::edit::slice_contours_with_line(
            &contours, p0, p1,
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_boolean_op(
    a: Vec<cosmic_foundry::model::Contour>,
    b: Vec<cosmic_foundry::model::Contour>,
    op: String,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || Ok(cosmic_foundry::edit::boolean_op_contours(&a, &b, &op))).await
}

#[tauri::command]
async fn foundry_outline_stroke(
    contours: Vec<cosmic_foundry::model::Contour>,
    width: f64,
    round_caps: bool,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || {
        Ok(cosmic_foundry::pathops::outline_stroke(
            &contours, width, round_caps,
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_export_ufo(
    app: AppHandle,
    project: cosmic_foundry::model::FoundryProject,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let slug: String = format!("{}-{}", project.family_name, project.style_name)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let default_name = format!("{}.ufo", slug.trim_matches('-'));
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter("Unified Font Object (.ufo)", &["ufo"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let mut dest = picked.into_path().map_err(err)?;
    if dest.extension().is_none() {
        dest = dest.with_extension("ufo");
    }
    bg(move || {
        cosmic_foundry::compiler::export_ufo(&project, &dest)?;
        Ok(Some(dest.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
async fn foundry_import_ufo(
    app: AppHandle,
) -> Result<Option<cosmic_foundry::model::FoundryProject>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let src = picked.into_path().map_err(err)?;
    bg(move || Ok(Some(cosmic_foundry::compiler::import_ufo(&src)?))).await
}

#[tauri::command]
async fn foundry_subset_font(
    app: AppHandle,
    path: String,
    text: String,
    axes: Vec<(String, f32)>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let src_path = PathBuf::from(&path);
    let stem = src_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("font");
    let ext = src_path
        .extension()
        .and_then(|s| s.to_str())
        .filter(|e| e.eq_ignore_ascii_case("otf") || e.eq_ignore_ascii_case("ttf"))
        .unwrap_or("ttf");
    let default_name = format!("{stem}-subset.{ext}");
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter("Font File", &["ttf", "otf"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = picked.into_path().map_err(err)?;
    bg(move || {
        let bytes = cosmic_foundry::compiler::subset_font(Path::new(&path), &text, &axes)?;
        fs::write(&dest, bytes).map_err(err)?;
        Ok(Some(dest.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
async fn discover_search_github_fonts(
    query: String,
) -> Result<Vec<google::GithubFontRepo>, String> {
    google::search_github_fonts(&query).await
}

#[derive(serde::Serialize)]
struct CompositesResult {
    project: cosmic_foundry::model::FoundryProject,
    created: usize,
}

#[tauri::command]
async fn foundry_auto_space(
    mut project: cosmic_foundry::model::FoundryProject,
    target_sidebearing: f64,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || {
        cosmic_foundry::typography::auto_space_project(&mut project, target_sidebearing);
        Ok(project)
    })
    .await
}

#[tauri::command]
async fn foundry_auto_kern(
    mut project: cosmic_foundry::model::FoundryProject,
    target_gap: f64,
    min_abs_kern: i16,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || {
        cosmic_foundry::typography::auto_kern_project(&mut project, target_gap, min_abs_kern);
        Ok(project)
    })
    .await
}

#[tauri::command]
async fn foundry_generate_composites(
    mut project: cosmic_foundry::model::FoundryProject,
) -> Result<CompositesResult, String> {
    bg(move || {
        let created = cosmic_foundry::typography::generate_latin_composites(&mut project);
        Ok(CompositesResult { project, created })
    })
    .await
}

#[tauri::command]
async fn foundry_apply_linked_metrics(
    mut project: cosmic_foundry::model::FoundryProject,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || {
        cosmic_foundry::typography::apply_linked_metrics(&mut project);
        Ok(project)
    })
    .await
}

#[derive(serde::Serialize)]
struct AutofixResult {
    project: cosmic_foundry::model::FoundryProject,
    fixed: usize,
    report: cosmic_foundry::qa::QaReport,
}

#[tauri::command]
async fn foundry_generate_style(
    project: cosmic_foundry::model::FoundryProject,
    kind: cosmic_foundry::styles::StyleVariantKind,
    amount: f64,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || {
        Ok(cosmic_foundry::styles::generate_style_variant(
            &project, kind, amount,
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_compile_variable_ttf(
    app: AppHandle,
    base: cosmic_foundry::model::FoundryProject,
    masters: Vec<cosmic_foundry::variable::VariableMaster>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let raw_family = if base.family_name.trim().is_empty() {
        "CosmicFont"
    } else {
        base.family_name.trim()
    };
    let clean_stem: String = raw_family
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let default_name = if clean_stem.is_empty() {
        "CosmicFont-Variable.ttf".to_string()
    } else {
        format!("{clean_stem}-Variable.ttf")
    };
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter("Variable TrueType Font", &["ttf"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = picked.into_path().map_err(err)?;
    bg(move || {
        let bytes = cosmic_foundry::variable::compile_variable_ttf(&base, &masters)?;
        fs::write(&dest, bytes).map_err(err)?;
        Ok(Some(dest.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
async fn foundry_audit_project(
    project: cosmic_foundry::model::FoundryProject,
) -> Result<cosmic_foundry::qa::QaReport, String> {
    bg(move || Ok(cosmic_foundry::qa::audit_project(&project))).await
}

#[tauri::command]
async fn foundry_autofix_project(
    mut project: cosmic_foundry::model::FoundryProject,
) -> Result<AutofixResult, String> {
    bg(move || {
        let (fixed, report) = cosmic_foundry::qa::autofix_project(&mut project);
        Ok(AutofixResult {
            project,
            fixed,
            report,
        })
    })
    .await
}

#[tauri::command]
async fn library_visual_match_image(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
    sample_chars: String,
    top_k: Option<usize>,
) -> Result<Vec<cosmic_foundry::retype::VisualMatchHit>, String> {
    let candidates: Vec<(String, String)> = {
        let cache = state.cache.lock().map_err(err)?;
        cache
            .families
            .iter()
            .filter(|f| !f.file.is_empty())
            .map(|f| (f.name.clone(), f.file.clone()))
            .collect()
    };
    bg(move || {
        let opts = cosmic_foundry::trace::TraceOptions::default();
        let contours = cosmic_foundry::trace::trace_image_bytes(&bytes, &opts)?;
        if contours.is_empty() {
            return Err("No trace shapes detected in image".to_string());
        }
        Ok(cosmic_foundry::retype::match_outline_against_fonts(
            &contours,
            &candidates,
            &sample_chars,
            top_k.unwrap_or(8),
        ))
    })
    .await
}

#[tauri::command]
async fn library_visual_match_svg(
    state: State<'_, AppState>,
    svg_text: String,
    sample_chars: String,
    top_k: Option<usize>,
) -> Result<Vec<cosmic_foundry::retype::VisualMatchHit>, String> {
    let candidates: Vec<(String, String)> = {
        let cache = state.cache.lock().map_err(err)?;
        cache
            .families
            .iter()
            .filter(|f| !f.file.is_empty())
            .map(|f| (f.name.clone(), f.file.clone()))
            .collect()
    };
    bg(move || {
        let contours = cosmic_foundry::import::import_svg_glyph(&svg_text, 1000)?;
        if contours.is_empty() {
            return Err("No valid outlines parsed from SVG".to_string());
        }
        Ok(cosmic_foundry::retype::match_outline_against_fonts(
            &contours,
            &candidates,
            &sample_chars,
            top_k.unwrap_or(8),
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_vibe_search(
    state: State<'_, AppState>,
    query: String,
    top_k: Option<usize>,
) -> Result<Vec<cosmic_foundry::vibe::VibeMatchHit>, String> {
    let candidates: Vec<(String, String, String)> = {
        let cache = state.cache.lock().map_err(err)?;
        cache
            .families
            .iter()
            .filter(|f| !f.file.is_empty())
            .map(|f| (f.name.clone(), f.file.clone(), f.category.clone()))
            .collect()
    };
    bg(move || {
        Ok(cosmic_foundry::vibe::search_fonts_by_vibe(
            &query,
            &candidates,
            top_k.unwrap_or(10),
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_find_outline_duplicates(
    state: State<'_, AppState>,
    paths: Option<Vec<String>>,
) -> Result<Vec<cosmic_foundry::outline_hash::DuplicateGroup>, String> {
    let target_paths: Vec<PathBuf> = if let Some(explicit) = paths {
        explicit
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .collect()
    } else {
        let cache = state.cache.lock().map_err(err)?;
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for fam in &cache.families {
            for p in &fam.files {
                if !p.is_empty() && seen.insert(p.clone()) {
                    list.push(PathBuf::from(p));
                }
            }
        }
        list
    };
    bg(move || {
        Ok(cosmic_foundry::outline_hash::find_outline_duplicates(
            &target_paths,
        ))
    })
    .await
}

#[tauri::command]
async fn foundry_triage_archive(
    path: String,
) -> Result<cosmic_foundry::ingest::ArchiveTriageReport, String> {
    bg(move || cosmic_foundry::ingest::triage_path(Path::new(&path))).await
}

#[tauri::command]
async fn foundry_pick_triage_file(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter(
            "Font Archive or File",
            &["zip", "ttf", "otf", "woff", "woff2"],
        )
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

// ---- VecGlypher AI Font Generation (C++/CUDA Ollama & OpenAI/vLLM + Pure-Rust) ----

#[derive(Serialize)]
struct AiBackendStatus {
    ok: bool,
    models: Vec<String>,
    has_vecglypher: bool,
    recommended_pull: String,
    message: String,
}

#[derive(Deserialize)]
struct AiGlyphRequest {
    endpoint: String,
    provider: String,
    model: String,
    style_prompt: String,
    character: String,
    #[serde(default)]
    exemplar_glyphs: Vec<cosmic_foundry::model::GlyphOutline>,
    #[serde(default)]
    temperature: f32,
    #[serde(default)]
    upm: u16,
}

#[derive(Deserialize)]
struct AiFontBatchRequest {
    project: cosmic_foundry::model::FoundryProject,
    endpoint: String,
    provider: String,
    model: String,
    style_prompt: String,
    chars: String,
    #[serde(default)]
    exemplar_keys: Vec<String>,
    #[serde(default)]
    temperature: f32,
    #[serde(default)]
    auto_space_kern: bool,
    #[serde(default)]
    auto_composites: bool,
    #[serde(default)]
    auto_fix_qa: bool,
}

#[derive(Clone, Serialize)]
struct AiGlyphProgressPayload {
    current: usize,
    total: usize,
    character: String,
    status: String,
}

pub(crate) async fn run_vecglypher_inference(
    client: &reqwest::Client,
    endpoint: &str,
    provider: &str,
    model: &str,
    cfg: &cosmic_foundry::vecglypher::VecGlypherPromptConfig,
    images_b64: &[String],
) -> Result<String, String> {
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("AI backend endpoint cannot be empty".into());
    }
    let model_name = if model.trim().is_empty() {
        cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF
    } else {
        model.trim()
    };

    if provider.eq_ignore_ascii_case("ollama") {
        let url = format!("{base}/api/chat");
        let payload =
            cosmic_foundry::vecglypher::build_ollama_chat_payload(model_name, cfg, images_b64);
        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed ({url}): {e}"))?;
        let status = resp.status();
        let val: Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid Ollama JSON response ({status}): {e}"))?;
        if let Some(err_msg) = val["error"].as_str() {
            return Err(format!("Ollama error: {err_msg}"));
        }
        val["message"]["content"]
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("Ollama response missing message.content ({status})"))
    } else {
        let url = if base.ends_with("/v1") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        };
        let payload =
            cosmic_foundry::vecglypher::build_openai_chat_payload(model_name, cfg, images_b64);
        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("OpenAI/vLLM request failed ({url}): {e}"))?;
        let status = resp.status();
        let val: Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid OpenAI/vLLM JSON response ({status}): {e}"))?;
        if let Some(err_msg) = val["error"]["message"]
            .as_str()
            .or_else(|| val["error"].as_str())
        {
            return Err(format!("OpenAI/vLLM error: {err_msg}"));
        }
        val["choices"][0]["message"]["content"]
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                format!("OpenAI/vLLM response missing choices[0].message.content ({status})")
            })
    }
}

#[tauri::command]
async fn foundry_ai_check_backend(
    endpoint: String,
    provider: String,
) -> Result<AiBackendStatus, String> {
    let recommended_pull = format!(
        "ollama run {}",
        cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF
    );
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Ok(AiBackendStatus {
            ok: false,
            models: Vec::new(),
            has_vecglypher: false,
            recommended_pull,
            message: "Endpoint URL is empty.".into(),
        });
    }

    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    else {
        return Ok(AiBackendStatus {
            ok: false,
            models: Vec::new(),
            has_vecglypher: false,
            recommended_pull,
            message: "Failed to initialize HTTP client.".into(),
        });
    };

    let url = if provider.eq_ignore_ascii_case("ollama") {
        format!("{base}/api/tags")
    } else if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            return Ok(AiBackendStatus {
                ok: false,
                models: Vec::new(),
                has_vecglypher: false,
                recommended_pull,
                message: format!("Backend unreachable at {url}: {e}"),
            });
        }
    };

    if !resp.status().is_success() {
        return Ok(AiBackendStatus {
            ok: false,
            models: Vec::new(),
            has_vecglypher: false,
            recommended_pull,
            message: format!("Backend returned HTTP {} at {url}", resp.status()),
        });
    }

    let val: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(AiBackendStatus {
                ok: false,
                models: Vec::new(),
                has_vecglypher: false,
                recommended_pull,
                message: format!("Invalid JSON from {url}: {e}"),
            });
        }
    };

    let mut models = Vec::new();
    if provider.eq_ignore_ascii_case("ollama") {
        if let Some(arr) = val["models"].as_array() {
            for item in arr {
                if let Some(name) = item["name"].as_str().or_else(|| item["model"].as_str()) {
                    models.push(name.to_string());
                }
            }
        }
    } else if let Some(arr) = val["data"].as_array() {
        for item in arr {
            if let Some(id) = item["id"].as_str() {
                models.push(id.to_string());
            }
        }
    }

    let has_vecglypher = models
        .iter()
        .any(|m| m.to_ascii_lowercase().contains("vecglypher"));
    let message = if has_vecglypher {
        format!(
            "Connected ({} models loaded, VecGlypher ready).",
            models.len()
        )
    } else {
        format!(
            "Connected ({} models loaded). Pull VecGlypher-27b-it for dedicated SVG glyph synthesis.",
            models.len()
        )
    };

    Ok(AiBackendStatus {
        ok: true,
        models,
        has_vecglypher,
        recommended_pull,
        message,
    })
}

fn build_glyph_from_contours(
    ch: char,
    contours: Vec<cosmic_foundry::model::Contour>,
    upm: u16,
) -> cosmic_foundry::model::GlyphOutline {
    let bbox = cosmic_foundry::model::compute_bbox(&contours);
    let width_span = (bbox[2] - bbox[0]).max(0.0);
    let pad = (upm as f64) * 0.12;
    let min_adv = (upm as f64) * 0.30;
    let advance_width = (width_span + pad).round().max(min_adv) as i32;
    let lsb = bbox[0].round() as i32;
    cosmic_foundry::model::GlyphOutline {
        codepoint: ch as u32,
        name: format!("uni{:04X}", ch as u32),
        advance_width,
        lsb,
        contours,
    }
}

#[tauri::command]
async fn foundry_ai_generate_glyph(
    req: AiGlyphRequest,
) -> Result<cosmic_foundry::model::GlyphOutline, String> {
    let ch = req
        .character
        .chars()
        .next()
        .ok_or_else(|| "Target character cannot be empty".to_string())?;
    let upm = if req.upm == 0 { 1000 } else { req.upm };
    let temp = if req.temperature <= 0.0 {
        0.7
    } else {
        req.temperature
    };

    let mut images_b64 = Vec::new();
    for g in req.exemplar_glyphs.iter().take(8) {
        if !g.contours.is_empty() {
            if let Ok(b64) = cosmic_foundry::vecglypher::render_glyph_exemplar_base64(g, upm, 192) {
                images_b64.push(b64);
            }
        }
    }

    let cfg = cosmic_foundry::vecglypher::VecGlypherPromptConfig {
        style_prompt: req.style_prompt,
        target_text: ch.to_string(),
        temperature: temp,
        ..Default::default()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(err)?;

    let raw = run_vecglypher_inference(
        &client,
        &req.endpoint,
        &req.provider,
        &req.model,
        &cfg,
        &images_b64,
    )
    .await?;
    let contours = cosmic_foundry::vecglypher::decode_vecglypher_svg(&raw, upm)?;
    Ok(build_glyph_from_contours(ch, contours, upm))
}

#[tauri::command]
async fn foundry_ai_generate_font_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    req: AiFontBatchRequest,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    use std::sync::atomic::Ordering;
    state.ai_cancel.store(false, Ordering::SeqCst);

    let mut project = req.project;
    let upm = if project.metrics.upm == 0 {
        1000
    } else {
        project.metrics.upm
    };
    let temp = if req.temperature <= 0.0 {
        0.7
    } else {
        req.temperature
    };

    let mut images_b64 = Vec::new();
    for key in req.exemplar_keys.iter().take(8) {
        if let Some(g) = project.glyphs.get(key) {
            if !g.contours.is_empty() {
                if let Ok(b64) =
                    cosmic_foundry::vecglypher::render_glyph_exemplar_base64(g, upm, 192)
                {
                    images_b64.push(b64);
                }
            }
        }
    }

    let mut target_chars = Vec::new();
    let mut seen_cp = HashSet::new();
    for ch in req.chars.chars() {
        if !ch.is_control() && seen_cp.insert(ch as u32) {
            target_chars.push(ch);
        }
    }
    if target_chars.is_empty() {
        return Err("Character batch cannot be empty".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(err)?;

    let total = target_chars.len();
    for (idx, ch) in target_chars.into_iter().enumerate() {
        if state.ai_cancel.load(Ordering::SeqCst) {
            let _ = app.emit(
                "ai-glyph-progress",
                AiGlyphProgressPayload {
                    current: idx,
                    total,
                    character: ch.to_string(),
                    status: "cancelled".into(),
                },
            );
            break;
        }

        let _ = app.emit(
            "ai-glyph-progress",
            AiGlyphProgressPayload {
                current: idx + 1,
                total,
                character: ch.to_string(),
                status: "generating".into(),
            },
        );

        let cfg = cosmic_foundry::vecglypher::VecGlypherPromptConfig {
            style_prompt: req.style_prompt.clone(),
            target_text: ch.to_string(),
            temperature: temp,
            ..Default::default()
        };

        if let Ok(raw) = run_vecglypher_inference(
            &client,
            &req.endpoint,
            &req.provider,
            &req.model,
            &cfg,
            &images_b64,
        )
        .await
        {
            if let Ok(contours) = cosmic_foundry::vecglypher::decode_vecglypher_svg(&raw, upm) {
                let glyph = build_glyph_from_contours(ch, contours, upm);
                project.glyphs.insert((ch as u32).to_string(), glyph);
            }
        }
    }

    if req.auto_composites {
        let _ = cosmic_foundry::typography::generate_latin_composites(&mut project);
    }
    if req.auto_space_kern {
        let sb = ((upm as f64) * 0.06).round();
        let gap = ((upm as f64) * 0.08).round();
        cosmic_foundry::typography::auto_space_project(&mut project, sb);
        cosmic_foundry::typography::auto_kern_project(&mut project, gap, 10);
    }
    if req.auto_fix_qa {
        let _ = cosmic_foundry::qa::autofix_project(&mut project);
    }

    let _ = app.emit(
        "ai-glyph-progress",
        AiGlyphProgressPayload {
            current: total,
            total,
            character: String::new(),
            status: "done".into(),
        },
    );

    Ok(project)
}

#[tauri::command]
async fn foundry_ai_cancel_batch(state: State<'_, AppState>) -> Result<bool, String> {
    use std::sync::atomic::Ordering;
    state.ai_cancel.store(true, Ordering::SeqCst);
    Ok(true)
}

#[derive(Clone, Serialize)]
struct AiPullProgressPayload {
    status: String,
    completed: u64,
    total: u64,
    percent: f32,
    done: bool,
    error: Option<String>,
}

#[tauri::command]
async fn foundry_ai_pull_model(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
    model: String,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    state.ai_pull_cancel.store(false, Ordering::SeqCst);

    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Endpoint URL is empty.".into());
    }
    let model_name = model.trim();
    if model_name.is_empty() {
        return Err("Model name cannot be empty.".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(err)?;

    let url = format!("{base}/api/pull");
    let payload = json!({
        "name": model_name,
        "stream": true
    });

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to initiate model pull at {url}: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama pull error ({status}): {body}"));
    }

    let mut stream = resp;
    let mut buffer = String::new();
    let mut last_total = 0u64;

    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|e| format!("Stream read error: {e}"))?
    {
        if state.ai_pull_cancel.load(Ordering::SeqCst) {
            let _ = app.emit(
                "ai-pull-progress",
                AiPullProgressPayload {
                    status: "Download cancelled by user.".into(),
                    completed: 0,
                    total: last_total,
                    percent: 0.0,
                    done: true,
                    error: Some("Cancelled".into()),
                },
            );
            return Ok(());
        }

        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer.drain(..=newline_pos);
            if line.is_empty() {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(&line) {
                if let Some(err_msg) = val["error"].as_str() {
                    let _ = app.emit(
                        "ai-pull-progress",
                        AiPullProgressPayload {
                            status: format!("Error: {err_msg}"),
                            completed: 0,
                            total: last_total,
                            percent: 0.0,
                            done: true,
                            error: Some(err_msg.to_string()),
                        },
                    );
                    return Err(format!("Ollama error: {err_msg}"));
                }

                let status_str = val["status"].as_str().unwrap_or("downloading").to_string();
                let completed = val["completed"].as_u64().unwrap_or(0);
                let total = val["total"].as_u64().unwrap_or(last_total);
                if total > 0 {
                    last_total = total;
                }
                let percent = if total > 0 {
                    ((completed as f64 / total as f64) * 100.0) as f32
                } else if status_str == "success" {
                    100.0
                } else {
                    0.0
                };
                let done = status_str == "success";

                let _ = app.emit(
                    "ai-pull-progress",
                    AiPullProgressPayload {
                        status: if done {
                            "Model downloaded and verified successfully!".into()
                        } else {
                            status_str
                        },
                        completed,
                        total,
                        percent,
                        done,
                        error: None,
                    },
                );

                if done {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn foundry_ai_cancel_pull(state: State<'_, AppState>) -> Result<bool, String> {
    use std::sync::atomic::Ordering;
    state.ai_pull_cancel.store(true, Ordering::SeqCst);
    Ok(true)
}

// ---- BYO-Sync, Portability, Collection ZIP & Migration ----

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Windows-PC".into())
}

#[tauri::command]
async fn sync_choose_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
async fn sync_manifest_now(
    app: AppHandle,
    sync_dir: String,
    library: sync::LibraryState,
    push_local: bool,
) -> Result<sync::SyncResult, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let families = state.cache.lock().unwrap().families.clone();
        sync::sync_manifest_folder(
            Path::new(&sync_dir),
            &library,
            &families,
            &device_name(),
            push_local,
        )
    })
    .await
}

#[tauri::command]
async fn export_profile_bundle(
    app: AppHandle,
    library: sync::LibraryState,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name("cosmic-profile.json")
        .add_filter("Cosmic Manifest", &["json"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = picked.into_path().map_err(err)?;
    bg(move || {
        let state = app.state::<AppState>();
        let families = state.cache.lock().unwrap().families.clone();
        sync::export_profile_json(&dest, &library, &families, &device_name())?;
        Ok(Some(dest.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
async fn import_profile_bundle(
    app: AppHandle,
    library: sync::LibraryState,
) -> Result<Option<sync::SyncResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Cosmic Manifest", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let src = picked.into_path().map_err(err)?;
    bg(move || {
        let state = app.state::<AppState>();
        let families = state.cache.lock().unwrap().families.clone();
        let res = sync::import_profile_json(&src, &library, &families)?;
        Ok(Some(res))
    })
    .await
}

#[tauri::command]
async fn export_collection_zip(
    app: AppHandle,
    collection_name: String,
    fonts: Vec<String>,
) -> Result<Option<sync::PackageResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    let safe_name: String = collection_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(format!("{safe_name}-fonts.zip"))
        .add_filter("ZIP Archive", &["zip"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = picked.into_path().map_err(err)?;
    bg(move || {
        let state = app.state::<AppState>();
        let families = state.cache.lock().unwrap().families.clone();
        let res = sync::package_collection_zip(&dest, &collection_name, &fonts, &families)?;
        Ok(Some(res))
    })
    .await
}

#[tauri::command]
fn find_similar_fonts(
    state: State<AppState>,
    family: String,
    limit: Option<usize>,
) -> Vec<similarity::SimilarMatch> {
    let families = state.cache.lock().unwrap().families.clone();
    similarity::find_similar_families(&family, &families, limit.unwrap_or(8))
}

#[tauri::command]
fn suggest_font_pairings(
    state: State<AppState>,
    family: String,
    limit: Option<usize>,
) -> Vec<similarity::SimilarMatch> {
    let families = state.cache.lock().unwrap().families.clone();
    similarity::suggest_pairings(&family, &families, limit.unwrap_or(6))
}

#[tauri::command]
async fn export_design_kit_zip(
    app: AppHandle,
    heading_family: String,
    body_family: String,
    base_px: f32,
    ratio: f32,
) -> Result<Option<sync::PackageResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name("cosmic-design-kit.zip")
        .add_filter("ZIP Archive", &["zip"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = picked.into_path().map_err(err)?;
    bg(move || {
        let state = app.state::<AppState>();
        let families = state.cache.lock().unwrap().families.clone();
        let res = sync::export_design_kit_zip(
            &dest,
            &heading_family,
            &body_family,
            base_px,
            ratio,
            &families,
        )?;
        Ok(Some(res))
    })
    .await
}

#[tauri::command]
fn detect_tuna_migration(state: State<AppState>) -> migrate::TunaMigrationStatus {
    let Some(tuna_dir) = migrate::default_tuna_dir(&state.env.data_dir) else {
        return migrate::TunaMigrationStatus::default();
    };
    migrate::detect_tuna_fonts(&tuna_dir)
}

#[tauri::command]
async fn run_tuna_migration(app: AppHandle) -> Result<Value, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let tuna_dir = migrate::default_tuna_dir(&state.env.data_dir)
            .ok_or_else(|| "tuna-fonts data folder not found".to_string())?;
        let (report, library, folders) =
            migrate::import_from_tuna_fonts(&tuna_dir, &state.env.data_dir)?;
        {
            let mut lock = state.folders.lock().unwrap();
            for f in &folders {
                if !lock
                    .iter()
                    .any(|x| fonts::path_key(x) == fonts::path_key(f))
                {
                    lock.push(f.clone());
                    let _ = app.asset_protocol_scope().allow_directory(f, true);
                }
            }
        }
        save_folders(&state);
        watch_folders(&app);
        let (families, _, diagnostics, projects) = state.refresh();
        Ok(json!({
            "report": report,
            "library": library,
            "families": families,
            "diagnostics": diagnostics,
            "projects": projects
        }))
    })
    .await
}

// ---- Global Shortcut, System Links & Explorer ----

#[tauri::command]
fn set_global_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    win::start_hotkey_listener(app, &shortcut)
}

#[tauri::command]
fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| !d.join("uninstall.exe").is_file()))
        .unwrap_or(true)
}

#[tauri::command]
fn open_web(target: String) -> Result<(), String> {
    let url = match target.as_str() {
        "release" => "https://github.com/theCosmicCrafter/cosmic-font-foundry/releases/latest",
        "issue" => "https://github.com/theCosmicCrafter/cosmic-font-foundry/issues/new/choose",
        "repo" => "https://github.com/theCosmicCrafter/cosmic-font-foundry",
        "upstream" => "https://github.com/atunfrito/tuna-fonts",
        _ => return Err(format!("unknown web target: {target}")),
    };
    win::open_url(url)
}

#[tauri::command]
fn show_in_explorer(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let p = PathBuf::from(&path);
    let is_font = fonts::is_font_file(&p);
    if !is_font || !p.is_file() || path.contains('"') {
        return Err(format!("not a valid font file: {path}"));
    }
    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn()
        .map(|_| ())
        .map_err(err)
}

// ---- Initialization & Boot ----

fn read_settings(app: &AppHandle) -> Value {
    app.path()
        .app_data_dir()
        .ok()
        .and_then(|d| fs::read_to_string(d.join(SETTINGS_FILE)).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

pub(crate) fn init_script(boot: &Value) -> String {
    format!(
        "window.__COSMIC__ = {boot};
        (function (t) {{
          if (t !== 'light' && t !== 'dark')
            t = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
          var d = document, s = function () {{ d.documentElement.dataset.theme = t; }};
          if (d.documentElement) s();
          else new MutationObserver(function (m, o) {{
            if (d.documentElement) {{ s(); o.disconnect(); }}
          }}).observe(d, {{ childList: true }});
        }})(window.__COSMIC__.settings.theme);"
    )
}

fn windows_prefers_dark() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 0)
        .unwrap_or(false)
}

fn windows_language() -> &'static str {
    let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if langid & 0x3ff == 0x0a {
        "es"
    } else {
        "en"
    }
}

pub fn elevated_mode() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.to_str()? != "--elevated" {
        return None;
    }
    let req_path = PathBuf::from(args.next()?);
    Some(fonts::system::handle_request(&req_path))
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app)
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                if w.label() == "main"
                    && read_settings(w.app_handle())["onClose"].as_str() != Some("exit")
                {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            saved_fonts,
            read_library_store,
            refresh_fonts,
            toggle_fonts,
            install_fonts,
            install_family_to_windows,
            install_families_to_windows,
            copy_families_to_vault,
            vault_stats,
            choose_vault_folder,
            import_folder_to_vault,
            scan_vault_cleanup,
            recycle_vault_duplicates,
            restore_system_fonts,
            backed_up_count,
            font_characters,
            quarantine_font,
            watched_folders,
            add_watched_folder,
            remove_watched_folder,
            watch_folder_path,
            scan_computer_fonts,
            cancel_computer_scan,
            project_manifests,
            google_catalog,
            google_install,
            custom_sources_list,
            custom_source_add,
            custom_source_remove,
            custom_source_toggle,
            custom_source_sync,
            install_forged_font,
            foundry_compile_modular,
            foundry_modular_to_project,
            foundry_compile_project,
            foundry_compile_woff,
            foundry_fork_font,
            foundry_inspect_glyph,
            foundry_import_svg_glyph,
            foundry_transform_glyph,
            foundry_trace_image,
            foundry_trace_sheet,
            foundry_measure_stems,
            foundry_slice_contours,
            foundry_boolean_op,
            foundry_outline_stroke,
            foundry_auto_space,
            foundry_auto_kern,
            foundry_generate_composites,
            foundry_apply_linked_metrics,
            foundry_generate_style,
            foundry_compile_variable_ttf,
            foundry_audit_project,
            foundry_autofix_project,
            library_visual_match_image,
            library_visual_match_svg,
            foundry_vibe_search,
            foundry_find_outline_duplicates,
            foundry_triage_archive,
            foundry_pick_triage_file,
            foundry_ai_check_backend,
            foundry_ai_generate_glyph,
            foundry_ai_generate_font_batch,
            foundry_ai_cancel_batch,
            foundry_ai_pull_model,
            foundry_ai_cancel_pull,
            foundry_export_ufo,
            foundry_import_ufo,
            foundry_subset_font,
            discover_search_github_fonts,
            sync_choose_folder,
            sync_manifest_now,
            export_profile_bundle,
            import_profile_bundle,
            export_collection_zip,
            find_similar_fonts,
            suggest_font_pairings,
            export_design_kit_zip,
            detect_tuna_migration,
            run_tuna_migration,
            set_global_shortcut,
            is_portable,
            open_web,
            show_in_explorer,
            tray::update_tray,
            tray::get_tray_status,
            tray::tray_action,
            startup::windows_startup,
            startup::set_windows_startup,
            startup::mark_show_on_open,
            agent::agent_get_state,
            agent::agent_set_config,
            agent::agent_run_command,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let hidden = startup::should_start_hidden(&data_dir);
            let env = Environment::real(data_dir.clone());
            let cache = Cache::load(&data_dir.join("fonts-cache.json"));
            let folders: Vec<PathBuf> = fs::read(data_dir.join("folders.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
                .unwrap_or_default()
                .into_iter()
                .map(PathBuf::from)
                .collect();

            let scope = app.asset_protocol_scope();
            for dir in [
                env.deactivated_dir(),
                env.system_backup_dir(),
                env.quarantine_dir(),
                env.user_fonts.clone(),
                env.system_fonts.clone(),
            ] {
                let _ = scope.allow_directory(dir, true);
            }
            for folder in &folders {
                let _ = scope.allow_directory(folder, true);
            }

            app.manage(AppState {
                env,
                cache: Mutex::new(cache),
                folders: Mutex::new(folders),
                loaded_folder_fonts: Mutex::default(),
                disabled_folder_fonts: Mutex::default(),
                watcher: Mutex::default(),
                scan_cancel: Arc::new(AtomicBool::new(false)),
                ai_cancel: Arc::new(AtomicBool::new(false)),
                ai_pull_cancel: Arc::new(AtomicBool::new(false)),
            });
            watch_folders(&handle);
            agent::init(&handle);

            let settings = read_settings(&handle);
            let dark = match settings["theme"].as_str() {
                Some("dark") => true,
                Some("light") => false,
                _ => windows_prefers_dark(),
            };
            let theme = if dark { "dark" } else { "light" };
            let language = settings["language"]
                .as_str()
                .unwrap_or_else(|| windows_language());

            tray::create(&handle, theme, language)?;

            let shortcut = settings["quickSwitcherShortcut"]
                .as_str()
                .unwrap_or("Ctrl+Shift+F");
            let _ = win::start_hotkey_listener(handle.clone(), shortcut);

            let boot = json!({
                "settings": settings,
                "version": app.package_info().version.to_string(),
                "systemLanguage": windows_language(),
            });

            let main_win = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Cosmic Font Foundry")
                .visible(!hidden)
                .inner_size(1280.0, 820.0)
                .min_inner_size(960.0, 600.0)
                .center()
                .decorations(false)
                .shadow(true)
                .zoom_hotkeys_enabled(false)
                .background_color(if dark { BG_DARK } else { BG_LIGHT }.into())
                .initialization_script(init_script(&boot))
                .build()?;
            if !hidden {
                let _ = main_win.set_focus();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to launch Cosmic Font Foundry");

    app.run(|app, ev| {
        if let RunEvent::Exit = ev {
            win::stop_hotkey_listener();
            app.state::<AppState>().unload_all_folder_fonts();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_discovers_fonts_and_skips_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("GochiHand-Regular.ttf");

        let valid_dir = tmp.path().join("ClientKit");
        fs::create_dir_all(&valid_dir).unwrap();
        fs::copy(&fixture, valid_dir.join("GochiHand-Regular.ttf")).unwrap();

        let ignored_dir = tmp.path().join("node_modules").join("some-pkg");
        fs::create_dir_all(&ignored_dir).unwrap();
        fs::copy(&fixture, ignored_dir.join("GochiHand-Regular.ttf")).unwrap();

        let excluded_dir = tmp.path().join("ExcludedRoot");
        fs::create_dir_all(&excluded_dir).unwrap();
        fs::copy(&fixture, excluded_dir.join("GochiHand-Regular.ttf")).unwrap();

        let excluded_roots = vec![fonts::path_key(&excluded_dir)];
        let known_families = std::collections::HashMap::new();
        let cancel = AtomicBool::new(false);
        let mut scanned_total = 0usize;
        let mut noop = |_: usize, _: &Path, _: Option<&DiscoveredFontFolder>| {};
        let mut out = Vec::new();

        scan_dir_for_fonts(
            tmp.path(),
            "Downloads",
            Some(4),
            Some(MAX_SCAN_DIRS),
            false,
            &excluded_roots,
            &known_families,
            &cancel,
            &mut scanned_total,
            &mut noop,
            &mut out,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "ClientKit");
        assert_eq!(out[0].origin_kind, "Downloads");
        assert_eq!(out[0].font_count, 1);
        assert_eq!(out[0].families, vec!["Gochi Hand".to_string()]);
    }

    #[test]
    fn thorough_custom_scan_finds_deep_and_nonstandard_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("GochiHand-Regular.ttf");

        let deep_dir = tmp
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("f")
            .join("DeepFonts");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::copy(&fixture, deep_dir.join("GochiHand-Regular.ttf")).unwrap();

        let pkg_dir = tmp.path().join("packages").join("brand-kit");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::copy(&fixture, pkg_dir.join("GochiHand-Regular.ttf")).unwrap();

        let dot_dir = tmp.path().join(".fonts");
        fs::create_dir_all(&dot_dir).unwrap();
        fs::copy(&fixture, dot_dir.join("GochiHand-Regular.ttf")).unwrap();

        let git_dir = tmp.path().join(".git").join("objects");
        fs::create_dir_all(&git_dir).unwrap();
        fs::copy(&fixture, git_dir.join("GochiHand-Regular.ttf")).unwrap();

        let known_families = std::collections::HashMap::new();
        let cancel = AtomicBool::new(false);
        let mut scanned_total = 0usize;
        let mut noop = |_: usize, _: &Path, _: Option<&DiscoveredFontFolder>| {};
        let mut out = Vec::new();
        scan_dir_for_fonts(
            tmp.path(),
            "Custom Folder",
            None,
            None,
            true,
            &[],
            &known_families,
            &cancel,
            &mut scanned_total,
            &mut noop,
            &mut out,
        );

        let labels: Vec<&str> = out.iter().map(|f| f.label.as_str()).collect();
        assert!(labels.contains(&"DeepFonts"));
        assert!(labels.contains(&"brand-kit"));
        assert!(labels.contains(&".fonts"));
        assert!(!labels.contains(&"objects"));
        assert_eq!(normalize_scan_root("D:"), PathBuf::from("D:\\"));
    }

    #[test]
    fn scanner_stops_immediately_when_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("GochiHand-Regular.ttf");
        for i in 0..5 {
            let d = tmp.path().join(format!("pack-{i}"));
            fs::create_dir_all(&d).unwrap();
            fs::copy(&fixture, d.join("GochiHand-Regular.ttf")).unwrap();
        }

        let known_families = std::collections::HashMap::new();
        let cancel = AtomicBool::new(true);
        let mut scanned_total = 0usize;
        let mut noop = |_: usize, _: &Path, _: Option<&DiscoveredFontFolder>| {};
        let mut out = Vec::new();
        scan_dir_for_fonts(
            tmp.path(),
            "Custom Folder",
            None,
            None,
            true,
            &[],
            &known_families,
            &cancel,
            &mut scanned_total,
            &mut noop,
            &mut out,
        );
        assert_eq!(scanned_total, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn expands_zip_archives_without_marking_temp_files_for_recycle() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("GochiHand-Regular.ttf");

        // Build a minimal ZIP archive containing GochiHand-Regular.ttf
        let zip_path = tmp.path().join("pack.zip");
        let font_bytes = fs::read(&fixture).unwrap();
        let mut zb = cosmic_core::sync::ZipBuilder::new();
        zb.add_file("nested/GochiHand-Regular.ttf", &font_bytes)
            .unwrap();
        fs::write(&zip_path, zb.finish().unwrap()).unwrap();

        let (candidates, _guard, errors) = vault::expand_font_and_zip_sources(&[zip_path]);
        assert!(errors.is_empty());
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].1); // from_zip == true
        assert_eq!(
            candidates[0].0.file_name().unwrap().to_string_lossy(),
            "GochiHand-Regular.ttf"
        );
        assert!(candidates[0].0.is_file());
    }
}
