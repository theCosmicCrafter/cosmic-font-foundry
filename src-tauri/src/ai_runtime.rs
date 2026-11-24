//! Managed portable `llama.cpp` (`llama-server.exe`) inference runtime for Cosmic Font Foundry.
//! Zero-prerequisite GPU/RAM/CPU inference: auto-downloads the official Windows Vulkan `llama.cpp`
//! binary (~37 MB) and resumable `VecGlypher-27b-it` GGUF (~16.55 GB), auto-fits layers to available
//! VRAM + system RAM (with automatic CPU fallback), and releases resources when idle or on exit.

use crate::AiPullProgressPayload;
use cosmic_core::inventory::write_json_atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
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
const MIN_CUSTOM_GGUF_BYTES: u64 = 100_000_000; // >100 MB sanity floor for user-supplied GGUFs
const APPROX_GGUF_DOWNLOAD_BYTES: u64 = 17_800_000_000; // ~16.58 GiB
const DISK_HEADROOM_BYTES: u64 = 512 * 1024 * 1024;
const AI_CONFIG_FILE: &str = "ai-config.json";
const DEFAULT_IDLE_SLEEP_SECS: u32 = 900;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn default_compute_mode() -> String {
    "auto".into()
}

fn default_idle_sleep_secs() -> u32 {
    DEFAULT_IDLE_SLEEP_SECS
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiRuntimeConfig {
    #[serde(default = "default_compute_mode")]
    pub compute_mode: String,
    #[serde(default)]
    pub storage_dir: Option<String>,
    #[serde(default)]
    pub custom_gguf_path: Option<String>,
    #[serde(default = "default_idle_sleep_secs")]
    pub idle_sleep_secs: u32,
}

impl Default for AiRuntimeConfig {
    fn default() -> Self {
        Self {
            compute_mode: default_compute_mode(),
            storage_dir: None,
            custom_gguf_path: None,
            idle_sleep_secs: DEFAULT_IDLE_SLEEP_SECS,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct HardwareProbe {
    pub disk_free_bytes: u64,
    pub ram_total_bytes: u64,
    pub ram_avail_bytes: u64,
    pub gpu_summary: Option<String>,
}

pub fn normalize_compute_mode(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gpu" => "gpu".into(),
        "cpu" => "cpu".into(),
        _ => "auto".into(),
    }
}

pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join(AI_CONFIG_FILE)
}

pub fn load_config(data_dir: &Path) -> AiRuntimeConfig {
    let mut cfg = fs::read_to_string(config_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<AiRuntimeConfig>(&s).ok())
        .unwrap_or_default();
    cfg.compute_mode = normalize_compute_mode(&cfg.compute_mode);
    cfg.storage_dir = cfg
        .storage_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cfg.custom_gguf_path = cfg
        .custom_gguf_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cfg
}

pub fn save_config(data_dir: &Path, cfg: &AiRuntimeConfig) -> Result<(), String> {
    let mut clean = cfg.clone();
    clean.compute_mode = normalize_compute_mode(&clean.compute_mode);
    clean.storage_dir = clean
        .storage_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    clean.custom_gguf_path = clean
        .custom_gguf_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    write_json_atomic(&config_path(data_dir), &clean)
}

pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ai-runtime")
}

pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ai-models")
}

pub fn effective_models_dir(data_dir: &Path, cfg: &AiRuntimeConfig) -> PathBuf {
    cfg.storage_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| models_dir(data_dir))
}

pub fn effective_gguf_path(data_dir: &Path, cfg: &AiRuntimeConfig) -> PathBuf {
    if let Some(custom) = cfg
        .custom_gguf_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return PathBuf::from(custom);
    }
    effective_models_dir(data_dir, cfg).join(VECGLYPHER_GGUF_FILENAME)
}

pub fn llama_server_exe(data_dir: &Path) -> PathBuf {
    runtime_dir(data_dir).join("llama-server.exe")
}

pub fn llama_server_log_path(data_dir: &Path) -> PathBuf {
    runtime_dir(data_dir).join("llama-server.log")
}

fn is_valid_gguf_file(path: &Path, min_bytes: u64) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() < min_bytes {
        return false;
    }
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).is_ok() && &magic == b"GGUF"
}

pub fn has_builtin_engine(data_dir: &Path) -> bool {
    llama_server_exe(data_dir).is_file()
}

pub fn has_builtin_model(data_dir: &Path) -> bool {
    let cfg = load_config(data_dir);
    let path = effective_gguf_path(data_dir, &cfg);
    let min_bytes = if cfg.custom_gguf_path.is_some() {
        MIN_CUSTOM_GGUF_BYTES
    } else {
        MIN_VALID_GGUF_BYTES
    };
    is_valid_gguf_file(&path, min_bytes)
}

