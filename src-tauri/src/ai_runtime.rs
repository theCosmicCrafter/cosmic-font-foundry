//! Managed portable `llama.cpp` (`llama-server.exe`) inference runtime for Cosmic Font Foundry.
//! Zero-prerequisite GPU/CPU inference: auto-downloads the official Windows Vulkan `llama.cpp`
//! binary (~37 MB) and resumable `VecGlypher-27b-it` GGUF (~16.55 GB) into `%LOCALAPPDATA%`,
//! spawns `llama-server.exe` on `127.0.0.1:17421`, and terminates it cleanly on app exit.

use crate::AiPullProgressPayload;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};

pub const BUILTIN_PORT: u16 = 17421;
pub const BUILTIN_ENDPOINT: &str = "http://127.0.0.1:17421";
pub const VECGLYPHER_GGUF_FILENAME: &str = "vecglypher-27b-it-q4_k_m.gguf";
pub const VECGLYPHER_HF_GGUF_URL: &str = "https://huggingface.co/wahidmounir/VecGlypher-27b-it-Q4_K_M-GGUF/resolve/main/vecglypher-27b-it-q4_k_m.gguf";
const LLAMA_CPP_RELEASES_API: &str =
    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=5";
const MIN_VALID_GGUF_BYTES: u64 = 1_000_000_000; // >1 GB sanity floor for 27B Q4_K_M

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ai-runtime")
}

pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ai-models")
}

pub fn llama_server_exe(data_dir: &Path) -> PathBuf {
    runtime_dir(data_dir).join("llama-server.exe")
}

pub fn vecglypher_gguf_path(data_dir: &Path) -> PathBuf {
    models_dir(data_dir).join(VECGLYPHER_GGUF_FILENAME)
}

pub fn has_builtin_engine(data_dir: &Path) -> bool {
    llama_server_exe(data_dir).is_file()
}

pub fn has_builtin_model(data_dir: &Path) -> bool {
    fs::metadata(vecglypher_gguf_path(data_dir))
        .map(|m| m.is_file() && m.len() >= MIN_VALID_GGUF_BYTES)
        .unwrap_or(false)
}

pub fn kill_managed_server(slot: &Mutex<Option<Child>>) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub async fn builtin_server_healthy() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    let url = format!("{BUILTIN_ENDPOINT}/health");
    client
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn emit_progress(
    app: Option<&AppHandle>,
    status: impl Into<String>,
    completed: u64,
    total: u64,
    percent: f32,
    done: bool,
    error: Option<String>,
) {
    if let Some(a) = app {
        let _ = a.emit(
            "ai-pull-progress",
            AiPullProgressPayload {
                status: status.into(),
                completed,
                total,
                percent,
                done,
                error,
            },
        );
    }
}

/// Resolve the latest official Windows Vulkan (or CPU fallback) `llama.cpp` release zip URL.
async fn resolve_llama_cpp_win_zip(client: &reqwest::Client) -> Result<(String, String), String> {
    let resp = client
        .get(LLAMA_CPP_RELEASES_API)
        .header("User-Agent", "Cosmic-Font-Foundry")
        .send()
        .await
        .map_err(|e| format!("Failed to query llama.cpp releases: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "GitHub releases API returned HTTP {} while locating llama.cpp runtime",
            resp.status()
        ));
    }

    let releases: Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid GitHub releases JSON: {e}"))?;

    let arr = releases
        .as_array()
        .ok_or_else(|| "Unexpected GitHub releases response shape".to_string())?;

    // Prefer Vulkan x64 (universal NVIDIA/AMD/Intel GPU acceleration in a single ~37 MB zip),
    // falling back to CPU x64 if Vulkan is absent.
    let mut cpu_fallback: Option<(String, String)> = None;
    for rel in arr {
        let Some(assets) = rel["assets"].as_array() else {
            continue;
        };
        for asset in assets {
            let name = asset["name"].as_str().unwrap_or("");
            let url = asset["browser_download_url"].as_str().unwrap_or("");
            if name.is_empty() || url.is_empty() {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("llama-") && lower.ends_with("-bin-win-vulkan-x64.zip") {
                return Ok((name.to_string(), url.to_string()));
            }
            if cpu_fallback.is_none()
                && lower.starts_with("llama-")
                && lower.ends_with("-bin-win-cpu-x64.zip")
            {
                cpu_fallback = Some((name.to_string(), url.to_string()));
            }
        }
    }

    cpu_fallback.ok_or_else(|| {
        "Could not locate a Windows x64 llama.cpp release archive on GitHub.".to_string()
    })
}

