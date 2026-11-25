//! Cosmic Foundry vector studio, glyph tracing, auto-metrics, and similarity commands.

use crate::{bg, err, google, vault, AppState};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter, Manager, State};

// ---- Cosmic Foundry Vector Studio & Search Engines ----

#[tauri::command]
pub(crate) async fn foundry_compile_modular(
    req: cosmic_foundry::model::ModularForgeRequest,
) -> Result<cosmic_foundry::model::CompiledFontResult, String> {
    bg(move || {
        let project = cosmic_foundry::modular::modular_to_project(&req);
        cosmic_foundry::compiler::compile_ttf(&project, false)
    })
    .await
}

#[tauri::command]
pub(crate) async fn foundry_modular_to_project(
    req: cosmic_foundry::model::ModularForgeRequest,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || Ok(cosmic_foundry::modular::modular_to_project(&req))).await
}

#[tauri::command]
pub(crate) async fn foundry_compile_project(
    project: cosmic_foundry::model::FoundryProject,
    clean_outlines: bool,
) -> Result<cosmic_foundry::model::CompiledFontResult, String> {
    bg(move || cosmic_foundry::compiler::compile_ttf(&project, clean_outlines)).await
}

#[tauri::command]
pub(crate) async fn foundry_compile_woff(ttf_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    bg(move || cosmic_foundry::compiler::compile_woff(&ttf_bytes)).await
}

#[tauri::command]
pub(crate) async fn foundry_fork_font(
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
pub(crate) async fn foundry_inspect_glyph(
    path: String,
    codepoint: u32,
) -> Result<cosmic_foundry::model::GlyphInspectInfo, String> {
    bg(move || cosmic_foundry::import::inspect_glyph_outline(Path::new(&path), codepoint)).await
}

#[tauri::command]
pub(crate) async fn foundry_import_svg_glyph(
    svg: String,
    upm: u16,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || cosmic_foundry::import::import_svg_glyph(&svg, upm)).await
}

#[tauri::command]
pub(crate) async fn foundry_transform_glyph(
    contours: Vec<cosmic_foundry::model::Contour>,
    op: cosmic_foundry::model::GlyphTransformOp,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || Ok(cosmic_foundry::pathops::apply_transform(&contours, &op))).await
}

#[tauri::command]
pub(crate) async fn foundry_trace_image(
    bytes: Vec<u8>,
    opts: cosmic_foundry::trace::TraceOptions,
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    bg(move || cosmic_foundry::trace::trace_image_bytes(&bytes, &opts)).await
}

#[tauri::command]
pub(crate) async fn foundry_trace_sheet(
    bytes: Vec<u8>,
    cols: usize,
    rows: usize,
    chars: String,
    opts: cosmic_foundry::trace::TraceOptions,
) -> Result<std::collections::BTreeMap<String, Vec<cosmic_foundry::model::Contour>>, String> {
    bg(move || cosmic_foundry::trace::trace_glyph_sheet(&bytes, cols, rows, &chars, &opts)).await
}

