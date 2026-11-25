//! BYO-Sync, portability bundles, collection ZIP export, pairings, and Tuna migration commands.

use crate::{bg, err, fonts, save_folders, watch_folders, AppState};
use cosmic_core::{migrate, reader, similarity, sync};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State};

// ---- BYO-Sync, Portability, Collection ZIP & Migration ----

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Windows-PC".into())
}

#[tauri::command]
pub(crate) async fn sync_choose_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) async fn sync_manifest_now(
    app: AppHandle,
    sync_dir: String,
    library: sync::LibraryState,
    push_local: bool,
) -> Result<sync::SyncResult, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let families = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();
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
pub(crate) async fn export_profile_bundle(
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
        let families = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();
        sync::export_profile_json(&dest, &library, &families, &device_name())?;
        Ok(Some(dest.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_profile_bundle(
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
        let families = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();
        let res = sync::import_profile_json(&src, &library, &families)?;
        Ok(Some(res))
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_collection_zip(
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
        let families = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();
        let res = sync::package_collection_zip(&dest, &collection_name, &fonts, &families)?;
        Ok(Some(res))
    })
    .await
}

#[tauri::command]
pub(crate) fn find_similar_fonts(
    state: State<AppState>,
    family: String,
    limit: Option<usize>,
) -> Vec<similarity::SimilarMatch> {
    let families = state
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .families
        .clone();
    similarity::find_similar_families(&family, &families, limit.unwrap_or(8))
}

#[tauri::command]
pub(crate) fn suggest_font_pairings(
    state: State<AppState>,
    family: String,
    limit: Option<usize>,
) -> Vec<similarity::SimilarMatch> {
    let families = state
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .families
        .clone();
    similarity::suggest_pairings(&family, &families, limit.unwrap_or(6))
}

#[tauri::command]
pub(crate) async fn export_design_kit_zip(
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
        let families = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();
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
pub(crate) fn detect_tuna_migration(state: State<AppState>) -> migrate::TunaMigrationStatus {
    let Some(tuna_dir) = migrate::default_tuna_dir(&state.env.data_dir) else {
        return migrate::TunaMigrationStatus::default();
    };
    migrate::detect_tuna_fonts(&tuna_dir)
}

#[tauri::command]
pub(crate) async fn run_tuna_migration(app: AppHandle) -> Result<Value, String> {
    bg(move || {
        let state = app.state::<AppState>();
        let tuna_dir = migrate::default_tuna_dir(&state.env.data_dir)
            .ok_or_else(|| "tuna-fonts data folder not found".to_string())?;
        let (report, library, folders) =
            migrate::import_from_tuna_fonts(&tuna_dir, &state.env.data_dir)?;
        {
            let mut lock = state.folders.lock().unwrap_or_else(|e| e.into_inner());
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

// ---- ArtCraft Suite Font Store Bridge (%LOCALAPPDATA%\Craft Fonts) ----

#[derive(Serialize)]
pub(crate) struct ArtCraftStoreStatus {
    path: String,
    exists: bool,
    artcraft_installed: bool,
    font_files: usize,
    watched: bool,
}

fn artcraft_font_store_dir() -> Option<PathBuf> {
    if let Some(override_dir) = std::env::var_os("CRAFT_FONT_STORE") {
        if !override_dir.is_empty() {
            return Some(PathBuf::from(override_dir));
        }
    }
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("XDG_DATA_HOME"))?;
    Some(PathBuf::from(base).join("Craft Fonts"))
}

fn detect_artcraft_installed(store_dir: Option<&Path>) -> bool {
    if store_dir.is_some_and(Path::exists) {
        return true;
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(local);
        if base.join("Programs").join("ArtCraft").exists() || base.join("ArtCraft").exists() {
            return true;
        }
    }
    false
}

fn count_store_font_files(root: &Path) -> usize {
    if !root.is_dir() {
        return 0;
    }
    let mut total = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                if name_str.starts_with('.') || name_str == "previews" || name_str == "indexes" {
                    continue;
                }
                stack.push(path);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc" | "otc"
                ) {
                    total += 1;
                }
            }
        }
    }
    total
}

fn slugify_family_dir(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "custom-font".into()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub(crate) fn artcraft_store_status(state: State<AppState>) -> ArtCraftStoreStatus {
    let store_dir = artcraft_font_store_dir();
    let exists = store_dir.as_deref().is_some_and(Path::is_dir);
    let artcraft_installed = detect_artcraft_installed(store_dir.as_deref());
    let font_files = store_dir
        .as_deref()
        .map(count_store_font_files)
        .unwrap_or(0);
    let watched = store_dir.as_deref().is_some_and(|dir| {
        let target_key = fonts::path_key(dir);
        state
            .folders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|f| fonts::path_key(f) == target_key)
    });
    ArtCraftStoreStatus {
        path: store_dir
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        exists,
        artcraft_installed,
        font_files,
        watched,
    }
}

#[tauri::command]
pub(crate) async fn send_families_to_artcraft_store(
    app: AppHandle,
    families: Vec<String>,
) -> Result<Value, String> {
    let store_dir = artcraft_font_store_dir()
        .ok_or_else(|| "Could not resolve ArtCraft Craft Fonts directory".to_string())?;
    bg(move || {
        fs::create_dir_all(&store_dir).map_err(err)?;
        let state = app.state::<AppState>();
        let wanted: HashSet<String> = families
            .iter()
            .map(|f| f.trim().to_ascii_lowercase())
            .filter(|f| !f.is_empty())
            .collect();
        if wanted.is_empty() {
            return Err("No font families specified".into());
        }
        let cached = state
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .families
            .clone();

        let mut copied = 0usize;
        let mut skipped = 0usize;
        let mut exported_families = Vec::new();
        for fam in cached
            .into_iter()
            .filter(|f| wanted.contains(&f.name.trim().to_ascii_lowercase()))
        {
            let fam_dir = store_dir.join(slugify_family_dir(&fam.name));
            fs::create_dir_all(&fam_dir).map_err(err)?;
            let mut fam_hit = false;
            for style in &fam.styles {
                let src = Path::new(&style.file);
                if !src.is_file() {
                    continue;
                }
                let Some(fname) = src.file_name() else {
                    continue;
                };
                let dest = fam_dir.join(fname);
                if dest.is_file() {
                    if let (Ok(s_hash), Ok(d_hash)) =
                        (reader::sha256_file(src), reader::sha256_file(&dest))
                    {
                        if s_hash == d_hash {
                            skipped += 1;
                            fam_hit = true;
                            continue;
                        }
                    }
                }
                fs::copy(src, &dest).map_err(err)?;
                copied += 1;
                fam_hit = true;
            }
            if fam_hit {
                exported_families.push(fam.name);
            }
        }
        if exported_families.is_empty() {
            return Err("Selected family files were not found on disk".into());
        }
        Ok(json!({
            "store_path": store_dir.to_string_lossy().into_owned(),
            "copied": copied,
            "skipped": skipped,
            "families": exported_families,
        }))
    })
    .await
}

#[tauri::command]
pub(crate) async fn watch_artcraft_store(app: AppHandle) -> Result<Value, String> {
    let store_dir = artcraft_font_store_dir()
        .ok_or_else(|| "Could not resolve ArtCraft Craft Fonts directory".to_string())?;
    bg(move || {
        fs::create_dir_all(&store_dir).map_err(err)?;
        let state = app.state::<AppState>();
        let store_key = fonts::path_key(&store_dir);
        let folders_snapshot = {
            let mut lock = state.folders.lock().unwrap_or_else(|e| e.into_inner());
            if !lock.iter().any(|f| fonts::path_key(f) == store_key) {
                lock.push(store_dir.clone());
                let _ = app.asset_protocol_scope().allow_directory(&store_dir, true);
            }
            lock.iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        save_folders(&state);
        watch_folders(&app);
        let (families, _, diagnostics, projects) = state.refresh();
        Ok(json!({
            "store_path": store_dir.to_string_lossy().into_owned(),
            "folders": folders_snapshot,
            "families": families,
            "diagnostics": diagnostics,
            "projects": projects,
        }))
    })
    .await
}