/// Download a file with streaming progress, cancellation checks, and optional HTTP Range resume.
async fn download_streaming_file(
    client: &reqwest::Client,
    url: &str,
    dest_path: &Path,
    phase_label: &str,
    resumable: bool,
    app: Option<&AppHandle>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    let part_path = dest_path.with_extension(format!(
        "{}.part",
        dest_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
    ));

    let mut existing_bytes = if resumable {
        fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0)
    } else {
        let _ = fs::remove_file(&part_path);
        0
    };

    let mut req = client.get(url).header("User-Agent", "Cosmic-Font-Foundry");
    if existing_bytes > 0 {
        req = req.header("Range", format!("bytes={existing_bytes}-"));
    }

    let mut resp = req
        .send()
        .await
        .map_err(|e| format!("Download request failed ({url}): {e}"))?;

    // If server ignored Range (200 instead of 206), restart cleanly from byte 0.
    let status = resp.status();
    if existing_bytes > 0 && status == reqwest::StatusCode::OK {
        existing_bytes = 0;
    } else if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && existing_bytes > 0 {
            let _ = fs::remove_file(&part_path);
            existing_bytes = 0;
            resp = client
                .get(url)
                .header("User-Agent", "Cosmic-Font-Foundry")
                .send()
                .await
                .map_err(|e| format!("Retry download request failed ({url}): {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Download returned HTTP {}", resp.status()));
            }
        } else {
            return Err(format!("Download returned HTTP {status} for {url}"));
        }
    }

    let remaining = resp.content_length().unwrap_or(0);
    let total = if remaining > 0 {
        existing_bytes + remaining
    } else {
        0
    };

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(existing_bytes > 0)
        .truncate(existing_bytes == 0)
        .open(&part_path)
        .map_err(|e| {
            format!(
                "Failed to open download target {}: {e}",
                part_path.display()
            )
        })?;

    let mut completed = existing_bytes;
    let mut last_emit_bytes = completed;

    emit_progress(
        app,
        phase_label,
        completed,
        total,
        if total > 0 {
            ((completed as f64 / total as f64) * 100.0) as f32
        } else {
            0.0
        },
        false,
        None,
    );

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Download stream interrupted: {e}"))?
    {
        if cancel_flag.is_some_and(|f| f.load(Ordering::SeqCst)) {
            let _ = file.flush();
            emit_progress(
                app,
                "Download paused/cancelled by user (progress saved for resume).",
                completed,
                total,
                if total > 0 {
                    ((completed as f64 / total as f64) * 100.0) as f32
                } else {
                    0.0
                },
                true,
                Some("Cancelled".into()),
            );
            return Err("Download cancelled by user — halting generation.".into());
        }

        file.write_all(&chunk)
            .map_err(|e| format!("Disk write failed during download: {e}"))?;
        completed += chunk.len() as u64;

        // Throttle progress events to ~every 2 MB so IPC stays silky smooth
        if completed.saturating_sub(last_emit_bytes) >= 2 * 1024 * 1024 || completed == total {
            last_emit_bytes = completed;
            let pct = if total > 0 {
                ((completed as f64 / total as f64) * 100.0) as f32
            } else {
                0.0
            };
            emit_progress(app, phase_label, completed, total, pct, false, None);
        }
    }

    file.flush()
        .map_err(|e| format!("Failed to flush downloaded file: {e}"))?;
    drop(file);

    if dest_path.exists() {
        let _ = fs::remove_file(dest_path);
    }
    fs::rename(&part_path, dest_path)
        .map_err(|e| format!("Failed to finalize {}: {e}", dest_path.display()))?;

    Ok(())
}

/// Extract a downloaded zip into `dest_dir` using Windows' built-in `tar.exe` (zero extra deps).
fn extract_zip_to_dir(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create runtime dir: {e}"))?;
    let mut cmd = Command::new("tar.exe");
    cmd.arg("-xf")
        .arg(zip_path)
        .arg("-C")
        .arg(dest_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to invoke tar.exe to extract llama.cpp runtime: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Runtime extraction failed: {stderr}"));
    }
    Ok(())
}