#[tauri::command]
pub(crate) async fn foundry_measure_stems(
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
pub(crate) async fn foundry_slice_contours(
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
pub(crate) async fn foundry_outline_stroke(
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

#[derive(serde::Serialize)]
pub(crate) struct FoundryWorkspaceInfo {
    vault_dir: String,
    exports_dir: String,
    ufo_projects_dir: String,
    ufo_projects: Vec<vault::ForgedUfoEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct WorkspaceExportResult {
    export_path: String,
    vault_archived: bool,
    vault_path: Option<String>,
}

fn slug_for_file(family: &str, style: &str) -> String {
    let slug: String = format!("{family}-{style}")
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "cosmic-font".to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub(crate) async fn foundry_workspace_info(
    state: State<'_, AppState>,
) -> Result<FoundryWorkspaceInfo, String> {
    let data_dir = state.env.data_dir.clone();
    bg(move || {
        let vault_dir = vault::load_vault_path(&data_dir);
        let exports_dir = vault::exports_dir(&data_dir);
        let ufo_projects_dir = vault::ufo_projects_dir(&data_dir);
        let ufo_projects = vault::list_ufo_projects(&data_dir);
        Ok(FoundryWorkspaceInfo {
            vault_dir: vault_dir.to_string_lossy().into_owned(),
            exports_dir: exports_dir.to_string_lossy().into_owned(),
            ufo_projects_dir: ufo_projects_dir.to_string_lossy().into_owned(),
            ufo_projects,
        })
    })
    .await
}

#[tauri::command]
pub(crate) fn foundry_open_workspace_folder(
    state: State<'_, AppState>,
    kind: String,
) -> Result<String, String> {
    let target = match kind.trim().to_ascii_lowercase().as_str() {
        "vault" => vault::load_vault_path(&state.env.data_dir),
        "exports" => vault::exports_dir(&state.env.data_dir),
        "ufo_projects" | "projects" | "ufo" => vault::ufo_projects_dir(&state.env.data_dir),
        other => return Err(format!("Unknown workspace folder kind: {other}")),
    };
    fs::create_dir_all(&target).map_err(err)?;
    let target_str = target.to_string_lossy().into_owned();
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&target)
            .spawn()
            .map_err(err)?;
    }
    Ok(target_str)
}

#[tauri::command]
pub(crate) async fn foundry_save_ufo_to_workspace(
    state: State<'_, AppState>,
    project: cosmic_foundry::model::FoundryProject,
) -> Result<String, String> {
    let ufo_root = vault::ufo_projects_dir(&state.env.data_dir);
    let slug = slug_for_file(&project.family_name, &project.style_name);
    let dest = ufo_root.join(format!("{slug}.ufo"));
    bg(move || {
        fs::create_dir_all(&ufo_root).map_err(err)?;
        cosmic_foundry::compiler::export_ufo(&project, &dest)?;
        Ok(dest.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub(crate) async fn foundry_import_ufo_path(
    path: String,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    let src = PathBuf::from(path.trim());
    if !src.is_dir()
        || !src
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ufo"))
    {
        return Err(format!("Not a valid .ufo directory: {}", src.display()));
    }
    bg(move || cosmic_foundry::compiler::import_ufo(&src)).await
}

#[tauri::command]
pub(crate) async fn foundry_export_font_to_workspace(
    app: AppHandle,
    family_name: String,
    style_name: String,
    ext: String,
    bytes: Vec<u8>,
    archive_to_vault: bool,
) -> Result<WorkspaceExportResult, String> {
    let clean_ext = match ext
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "ttf" => "ttf",
        "otf" => "otf",
        "woff" => "woff",
        "woff2" => "woff2",
        other => return Err(format!("Unsupported export extension: {other}")),
    };
    let data_dir = app.state::<AppState>().env.data_dir.clone();
    let family_folder = vault::sanitize_workspace_segment(&family_name, "Cosmic Font");
    let slug = slug_for_file(&family_name, &style_name);
    let exports_family_dir = vault::exports_dir(&data_dir).join(&family_folder);
    let export_file = exports_family_dir.join(format!("{slug}.{clean_ext}"));
    let app_bg = app.clone();

    let (result, updated_families) = bg(move || {
        fs::create_dir_all(&exports_family_dir).map_err(err)?;
        fs::write(&export_file, &bytes).map_err(err)?;

        let mut vault_archived = false;
        let mut vault_path_out = None;
        let mut updated_families = None;

        if archive_to_vault && matches!(clean_ext, "ttf" | "otf") {
            let state = app_bg.state::<AppState>();
            let vault_dir = vault::load_vault_path(&state.env.data_dir);
            if let Ok(report) =
                vault::import_to_vault(&vault_dir, std::slice::from_ref(&export_file), false)
            {
                vault_archived = report.imported > 0 || report.skipped_duplicates > 0;
                vault_path_out = Some(vault_dir.to_string_lossy().into_owned());
                crate::ensure_vault_watched(&app_bg, &state, &vault_dir);
                let (families, _, _, _) = state.refresh();
                updated_families = Some(families);
            }
        }

        Ok((
            WorkspaceExportResult {
                export_path: export_file.to_string_lossy().into_owned(),
                vault_archived,
                vault_path: vault_path_out,
            },
            updated_families,
        ))
    })
    .await?;

    if let Some(families) = updated_families {
        let _ = app.emit("inventory-updated", &families);
        let _ = app.emit_to("quick", "inventory-updated", &families);
    }
    Ok(result)
}

#[tauri::command]
pub(crate) async fn foundry_export_ufo(
    app: AppHandle,
    state: State<'_, AppState>,
    project: cosmic_foundry::model::FoundryProject,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let ufo_dir = vault::ufo_projects_dir(&state.env.data_dir);
    let _ = fs::create_dir_all(&ufo_dir);
    let default_name = format!(
        "{}.ufo",
        slug_for_file(&project.family_name, &project.style_name)
    );
    let Some(picked) = app
        .dialog()
        .file()
        .set_directory(&ufo_dir)
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
pub(crate) async fn foundry_import_ufo(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<cosmic_foundry::model::FoundryProject>, String> {
    use tauri_plugin_dialog::DialogExt;
    let ufo_dir = vault::ufo_projects_dir(&state.env.data_dir);
    let _ = fs::create_dir_all(&ufo_dir);
    let Some(picked) = app
        .dialog()
        .file()
        .set_directory(&ufo_dir)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let src = picked.into_path().map_err(err)?;
    bg(move || Ok(Some(cosmic_foundry::compiler::import_ufo(&src)?))).await
}

#[tauri::command]
pub(crate) async fn foundry_subset_font(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    text: String,
    axes: Vec<(String, f32)>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let exports_dir = vault::exports_dir(&state.env.data_dir);
    let _ = fs::create_dir_all(&exports_dir);
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
        .set_directory(&exports_dir)
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
pub(crate) async fn discover_search_github_fonts(
    query: String,
) -> Result<Vec<google::GithubFontRepo>, String> {
    google::search_github_fonts(&query).await
}

#[tauri::command]
pub(crate) async fn discover_search_fontesk(
    query: String,
) -> Result<Vec<google::GoogleFontEntry>, String> {
    google::search_fontesk_fonts(&query).await
}

#[derive(serde::Serialize)]
pub(crate) struct CompositesResult {
    project: cosmic_foundry::model::FoundryProject,
    created: usize,
}

#[tauri::command]
pub(crate) async fn foundry_auto_space(
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
pub(crate) async fn foundry_auto_kern(
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
pub(crate) async fn foundry_generate_composites(
    mut project: cosmic_foundry::model::FoundryProject,
) -> Result<CompositesResult, String> {
    bg(move || {
        let created = cosmic_foundry::typography::generate_latin_composites(&mut project);
        Ok(CompositesResult { project, created })
    })
    .await
}

#[tauri::command]
pub(crate) async fn foundry_apply_linked_metrics(
    mut project: cosmic_foundry::model::FoundryProject,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    bg(move || {
        cosmic_foundry::typography::apply_linked_metrics(&mut project);
        Ok(project)
    })
    .await
}

#[derive(serde::Serialize)]
pub(crate) struct AutofixResult {
    project: cosmic_foundry::model::FoundryProject,
    fixed: usize,
    report: cosmic_foundry::qa::QaReport,
}

#[tauri::command]
pub(crate) async fn foundry_generate_style(
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
pub(crate) async fn foundry_compile_variable_ttf(
    app: AppHandle,
    state: State<'_, AppState>,
    base: cosmic_foundry::model::FoundryProject,
    masters: Vec<cosmic_foundry::variable::VariableMaster>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let exports_dir = vault::exports_dir(&state.env.data_dir);
    let _ = fs::create_dir_all(&exports_dir);
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
        .set_directory(&exports_dir)
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
pub(crate) async fn foundry_audit_project(
    project: cosmic_foundry::model::FoundryProject,
) -> Result<cosmic_foundry::qa::QaReport, String> {
    bg(move || Ok(cosmic_foundry::qa::audit_project(&project))).await
}

#[tauri::command]
pub(crate) async fn foundry_autofix_project(
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
pub(crate) async fn library_visual_match_image(
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
pub(crate) async fn library_visual_match_svg(
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
pub(crate) async fn foundry_vibe_search(
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
pub(crate) async fn foundry_find_outline_duplicates(
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
pub(crate) async fn foundry_triage_archive(
    path: String,
) -> Result<cosmic_foundry::ingest::ArchiveTriageReport, String> {
    bg(move || cosmic_foundry::ingest::triage_path(Path::new(&path))).await
}

#[tauri::command]
pub(crate) async fn foundry_pick_triage_file(app: AppHandle) -> Result<Option<String>, String> {
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
