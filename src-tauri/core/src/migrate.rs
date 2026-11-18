//! Non-destructive migrator from upstream `tuna-fonts` (`%APPDATA%\es.atunfrito.tunafonts`)
//! into Cosmic Font Foundry (`%APPDATA%\com.thecosmiccrafter.cosmicfontfoundry`).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::sync::{LocalCollection, LibraryState};

pub const TUNA_APP_IDENTIFIER: &str = "es.atunfrito.tunafonts";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TunaMigrationStatus {
    pub available: bool,
    pub source_dir: String,
    pub favorites_count: usize,
    pub collections_count: usize,
    pub folders_count: usize,
    pub deactivated_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TunaMigrationReport {
    pub imported_favorites: usize,
    pub imported_collections: usize,
    pub imported_folders: usize,
    pub copied_deactivated_files: usize,
    pub imported_settings: bool,
}

#[derive(Deserialize, Default)]
struct TunaLibrary {
    #[serde(default)]
    favoritas: Vec<String>,
    #[serde(default)]
    colecciones: Vec<TunaCollection>,
}

#[derive(Deserialize, Default)]
struct TunaCollection {
    #[serde(default)]
    id: String,
    #[serde(default)]
    nombre: String,
    #[serde(default)]
    fuentes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MigratedSavedEntry {
    pub file: PathBuf,
    pub original: PathBuf,
    pub value: String,
    pub data: String,
    pub origin: String,
}

#[derive(Deserialize)]
struct TunaGuardada {
    archivo: PathBuf,
    original: PathBuf,
    valor: String,
    datos: String,
    origen: serde_json::Value,
}

fn translate_origin(v: &Value) -> String {
    match v.as_str().unwrap_or("usu") {
        "sis" => "sys".into(),
        "bib" => "lib".into(),
        "car" => "dir".into(),
        _ => "usr".into(),
    }
}

pub fn default_tuna_dir(cosmic_data_dir: &Path) -> Option<PathBuf> {
    let parent = cosmic_data_dir.parent()?;
    let candidate = parent.join(TUNA_APP_IDENTIFIER);
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

pub fn detect_tuna_fonts(source_dir: &Path) -> TunaMigrationStatus {
    if !source_dir.is_dir() {
        return TunaMigrationStatus::default();
    }
    let lib: TunaLibrary = fs::read(source_dir.join("biblioteca.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let folders: Vec<String> = fs::read(source_dir.join("carpetas.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let deactivated: Vec<Value> = fs::read(source_dir.join("desactivadas.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    TunaMigrationStatus {
        available: true,
        source_dir: source_dir.to_string_lossy().into_owned(),
        favorites_count: lib.favoritas.len(),
        collections_count: lib.colecciones.len(),
        folders_count: folders.len(),
        deactivated_count: deactivated.len(),
    }
}

/// Non-destructively copies favorites, collections, watched folders, deactivated font backups,
/// and basic settings from `source_dir` (`es.atunfrito.tunafonts`) into `dest_dir`.
pub fn import_from_tuna_fonts(
    source_dir: &Path,
    dest_dir: &Path,
) -> Result<(TunaMigrationReport, LibraryState, Vec<PathBuf>), String> {
    if !source_dir.is_dir() {
        return Err(format!("tuna-fonts data folder not found: {}", source_dir.display()));
    }
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

    let mut report = TunaMigrationReport::default();

    // 1. Library (biblioteca.json -> merged with existing library.json)
    let mut current_lib: LibraryState = fs::read(dest_dir.join("library.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    if let Ok(bytes) = fs::read(source_dir.join("biblioteca.json")) {
        if let Ok(old_lib) = serde_json::from_slice::<TunaLibrary>(&bytes) {
            for fav in old_lib.favoritas {
                if !current_lib
                    .favorites
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(&fav))
                {
                    current_lib.favorites.push(fav);
                    report.imported_favorites += 1;
                }
            }
            for col in old_lib.colecciones {
                if let Some(existing) = current_lib
                    .collections
                    .iter_mut()
                    .find(|c| c.id == col.id || c.name.eq_ignore_ascii_case(&col.nombre))
                {
                    for font in col.fuentes {
                        if !existing.fonts.iter().any(|f| f.eq_ignore_ascii_case(&font)) {
                            existing.fonts.push(font);
                        }
                    }
                } else {
                    current_lib.collections.push(LocalCollection {
                        id: col.id,
                        name: col.nombre,
                        fonts: col.fuentes,
                        pinned: false,
                    });
                    report.imported_collections += 1;
                }
            }
            let lib_bytes = serde_json::to_vec_pretty(&current_lib).map_err(|e| e.to_string())?;
            fs::write(dest_dir.join("library.json"), lib_bytes).map_err(|e| e.to_string())?;
        }
    }

    // 2. Watched folders (carpetas.json -> folders.json)
    let mut folders: Vec<String> = fs::read(dest_dir.join("folders.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Ok(bytes) = fs::read(source_dir.join("carpetas.json")) {
        if let Ok(old_folders) = serde_json::from_slice::<Vec<String>>(&bytes) {
            for f in old_folders {
                if !folders.iter().any(|x| x.eq_ignore_ascii_case(&f)) {
                    folders.push(f);
                    report.imported_folders += 1;
                }
            }
            let f_bytes = serde_json::to_vec_pretty(&folders).map_err(|e| e.to_string())?;
            fs::write(dest_dir.join("folders.json"), f_bytes).map_err(|e| e.to_string())?;
        }
    }

    // 3. Deactivated user fonts (desactivadas.json + desactivadas/ -> deactivated.json + deactivated/)
    if let Ok(bytes) = fs::read(source_dir.join("desactivadas.json")) {
        if let Ok(old_saved) = serde_json::from_slice::<Vec<TunaGuardada>>(&bytes) {
            let dest_deact_dir = dest_dir.join("deactivated");
            fs::create_dir_all(&dest_deact_dir).map_err(|e| e.to_string())?;
            let mut dest_list: Vec<MigratedSavedEntry> = fs::read(dest_dir.join("deactivated.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();

            for g in old_saved {
                let Some(fname) = g.archivo.file_name() else {
                    continue;
                };
                let target_file = dest_deact_dir.join(fname);
                if g.archivo.is_file() && !target_file.exists() {
                    if fs::copy(&g.archivo, &target_file).is_ok() {
                        report.copied_deactivated_files += 1;
                    }
                }
                if target_file.is_file()
                    && !dest_list.iter().any(|d| d.file == target_file)
                {
                    dest_list.push(MigratedSavedEntry {
                        file: target_file,
                        original: g.original,
                        value: g.valor,
                        data: g.datos,
                        origin: translate_origin(&g.origen),
                    });
                }
            }
            let d_bytes = serde_json::to_vec_pretty(&dest_list).map_err(|e| e.to_string())?;
            fs::write(dest_dir.join("deactivated.json"), d_bytes).map_err(|e| e.to_string())?;
        }
    }

    // 4. Settings (ajustes.json -> settings.json) if destination doesn't already have settings
    let old_settings_path = source_dir.join("ajustes.json");
    let new_settings_path = dest_dir.join("settings.json");
    if old_settings_path.is_file() && !new_settings_path.exists() {
        if let Ok(text) = fs::read_to_string(&old_settings_path) {
            if let Ok(old_val) = serde_json::from_str::<Value>(&text) {
                let s = if old_val.get("ajustes").is_some_and(Value::is_object) {
                    &old_val["ajustes"]
                } else {
                    &old_val
                };
                let mapped = json!({
                    "theme": s.get("tema").cloned().unwrap_or(Value::Null),
                    "language": s.get("idioma").and_then(Value::as_str).unwrap_or("en"),
                    "languageChosen": s.get("idiomaElegido").and_then(Value::as_bool).unwrap_or(false),
                    "sampleText": s.get("textoPrueba").and_then(Value::as_str).unwrap_or(""),
                    "view": if s.get("vista").and_then(Value::as_str) == Some("cuadricula") { "grid" } else { "list" },
                    "size": s.get("tamano").and_then(Value::as_u64).unwrap_or(30),
                    "onClose": if s.get("alCerrar").and_then(Value::as_str) == Some("salir") { "exit" } else { "tray" },
                    "systemUnlocked": s.get("desbloqueadas").and_then(Value::as_bool).unwrap_or(false),
                    "showLocked": s.get("verBloqueadas").and_then(Value::as_bool).unwrap_or(false),
                    "installFor": if s.get("instalarPara").and_then(Value::as_str) == Some("todos") { "all" } else { "user" },
                    "quickSwitcherShortcut": "Ctrl+Shift+F"
                });
                if let Ok(out) = serde_json::to_vec_pretty(&mapped) {
                    if fs::write(&new_settings_path, out).is_ok() {
                        report.imported_settings = true;
                    }
                }
            }
        }
    }

    let folder_paths = folders.into_iter().map(PathBuf::from).collect();
    Ok((report, current_lib, folder_paths))
}