#[cfg(target_os = "windows")]
fn disk_free_bytes_for_path(target_dir: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut cur = Some(target_dir);
    let mut existing = None;
    while let Some(p) = cur {
        if p.exists() {
            existing = Some(p);
            break;
        }
        cur = p.parent();
    }
    let Some(dir) = existing else {
        return 0;
    };
    let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free_avail: u64 = 0;
    // SAFETY: `wide` is null-terminated UTF-16 and `free_avail` points to valid stack storage.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_avail,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        free_avail
    } else {
        0
    }
}

#[cfg(not(target_os = "windows"))]
fn disk_free_bytes_for_path(_target_dir: &Path) -> u64 {
    0
}

#[cfg(target_os = "windows")]
fn system_memory_bytes() -> (u64, u64) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: `MEMORYSTATUSEX` is a plain Win32 POD struct initialized with its exact byte size.
    unsafe {
        let mut mem: MEMORYSTATUSEX = std::mem::zeroed();
        mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut mem) != 0 {
            (mem.ullTotalPhys, mem.ullAvailPhys)
        } else {
            (0, 0)
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn system_memory_bytes() -> (u64, u64) {
    (0, 0)
}

fn detect_nvidia_gpu_summary() -> Option<String> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.free,memory.total",
        "--format=csv,noheader,nounits",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| !l.trim().is_empty())?
        .to_string();
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next()?.to_string();
    let free_mib: f64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let total_mib: f64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    if total_mib > 0.0 {
        Some(format!(
            "{name} ({:.1} / {:.1} GB VRAM free)",
            free_mib / 1024.0,
            total_mib / 1024.0
        ))
    } else {
        Some(name)
    }
}