/// Ensure the built-in `llama-server.exe` runtime and `VecGlypher-27b-it` GGUF are downloaded,
/// extracted, and running on `127.0.0.1:17421`.
pub async fn provision_and_start_builtin(
    data_dir: &Path,
    llama_child: &Mutex<Option<Child>>,
    app: Option<&AppHandle>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(), String> {
    if let Some(flag) = cancel_flag {
        flag.store(false, Ordering::SeqCst);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(7200))
        .build()
        .map_err(|e| e.to_string())?;

    // Step 1: Download & extract portable llama.cpp Vulkan GPU engine (~37 MB)
    if !has_builtin_engine(data_dir) {
        emit_progress(
            app,
            "Step 1/3: Locating built-in llama.cpp GPU engine…",
            0,
            0,
            1.0,
            false,
            None,
        );
        let (asset_name, zip_url) = resolve_llama_cpp_win_zip(&client).await?;
        let rdir = runtime_dir(data_dir);
        let zip_dest = rdir.join(&asset_name);

        download_streaming_file(
            &client,
            &zip_url,
            &zip_dest,
            &format!("Step 1/3: Downloading built-in GPU engine ({asset_name})…"),
            false,
            app,
            cancel_flag,
        )
        .await?;

        emit_progress(
            app,
            "Step 1/3: Extracting built-in GPU engine…",
            0,
            0,
            99.0,
            false,
            None,
        );
        extract_zip_to_dir(&zip_dest, &rdir)?;
        let _ = fs::remove_file(&zip_dest);

        if !has_builtin_engine(data_dir) {
            return Err(format!(
                "Extracted runtime archive but llama-server.exe was not found in {}",
                rdir.display()
            ));
        }
    }

    // Step 2: Download VecGlypher-27b-it Q4_K_M GGUF (~16.55 GB, resumable)
    if !has_builtin_model(data_dir) {
        let model_dest = vecglypher_gguf_path(data_dir);
        download_streaming_file(
            &client,
            VECGLYPHER_HF_GGUF_URL,
            &model_dest,
            "Step 2/3: Downloading VecGlypher-27b-it GGUF (~16.55 GB)…",
            true,
            app,
            cancel_flag,
        )
        .await?;
    }

    // Step 3: Launch & verify managed llama-server.exe on 127.0.0.1:17421
    ensure_builtin_server_running(data_dir, llama_child, app).await?;

    emit_progress(
        app,
        "Built-in GPU engine & VecGlypher-27b-it ready!",
        1,
        1,
        100.0,
        true,
        None,
    );

    Ok(())
}

/// Start `llama-server.exe` in the background (hidden console) if not already listening on `:17421`.
pub async fn ensure_builtin_server_running(
    data_dir: &Path,
    llama_child: &Mutex<Option<Child>>,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    if builtin_server_healthy().await {
        return Ok(());
    }

    let exe = llama_server_exe(data_dir);
    let model = vecglypher_gguf_path(data_dir);
    if !exe.is_file() {
        return Err("Built-in llama-server.exe is not installed yet.".into());
    }
    if !has_builtin_model(data_dir) {
        return Err("VecGlypher-27b-it GGUF model is not downloaded yet.".into());
    }

    emit_progress(
        app,
        "Step 3/3: Loading VecGlypher-27b-it into GPU memory…",
        0,
        0,
        98.0,
        false,
        None,
    );

    // Clear any dead previous handle before spawning
    kill_managed_server(llama_child);

    let mut cmd = Command::new(&exe);
    cmd.current_dir(runtime_dir(data_dir))
        .arg("--model")
        .arg(&model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(BUILTIN_PORT.to_string())
        .arg("--ngl")
        .arg("99")
        .arg("--ctx-size")
        .arg("4096")
        .arg("--alias")
        .arg(cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start built-in llama-server.exe: {e}"))?;

    if let Ok(mut guard) = llama_child.lock() {
        *guard = Some(child);
    }

    // Wait up to 90 seconds for 16.5 GB model weights to load into GPU VRAM
    for _ in 0..90 {
        if builtin_server_healthy().await {
            return Ok(());
        }
        // Check if child exited prematurely
        if let Ok(mut guard) = llama_child.lock() {
            if let Some(ch) = guard.as_mut() {
                if let Ok(Some(exit_status)) = ch.try_wait() {
                    *guard = None;
                    return Err(format!(
                        "Built-in llama-server.exe exited prematurely ({exit_status}). \
                         Ensure your GPU has >=18 GB VRAM (or sufficient system RAM) for VecGlypher-27b-it."
                    ));
                }
            }
        }
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(Duration::from_secs(1));
        })
        .await;
    }

    Err("Timed out waiting for built-in llama-server.exe to load VecGlypher-27b-it.".into())
}
