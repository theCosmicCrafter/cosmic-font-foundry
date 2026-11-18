pub mod fonts;
mod google;
mod startup;
mod tray;
pub mod win;

use cosmic_core::{migrate, sync};
use fonts::{
    cache::Cache, changes, summarize_diagnostics, Environment, FontFamily, InventoryDiagnostics,
    Origin,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

const SETTINGS_FILE: &str = "settings.json";
const LIBRARY_FILE: &str = "library.json";

/// Cosmic Font Foundry window backgrounds (matched to `--win` in `src/app.css` to eliminate startup flash).
const BG_LIGHT: (u8, u8, u8, u8) = (0xf5, 0xf6, 0xfc, 255);
const BG_DARK: (u8, u8, u8, u8) = (0x09, 0x0b, 0x12, 255);

fn err(e: impl ToString) -> String {
    e.to_string()
}

pub struct AppState {
    env: Environment,
    cache: Mutex<Cache>,
    folders: Mutex<Vec<PathBuf>>,
    loaded_folder_fonts: Mutex<HashSet<String>>,
    disabled_folder_fonts: Mutex<HashSet<String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
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
struct ToggleResponse {
    families: Vec<FontFamily>,
    outcome: changes::ToggleOutcome,
    diagnostics: InventoryDiagnostics,
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

#[tauri::command]
async fn toggle_fonts(
    app: AppHandle,
    families: Vec<String>,
    activate: bool,
    system_unlocked: bool,
) -> Result<ToggleResponse, String> {
    let resp = bg({
        let app = app.clone();
        move || {
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
            Ok(ToggleResponse {
                families: updated_families,
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
            let outcome = install_files(&state, &path_bufs, install_for_all);
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

#[derive(Serialize)]
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
struct DiscoveredFontFolder {
    path: String,
    label: String,
    origin_kind: String,
    font_count: usize,
    families: Vec<String>,
    files: Vec<String>,
}

#[derive(Serialize)]
struct OsFontReport {
    os_name: String,
    arch: String,
    system_fonts_dir: String,
    user_fonts_dir: String,
    system_font_count: usize,
    user_font_count: usize,
    discovered_folders: Vec<DiscoveredFontFolder>,
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

fn scan_dir_for_fonts(
    root: &Path,
    origin_kind: &str,
    max_depth: usize,
    excluded_roots: &[String],
    known_families: &std::collections::HashMap<String, String>,
    out: &mut Vec<DiscoveredFontFolder>,
) {
    use std::collections::BTreeSet;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited_dirs = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if visited_dirs >= MAX_SCAN_DIRS {
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
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files = Vec::new();
        let mut families = BTreeSet::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                if depth < max_depth {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !should_skip_scan_dir(&name) {
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
            out.push(DiscoveredFontFolder {
                path: dir.to_string_lossy().into_owned(),
                label,
                origin_kind: origin_kind.to_string(),
                font_count: files.len(),
                families: families.into_iter().collect(),
                files,
            });
        }
    }
}

#[tauri::command]
async fn scan_computer_fonts(
    app: AppHandle,
    custom_dir: Option<String>,
) -> Result<OsFontReport, String> {
    bg(move || {
        let state = app.state::<AppState>();
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

        let mut discovered_folders = Vec::new();
        if let Some(dir) = custom_dir.filter(|s| !s.trim().is_empty()) {
            let p = PathBuf::from(dir);
            if p.is_dir() {
                scan_dir_for_fonts(
                    &p,
                    "Custom Folder",
                    4,
                    &excluded_roots,
                    &known_families,
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
                if root.is_dir() && seen_paths.insert(fonts::path_key(&root)) {
                    scan_dir_for_fonts(
                        &root,
                        kind,
                        depth,
                        &excluded_roots,
                        &known_families,
                        &mut discovered_folders,
                    );
                }
            }
        }
        discovered_folders.sort_by_key(|f| std::cmp::Reverse(f.font_count));

        let os_name = if cfg!(target_os = "windows") {
            "Windows".to_string()
        } else {
            std::env::consts::OS.to_string()
        };
        Ok(OsFontReport {
            os_name,
            arch: std::env::consts::ARCH.to_string(),
            system_fonts_dir,
            user_fonts_dir,
            system_font_count,
            user_font_count,
            discovered_folders,
        })
    })
    .await
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
            restore_system_fonts,
            backed_up_count,
            font_characters,
            quarantine_font,
            watched_folders,
            add_watched_folder,
            remove_watched_folder,
            watch_folder_path,
            scan_computer_fonts,
            project_manifests,
            google_catalog,
            google_install,
            install_forged_font,
            sync_choose_folder,
            sync_manifest_now,
            export_profile_bundle,
            import_profile_bundle,
            export_collection_zip,
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
            });
            watch_folders(&handle);

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
        let mut out = Vec::new();

        scan_dir_for_fonts(
            tmp.path(),
            "Downloads",
            4,
            &excluded_roots,
            &known_families,
            &mut out,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "ClientKit");
        assert_eq!(out[0].origin_kind, "Downloads");
        assert_eq!(out[0].font_count, 1);
        assert_eq!(out[0].families, vec!["Gochi Hand".to_string()]);
    }
}