pub fn inspect_hardware(target_dir: &Path) -> HardwareProbe {
    let disk_free_bytes = disk_free_bytes_for_path(target_dir);
    let (ram_total_bytes, ram_avail_bytes) = system_memory_bytes();
    let gpu_summary = detect_nvidia_gpu_summary();
    HardwareProbe {
        disk_free_bytes,
        ram_total_bytes,
        ram_avail_bytes,
        gpu_summary,
    }
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

fn part_path_for(dest_path: &Path) -> PathBuf {
    dest_path.with_extension(format!(
        "{}.part",
        dest_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
    ))
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

    let part_path = part_path_for(dest_path);

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

    // Step 2: Download VecGlypher-27b-it Q4_K_M GGUF (~16.55 GB, resumable) into active storage folder
    let cfg = load_config(data_dir);
    let model_dest = effective_gguf_path(data_dir, &cfg);
    if !has_builtin_model(data_dir) {
        if let Some(custom) = cfg.custom_gguf_path.as_deref() {
            return Err(format!(
                "Selected custom GGUF model is missing or invalid ({custom}). \
                 Clear custom GGUF or pick a valid file."
            ));
        }
        let target_dir = model_dest.parent().unwrap_or(data_dir);
        let part_bytes = fs::metadata(part_path_for(&model_dest))
            .map(|m| m.len())
            .unwrap_or(0);
        let required_bytes = APPROX_GGUF_DOWNLOAD_BYTES
            .saturating_sub(part_bytes)
            .saturating_add(DISK_HEADROOM_BYTES);
        let hw = inspect_hardware(target_dir);
        if hw.disk_free_bytes > 0 && hw.disk_free_bytes < required_bytes {
            return Err(format!(
                "Not enough free disk space in {}: {:.1} GB free, {:.1} GB required to download VecGlypher-27b-it (~16.6 GB). Pick another drive via \"Choose Storage Folder…\".",
                target_dir.display(),
                hw.disk_free_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                required_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            ));
        }

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
        "Built-in engine & VecGlypher-27b-it ready!",
        1,
        1,
        100.0,
        true,
        None,
    );

    Ok(())
}

fn read_log_tail(log_path: &Path, max_lines: usize) -> String {
    let Ok(text) = fs::read_to_string(log_path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join(" | ")
}

fn spawn_llama_server(
    data_dir: &Path,
    exe: &Path,
    model: &Path,
    mode: &str,
    idle_sleep_secs: u32,
) -> Result<Child, String> {
    let rdir = runtime_dir(data_dir);
    let _ = fs::create_dir_all(&rdir);
    let log_path = llama_server_log_path(data_dir);
    let (stdout_cfg, stderr_cfg) = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => {
            let err_f = f
                .try_clone()
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null());
            (Stdio::from(f), err_f)
        }
        Err(_) => (Stdio::null(), Stdio::null()),
    };

    let mut cmd = Command::new(exe);
    cmd.current_dir(&rdir)
        .arg("--model")
        .arg(model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(BUILTIN_PORT.to_string())
        .arg("--alias")
        .arg(cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF)
        .arg("--ctx-size")
        .arg("2048")
        .arg("--parallel")
        .arg("1")
        .arg("--flash-attn")
        .arg("auto")
        .arg("--cache-type-k")
        .arg("q8_0")
        .arg("--cache-type-v")
        .arg("q8_0")
        .arg("--no-webui");

    if idle_sleep_secs > 0 {
        cmd.arg("--sleep-idle-seconds")
            .arg(idle_sleep_secs.to_string());
    }

    match mode {
        "gpu" => {
            cmd.arg("--ngl").arg("99");
        }
        "cpu" => {
            cmd.arg("--device").arg("none").arg("--ngl").arg("0");
        }
        _ => {
            // Auto-fit leaves --ngl unset while capping --ctx-size at 2048 (instead of the
            // model's 131K training context) so KV cache stays ~120 MB and layers fit in VRAM.
            cmd.arg("--fit")
                .arg("on")
                .arg("--fit-ctx")
                .arg("2048")
                .arg("--fit-target")
                .arg("1024");
        }
    }

    cmd.stdin(Stdio::null())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to start built-in llama-server.exe: {e}"))
}

async fn wait_for_server_ready(llama_child: &Mutex<Option<Child>>) -> Result<(), Option<String>> {
    // Poll every 500ms up to 150s (300 iterations) so slower CPU/RAM systems also finish loading cleanly.
    for _ in 0..300 {
        if builtin_server_healthy().await {
            return Ok(());
        }
        if let Ok(mut guard) = llama_child.lock() {
            if let Some(ch) = guard.as_mut() {
                if let Ok(Some(exit_status)) = ch.try_wait() {
                    *guard = None;
                    return Err(Some(exit_status.to_string()));
                }
            }
        }
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(Duration::from_millis(500));
        })
        .await;
    }
    Err(None)
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

    let cfg = load_config(data_dir);
    let exe = llama_server_exe(data_dir);
    let model = effective_gguf_path(data_dir, &cfg);
    if !exe.is_file() {
        return Err("Built-in llama-server.exe is not installed yet.".into());
    }
    if !has_builtin_model(data_dir) {
        return Err("VecGlypher-27b-it GGUF model is not downloaded or valid yet.".into());
    }

    let mode = cfg.compute_mode.as_str();
    let stage_msg = match mode {
        "cpu" => "Step 3/3: Loading VecGlypher-27b-it in universal CPU + System RAM mode…",
        "gpu" => "Step 3/3: Loading VecGlypher-27b-it onto GPU VRAM…",
        _ => "Step 3/3: Auto-fitting VecGlypher-27b-it across available GPU + System RAM…",
    };
    emit_progress(app, stage_msg, 0, 0, 98.0, false, None);

    // Clear any dead previous handle before spawning
    kill_managed_server(llama_child);

    let child = spawn_llama_server(data_dir, &exe, &model, mode, cfg.idle_sleep_secs)?;
    if let Ok(mut guard) = llama_child.lock() {
        *guard = Some(child);
    }

    match wait_for_server_ready(llama_child).await {
        Ok(()) => Ok(()),
        Err(Some(exit_status)) if mode == "auto" => {
            emit_progress(
                app,
                "GPU initialization failed — falling back to universal CPU + System RAM mode…",
                0,
                0,
                98.0,
                false,
                None,
            );
            kill_managed_server(llama_child);
            let cpu_child = spawn_llama_server(data_dir, &exe, &model, "cpu", cfg.idle_sleep_secs)?;
            if let Ok(mut guard) = llama_child.lock() {
                *guard = Some(cpu_child);
            }
            match wait_for_server_ready(llama_child).await {
                Ok(()) => Ok(()),
                Err(cpu_exit) => {
                    let tail = read_log_tail(&llama_server_log_path(data_dir), 12);
                    let reason = cpu_exit
                        .map(|s| format!("exited ({s})"))
                        .unwrap_or_else(|| "timed out".into());
                    Err(format!(
                        "Built-in llama-server.exe failed in both Auto-Fit GPU ({exit_status}) and CPU fallback ({reason}). Log: {tail}"
                    ))
                }
            }
        }
        Err(Some(exit_status)) => {
            let tail = read_log_tail(&llama_server_log_path(data_dir), 12);
            Err(format!(
                "Built-in llama-server.exe exited prematurely ({exit_status}) in {mode} mode. Log: {tail}"
            ))
        }
        Err(None) => {
            let tail = read_log_tail(&llama_server_log_path(data_dir), 12);
            Err(format!(
                "Timed out waiting for built-in llama-server.exe to load VecGlypher-27b-it. Log: {tail}"
            ))
        }
    }
}
