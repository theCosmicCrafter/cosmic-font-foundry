//! BYO-Sync, portability bundles, collection ZIP export, pairings, and Tuna migration commands.

use crate::{bg, err, fonts, save_folders, watch_folders, AppState};
use cosmic_core::{migrate, similarity, sync};
use serde_json::{json, Value};
use std::path::Path;
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
