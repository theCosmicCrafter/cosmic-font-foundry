//! Cosmic Foundry vector studio, glyph tracing, auto-metrics, and similarity commands.

use crate::{bg, err, google, AppState};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};

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

#[tauri::command]
pub(crate) async fn foundry_export_ufo(
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
pub(crate) async fn foundry_import_ufo(
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
pub(crate) async fn foundry_subset_font(
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
pub(crate) async fn discover_search_github_fonts(
    query: String,
) -> Result<Vec<google::GithubFontRepo>, String> {
    google::search_github_fonts(&query).await
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
