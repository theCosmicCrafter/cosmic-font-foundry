//! VecGlypher AI glyph & batch generation and model provisioning commands.

use crate::{ai_runtime, err, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager, State};

// ---- VecGlypher AI Font Generation (Auto GPU / RAM / CPU + Ollama & OpenAI/vLLM) ----

#[derive(Serialize)]
pub(crate) struct AiBackendStatus {
    ok: bool,
    models: Vec<String>,
    has_vecglypher: bool,
    recommended_pull: String,
    message: String,
    server_running: bool,
    compute_mode: String,
    models_dir: String,
    custom_gguf_path: Option<String>,
    disk_free_bytes: u64,
    ram_total_bytes: u64,
    ram_avail_bytes: u64,
    gpu_summary: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AiGlyphRequest {
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
pub(crate) struct AiFontBatchRequest {
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
pub(crate) struct AiGlyphProgressPayload {
    current: usize,
    total: usize,
    character: String,
    status: String,
}

pub(crate) fn resolve_prime_vecglypher_model(model: &str) -> Result<String, String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Ok(cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF.to_string());
    }
    if !trimmed.to_ascii_lowercase().contains("vecglypher") {
        return Err(format!(
            "Prime model lock: refusing to substitute '{}' for VecGlypher-27b-it. \
             Only VecGlypher prime models ({}) are trained to emit native glyph SVG outlines; \
             switching models will corrupt glyph generation.",
            trimmed,
            cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF
        ));
    }
    Ok(trimmed.to_string())
}

pub(crate) async fn ensure_prime_model_downloaded(
    app: Option<&AppHandle>,
    cancel_flag: Option<&AtomicBool>,
    endpoint: &str,
    provider: &str,
    model: &str,
) -> Result<(String, String), String> {
    let prime_model = resolve_prime_vecglypher_model(model)?;
    if provider.eq_ignore_ascii_case("builtin") {
        let Some(a) = app else {
            return Err("Built-in managed engine requires desktop app context.".into());
        };
        let state = a.state::<AppState>();
        ai_runtime::provision_and_start_builtin(
            &state.env.data_dir,
            &state.llama_child,
            Some(a),
            cancel_flag,
        )
        .await?;
        return Ok((ai_runtime::BUILTIN_ENDPOINT.to_string(), prime_model));
    }
    if !provider.eq_ignore_ascii_case("ollama") {
        return Ok((endpoint.to_string(), prime_model));
    }

    let status = probe_remote_ai_backend(endpoint, provider).await?;
    if !status.ok {
        return Err(format!(
            "Local Ollama backend unreachable ({}) — cannot verify or download prime model '{}'.",
            status.message, prime_model
        ));
    }

    let target_lower = prime_model.to_ascii_lowercase();
    if let Some(installed) = status.models.iter().find(|m| {
        let lower = m.to_ascii_lowercase();
        lower == target_lower
            || lower.contains(&target_lower)
            || target_lower.contains(&lower)
            || lower.contains("vecglypher")
    }) {
        return Ok((endpoint.to_string(), installed.clone()));
    }

    // Prime model is not installed yet: halt production and wait for full download & verification.
    pull_ollama_model_internal(app, cancel_flag, endpoint, &prime_model).await?;
    Ok((endpoint.to_string(), prime_model))
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
    let prime_model = resolve_prime_vecglypher_model(model)?;
    let model_name = prime_model.as_str();

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

fn builtin_status_fields(
    data_dir: &Path,
) -> (String, String, Option<String>, ai_runtime::HardwareProbe) {
    let cfg = ai_runtime::load_config(data_dir);
    let models_dir = ai_runtime::effective_models_dir(data_dir, &cfg);
    let hw = ai_runtime::inspect_hardware(&models_dir);
    (
        cfg.compute_mode,
        models_dir.to_string_lossy().into_owned(),
        cfg.custom_gguf_path,
        hw,
    )
}

fn enrich_remote_status(data_dir: &Path, mut status: AiBackendStatus) -> AiBackendStatus {
    let (compute_mode, models_dir, custom_gguf_path, hw) = builtin_status_fields(data_dir);
    status.server_running = false;
    status.compute_mode = compute_mode;
    status.models_dir = models_dir;
    status.custom_gguf_path = custom_gguf_path;
    status.disk_free_bytes = hw.disk_free_bytes;
    status.ram_total_bytes = hw.ram_total_bytes;
    status.ram_avail_bytes = hw.ram_avail_bytes;
    status.gpu_summary = hw.gpu_summary;
    status
}

#[tauri::command]
pub(crate) async fn foundry_ai_check_backend(
    state: State<'_, AppState>,
    endpoint: String,
    provider: String,
) -> Result<AiBackendStatus, String> {
    if provider.eq_ignore_ascii_case("builtin") {
        let has_engine = ai_runtime::has_builtin_engine(&state.env.data_dir);
        let has_model = ai_runtime::has_builtin_model(&state.env.data_dir);
        let healthy = ai_runtime::builtin_server_healthy().await;
        let (compute_mode, models_dir, custom_gguf_path, hw) =
            builtin_status_fields(&state.env.data_dir);
        let ready = has_engine && has_model;
        let models = if ready {
            vec![cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF.to_string()]
        } else {
            Vec::new()
        };
        let mode_label = match compute_mode.as_str() {
            "cpu" => "CPU + System RAM",
            "gpu" => "GPU Only",
            _ => "Auto-Fit GPU + RAM",
        };
        let message = if ready && healthy {
            format!(
                "Built-in llama.cpp running on 127.0.0.1:17421 ({mode_label} · VecGlypher ready)."
            )
        } else if ready {
            format!("Built-in llama.cpp & VecGlypher-27b-it installed ({mode_label} · auto-starts on Generate).")
        } else if has_engine && custom_gguf_path.is_some() {
            "Built-in llama.cpp installed, but selected custom .gguf file is missing or invalid."
                .into()
        } else if has_engine {
            "Built-in llama.cpp engine installed. Click Download to fetch VecGlypher-27b-it (~16.6 GB)."
                .into()
        } else {
            "Zero-setup Built-in Engine — 1-click downloads portable llama.cpp (~37 MB) + VecGlypher-27b-it (~16.6 GB, Auto GPU+RAM / CPU)."
                .into()
        };
        return Ok(AiBackendStatus {
            ok: true,
            models,
            has_vecglypher: ready,
            recommended_pull: "Automatic 1-Click Built-in Install".into(),
            message,
            server_running: healthy,
            compute_mode,
            models_dir,
            custom_gguf_path,
            disk_free_bytes: hw.disk_free_bytes,
            ram_total_bytes: hw.ram_total_bytes,
            ram_avail_bytes: hw.ram_avail_bytes,
            gpu_summary: hw.gpu_summary,
        });
    }
    let remote = probe_remote_ai_backend(&endpoint, &provider).await?;
    Ok(enrich_remote_status(&state.env.data_dir, remote))
}

#[tauri::command]
pub(crate) async fn foundry_ai_set_runtime_config(
    state: State<'_, AppState>,
    compute_mode: Option<String>,
    storage_dir: Option<String>,
    custom_gguf_path: Option<String>,
) -> Result<AiBackendStatus, String> {
    let mut cfg = ai_runtime::load_config(&state.env.data_dir);
    if let Some(m) = compute_mode {
        cfg.compute_mode = ai_runtime::normalize_compute_mode(&m);
    }
    if let Some(dir) = storage_dir {
        let trimmed = dir.trim().to_string();
        cfg.storage_dir = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    if let Some(gguf) = custom_gguf_path {
        let trimmed = gguf.trim().to_string();
        if trimmed.is_empty() {
            cfg.custom_gguf_path = None;
        } else {
            if !Path::new(&trimmed).is_file() {
                return Err(format!("GGUF file not found: {trimmed}"));
            }
            cfg.custom_gguf_path = Some(trimmed);
        }
    }
    ai_runtime::save_config(&state.env.data_dir, &cfg)?;
    ai_runtime::kill_managed_server(&state.llama_child);
    foundry_ai_check_backend(state, ai_runtime::BUILTIN_ENDPOINT.into(), "builtin".into()).await
}

#[tauri::command]
pub(crate) async fn foundry_ai_stop_builtin(
    state: State<'_, AppState>,
) -> Result<AiBackendStatus, String> {
    ai_runtime::kill_managed_server(&state.llama_child);
    foundry_ai_check_backend(state, ai_runtime::BUILTIN_ENDPOINT.into(), "builtin".into()).await
}

#[tauri::command]
pub(crate) async fn foundry_ai_choose_models_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) async fn foundry_ai_choose_gguf_file(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("GGUF Model", &["gguf"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

pub(crate) async fn probe_remote_ai_backend(
    endpoint: &str,
    provider: &str,
) -> Result<AiBackendStatus, String> {
    let recommended_pull = format!(
        "ollama run {}",
        cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF
    );
    let make_err_status = |msg: String| AiBackendStatus {
        ok: false,
        models: Vec::new(),
        has_vecglypher: false,
        recommended_pull: recommended_pull.clone(),
        message: msg,
        server_running: false,
        compute_mode: "auto".into(),
        models_dir: String::new(),
        custom_gguf_path: None,
        disk_free_bytes: 0,
        ram_total_bytes: 0,
        ram_avail_bytes: 0,
        gpu_summary: None,
    };
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Ok(make_err_status("Endpoint URL is empty.".into()));
    }

    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    else {
        return Ok(make_err_status("Failed to initialize HTTP client.".into()));
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
            return Ok(make_err_status(format!(
                "Backend unreachable at {url}: {e}"
            )));
        }
    };

    if !resp.status().is_success() {
        return Ok(make_err_status(format!(
            "Backend returned HTTP {} at {url}",
            resp.status()
        )));
    }

    let val: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(make_err_status(format!("Invalid JSON from {url}: {e}")));
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
        server_running: false,
        compute_mode: "auto".into(),
        models_dir: String::new(),
        custom_gguf_path: None,
        disk_free_bytes: 0,
        ram_total_bytes: 0,
        ram_avail_bytes: 0,
        gpu_summary: None,
    })
}

pub(crate) fn build_glyph_from_contours(
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

fn collect_exemplar_representations(
    glyphs: impl Iterator<Item = (char, cosmic_foundry::model::GlyphOutline)>,
    upm: u16,
) -> (Vec<String>, Vec<(char, String)>) {
    let mut images_b64 = Vec::new();
    let mut svg_exemplars = Vec::new();
    for (ch, g) in glyphs {
        if g.contours.is_empty() {
            continue;
        }
        if images_b64.len() < 8 {
            if let Ok(b64) = cosmic_foundry::vecglypher::render_glyph_exemplar_base64(&g, upm, 192)
            {
                images_b64.push(b64);
            }
        }
        if svg_exemplars.len() < 4 {
            if let Some(path_svg) = cosmic_foundry::vecglypher::glyph_to_compact_svg_path(&g, upm) {
                svg_exemplars.push((ch, path_svg));
            }
        }
    }
    (images_b64, svg_exemplars)
}

#[allow(clippy::too_many_arguments)]
async fn infer_glyph_resilient(
    client: &reqwest::Client,
    endpoint: &str,
    provider: &str,
    model: &str,
    style_prompt: &str,
    ch: char,
    upm: u16,
    temp: f32,
    images_b64: &[String],
    svg_exemplars: &[(char, String)],
) -> Result<Vec<cosmic_foundry::model::Contour>, String> {
    let is_builtin = provider.eq_ignore_ascii_case("builtin");
    let use_mm_images = !is_builtin && !images_b64.is_empty();

    let attempt = |t: f32, use_images: bool| async move {
        let cfg = cosmic_foundry::vecglypher::VecGlypherPromptConfig {
            style_prompt: style_prompt.to_string(),
            target_text: ch.to_string(),
            temperature: t,
            svg_exemplars: if use_images {
                Vec::new()
            } else {
                svg_exemplars.to_vec()
            },
            ..Default::default()
        };
        let imgs = if use_images { images_b64 } else { &[] };
        let raw = run_vecglypher_inference(client, endpoint, provider, model, &cfg, imgs).await?;
        cosmic_foundry::vecglypher::decode_vecglypher_svg(&raw, upm)
    };

    let first = if use_mm_images {
        match attempt(temp, true).await {
            Ok(c) => Ok(c),
            Err(_) => attempt(temp, false).await,
        }
    } else {
        attempt(temp, false).await
    };

    match first {
        Ok(contours) => Ok(contours),
        Err(first_err) => {
            let retry_temp = (temp + 0.35).clamp(0.2, 0.85);
            attempt(retry_temp, false).await.map_err(|_| first_err)
        }
    }
}

#[tauri::command]
pub(crate) async fn foundry_ai_generate_glyph(
    app: AppHandle,
    state: State<'_, AppState>,
    req: AiGlyphRequest,
) -> Result<cosmic_foundry::model::GlyphOutline, String> {
    let ch = req
        .character
        .chars()
        .next()
        .ok_or_else(|| "Target character cannot be empty".to_string())?;
    let upm = if req.upm == 0 { 1000 } else { req.upm };
    let temp = req.temperature.clamp(0.0, 2.0);

    let (effective_endpoint, verified_model) = ensure_prime_model_downloaded(
        Some(&app),
        Some(&state.ai_pull_cancel),
        &req.endpoint,
        &req.provider,
        &req.model,
    )
    .await?;

    let (images_b64, svg_exemplars) = collect_exemplar_representations(
        req.exemplar_glyphs.iter().take(8).cloned().map(|g| {
            let ec = char::from_u32(g.codepoint).unwrap_or('?');
            (ec, g)
        }),
        upm,
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(err)?;

    let contours = infer_glyph_resilient(
        &client,
        &effective_endpoint,
        &req.provider,
        &verified_model,
        &req.style_prompt,
        ch,
        upm,
        temp,
        &images_b64,
        &svg_exemplars,
    )
    .await?;
    Ok(build_glyph_from_contours(ch, contours, upm))
}

#[tauri::command]
pub(crate) async fn foundry_ai_generate_font_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    req: AiFontBatchRequest,
) -> Result<cosmic_foundry::model::FoundryProject, String> {
    state.ai_cancel.store(false, Ordering::SeqCst);

    let (effective_endpoint, verified_model) = ensure_prime_model_downloaded(
        Some(&app),
        Some(&state.ai_pull_cancel),
        &req.endpoint,
        &req.provider,
        &req.model,
    )
    .await?;

    let mut project = req.project;
    let upm = if project.metrics.upm == 0 {
        1000
    } else {
        project.metrics.upm
    };
    let temp = req.temperature.clamp(0.0, 2.0);

    let (mut images_b64, mut svg_exemplars) = collect_exemplar_representations(
        req.exemplar_keys.iter().take(8).filter_map(|key| {
            let g = project.glyphs.get(key)?.clone();
            let ec = char::from_u32(g.codepoint).unwrap_or('?');
            Some((ec, g))
        }),
        upm,
    );

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

    // When no exemplars were pre-selected, run key structural characters first and auto-seed
    // subsequent glyphs from them for consistent weight and proportions across the alphabet.
    let auto_seed = req.exemplar_keys.is_empty() && target_chars.len() > 1;
    let priority_seeds = ['H', 'O', 'A', 'n', 'o'];
    if auto_seed {
        let mut prioritized = Vec::with_capacity(target_chars.len());
        for seed in priority_seeds {
            if let Some(pos) = target_chars.iter().position(|&c| c == seed) {
                prioritized.push(target_chars.remove(pos));
            }
        }
        prioritized.extend(target_chars);
        target_chars = prioritized;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(err)?;

    let total = target_chars.len();
    let mut succeeded = 0usize;
    let mut last_error: Option<String> = None;

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

        match infer_glyph_resilient(
            &client,
            &effective_endpoint,
            &req.provider,
            &verified_model,
            &req.style_prompt,
            ch,
            upm,
            temp,
            &images_b64,
            &svg_exemplars,
        )
        .await
        {
            Ok(contours) => {
                let glyph = build_glyph_from_contours(ch, contours, upm);
                if auto_seed && priority_seeds.contains(&ch) {
                    if svg_exemplars.len() < 4 {
                        if let Some(path_svg) =
                            cosmic_foundry::vecglypher::glyph_to_compact_svg_path(&glyph, upm)
                        {
                            svg_exemplars.push((ch, path_svg));
                        }
                    }
                    if !req.provider.eq_ignore_ascii_case("builtin") && images_b64.len() < 4 {
                        if let Ok(b64) = cosmic_foundry::vecglypher::render_glyph_exemplar_base64(
                            &glyph, upm, 192,
                        ) {
                            images_b64.push(b64);
                        }
                    }
                }
                project.glyphs.insert((ch as u32).to_string(), glyph);
                succeeded += 1;
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    if succeeded == 0 && !state.ai_cancel.load(Ordering::SeqCst) {
        return Err(format!(
            "Batch generation failed to produce any valid glyphs: {}",
            last_error.unwrap_or_else(|| "unknown inference error".into())
        ));
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
pub(crate) async fn foundry_ai_cancel_batch(state: State<'_, AppState>) -> Result<bool, String> {
    state.ai_cancel.store(true, Ordering::SeqCst);
    Ok(true)
}

#[derive(Clone, Serialize)]
pub(crate) struct AiPullProgressPayload {
    pub(crate) status: String,
    pub(crate) completed: u64,
    pub(crate) total: u64,
    pub(crate) percent: f32,
    pub(crate) done: bool,
    pub(crate) error: Option<String>,
}

pub(crate) async fn pull_ollama_model_internal(
    app: Option<&AppHandle>,
    cancel_flag: Option<&AtomicBool>,
    endpoint: &str,
    model: &str,
) -> Result<(), String> {
    if let Some(flag) = cancel_flag {
        flag.store(false, Ordering::SeqCst);
    }

    let prime_model = resolve_prime_vecglypher_model(model)?;
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Endpoint URL is empty.".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(err)?;

    let url = format!("{base}/api/pull");
    let payload = json!({
        "name": prime_model,
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
        if cancel_flag.is_some_and(|f| f.load(Ordering::SeqCst)) {
            if let Some(a) = app {
                let _ = a.emit(
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
            }
            return Err("Prime model download cancelled by user — halting generation.".into());
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
                    if let Some(a) = app {
                        let _ = a.emit(
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
                    }
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

                if let Some(a) = app {
                    let _ = a.emit(
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
                }

                if done {
                    return Ok(());
                }
            }
        }
    }

    Err(
        "Prime model download stream ended before verification completed — halting generation."
            .into(),
    )
}

#[tauri::command]
pub(crate) async fn foundry_ai_pull_model(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
    model: String,
    provider: Option<String>,
) -> Result<(), String> {
    if provider
        .as_deref()
        .unwrap_or("ollama")
        .eq_ignore_ascii_case("builtin")
    {
        return ai_runtime::provision_and_start_builtin(
            &state.env.data_dir,
            &state.llama_child,
            Some(&app),
            Some(&state.ai_pull_cancel),
        )
        .await;
    }
    pull_ollama_model_internal(Some(&app), Some(&state.ai_pull_cancel), &endpoint, &model).await
}

#[tauri::command]
pub(crate) async fn foundry_ai_cancel_pull(state: State<'_, AppState>) -> Result<bool, String> {
    state.ai_pull_cancel.store(true, Ordering::SeqCst);
    Ok(true)
}
