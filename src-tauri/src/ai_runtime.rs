//! Managed portable `llama.cpp` (`llama-server.exe`) inference runtime for Cosmic Font Foundry.
//! Zero-prerequisite GPU/RAM/CPU inference: auto-downloads the official Windows Vulkan `llama.cpp`
//! binary (~37 MB) and resumable `VecGlypher-27b-it` GGUF (~16.55 GB), auto-fits layers to available
//! VRAM + system RAM (with automatic CPU fallback), and releases resources when idle or on exit.

use crate::AiPullProgressPayload;
use cosmic_core::inventory::write_json_atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
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
    #[serde(default)]
    pub custom_engine_path: Option<String>,
    #[serde(default = "default_idle_sleep_secs")]
    pub idle_sleep_secs: u32,
}

impl Default for AiRuntimeConfig {
    fn default() -> Self {
        Self {
            compute_mode: default_compute_mode(),
            storage_dir: None,
            custom_gguf_path: None,
            custom_engine_path: None,
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
    pub engine_backend: String,
    pub active_tier: Option<String>,
    pub gpu_hold: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NvidiaGpuTelemetry {
    pub name: String,
    pub driver_major: u32,
    pub free_mib: u64,
    pub total_mib: u64,
    pub util_pct: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerCliCaps {
    pub has_no_ui: bool,
    pub has_no_webui: bool,
    pub has_n_gpu_layers: bool,
    pub has_no_repack: bool,
    pub has_flash_attn: bool,
    pub has_cache_type: bool,
    pub has_cache_reuse: bool,
    pub has_slot_similarity: bool,
    pub has_spec_type: bool,
    pub has_sleep_idle: bool,
    pub has_split_mode: bool,
    pub has_main_gpu: bool,
    pub has_fit: bool,
    pub is_cuda_backend: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VramTier {
    FullGpu,
    HybridGpu {
        layers: u32,
        kv_type: &'static str,
        batch_size: u32,
        ubatch_size: u32,
    },
    VulkanAutoFit,
    CpuOnly,
}

#[derive(Clone, Debug)]
pub struct LaunchAttempt {
    pub tier: VramTier,
    pub label: String,
}

static CLI_CAPS_CACHE: OnceLock<Mutex<HashMap<PathBuf, ServerCliCaps>>> = OnceLock::new();
static ACTIVE_TIER_LABEL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn get_active_tier_label() -> Option<String> {
    ACTIVE_TIER_LABEL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|g| g.clone())
}

fn set_active_tier_label(label: Option<String>) {
    if let Ok(mut g) = ACTIVE_TIER_LABEL.get_or_init(|| Mutex::new(None)).lock() {
        *g = label;
    }
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
    cfg.custom_engine_path = cfg
        .custom_engine_path
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
    clean.custom_engine_path = clean
        .custom_engine_path
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

fn parse_build_number(dir_name: &str) -> u64 {
    dir_name
        .split(['-', '_'])
        .filter_map(|part| part.strip_prefix('b').or(Some(part)))
        .filter_map(|digits| digits.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

fn find_system_cuda_llama_server() -> Option<(PathBuf, &'static str)> {
    let mut roots: Vec<PathBuf> = vec![PathBuf::from(r"C:\tools")];
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(profile);
        roots.push(p.join("tools"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(local));
    }

    let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !dir_name.contains("llama") {
                continue;
            }
            let exe = dir.join("llama-server.exe");
            let cuda_dll = dir.join("ggml-cuda.dll");
            if exe.is_file() && cuda_dll.is_file() {
                candidates.push((parse_build_number(&dir_name), exe));
            }
        }
    }
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, exe)| (exe, "CUDA (Local System)"))
}

pub fn resolve_active_llama_server_exe(
    data_dir: &Path,
    cfg: &AiRuntimeConfig,
    nvidia_present: bool,
) -> Option<(PathBuf, &'static str)> {
    if let Some(custom) = cfg.custom_engine_path.as_deref().map(PathBuf::from) {
        if custom.is_file() {
            let label = if custom
                .parent()
                .is_some_and(|p| p.join("ggml-cuda.dll").is_file())
            {
                "CUDA (Custom Engine)"
            } else {
                "Custom llama.cpp"
            };
            return Some((custom, label));
        }
    }
    if let Ok(env_exe) = std::env::var("COSMIC_LLAMA_SERVER") {
        let p = PathBuf::from(env_exe.trim());
        if p.is_file() {
            let label = if p
                .parent()
                .is_some_and(|d| d.join("ggml-cuda.dll").is_file())
            {
                "CUDA (Env)"
            } else {
                "Env llama.cpp"
            };
            return Some((p, label));
        }
    }

    let rdir = runtime_dir(data_dir);
    let builtin_exe = rdir.join("llama-server.exe");
    let builtin_has_cuda = rdir.join("ggml-cuda.dll").is_file();
    if builtin_exe.is_file() && builtin_has_cuda {
        return Some((builtin_exe, "CUDA (Built-in)"));
    }

    if nvidia_present {
        if let Some(found) = find_system_cuda_llama_server() {
            return Some(found);
        }
    }

    if builtin_exe.is_file() {
        let label = if rdir.join("ggml-vulkan.dll").is_file() {
            "Vulkan (Built-in)"
        } else {
            "CPU (Built-in)"
        };
        return Some((builtin_exe, label));
    }

    None
}

pub fn gpu_hold_active() -> bool {
    if std::env::var("RESONANCE_NO_GPU").ok().as_deref() == Some("1") {
        return true;
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if PathBuf::from(profile)
            .join("CascadeProjects")
            .join("ResonanceStudio")
            .join(".no-gpu")
            .is_file()
        {
            return true;
        }
    }
    false
}

pub fn parse_help_caps(help_text: &str, is_cuda_backend: bool) -> ServerCliCaps {
    ServerCliCaps {
        has_no_ui: help_text.contains("--no-ui"),
        has_no_webui: help_text.contains("--no-webui"),
        has_n_gpu_layers: help_text.contains("--n-gpu-layers"),
        has_no_repack: help_text.contains("--no-repack"),
        has_flash_attn: help_text.contains("--flash-attn"),
        has_cache_type: help_text.contains("--cache-type-k")
            && help_text.contains("--cache-type-v"),
        has_cache_reuse: help_text.contains("--cache-reuse"),
        has_slot_similarity: help_text.contains("--slot-prompt-similarity"),
        has_spec_type: help_text.contains("--spec-type"),
        has_sleep_idle: help_text.contains("--sleep-idle-seconds"),
        has_split_mode: help_text.contains("--split-mode"),
        has_main_gpu: help_text.contains("--main-gpu"),
        has_fit: help_text.contains("--fit") && help_text.contains("--fit-ctx"),
        is_cuda_backend,
    }
}

pub fn inspect_server_cli_caps(exe: &Path) -> ServerCliCaps {
    let cache = CLI_CAPS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(exe) {
            return cached.clone();
        }
    }

    let is_cuda_backend = exe
        .parent()
        .is_some_and(|p| p.join("ggml-cuda.dll").is_file());
    let mut cmd = Command::new(exe);
    cmd.arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let caps = match cmd.output() {
        Ok(out) => {
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            combined.push('\n');
            combined.push_str(&String::from_utf8_lossy(&out.stderr));
            parse_help_caps(&combined, is_cuda_backend)
        }
        Err(_) => ServerCliCaps {
            has_n_gpu_layers: true,
            is_cuda_backend,
            ..Default::default()
        },
    };

    if let Ok(mut guard) = cache.lock() {
        guard.insert(exe.to_path_buf(), caps.clone());
    }
    caps
}

pub fn has_builtin_engine(data_dir: &Path) -> bool {
    let cfg = load_config(data_dir);
    let has_nv = detect_nvidia_gpu().is_some();
    resolve_active_llama_server_exe(data_dir, &cfg, has_nv).is_some()
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

pub fn parse_nvidia_smi_line(line: &str) -> Option<NvidiaGpuTelemetry> {
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next()?.to_string();
    if name.is_empty() {
        return None;
    }
    let driver_raw = parts.next().unwrap_or("");
    let driver_major = driver_raw
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let free_mib = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let total_mib = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let util_pct = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0) as u32;
    Some(NvidiaGpuTelemetry {
        name,
        driver_major,
        free_mib,
        total_mib,
        util_pct,
    })
}

pub fn detect_nvidia_gpu() -> Option<NvidiaGpuTelemetry> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,driver_version,memory.free,memory.total,utilization.gpu",
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
    parse_nvidia_smi_line(&line)
}

fn format_nvidia_gpu_summary(nv: &NvidiaGpuTelemetry) -> String {
    if nv.total_mib > 0 {
        format!(
            "{} ({:.1} / {:.1} GB VRAM free · {}% util)",
            nv.name,
            nv.free_mib as f64 / 1024.0,
            nv.total_mib as f64 / 1024.0,
            nv.util_pct
        )
    } else {
        nv.name.clone()
    }
}

pub fn inspect_hardware_for_data_dir(data_dir: &Path, target_dir: &Path) -> HardwareProbe {
    let disk_free_bytes = disk_free_bytes_for_path(target_dir);
    let (ram_total_bytes, ram_avail_bytes) = system_memory_bytes();
    let nv = detect_nvidia_gpu();
    let gpu_summary = nv.as_ref().map(format_nvidia_gpu_summary);
    let cfg = load_config(data_dir);
    let engine_backend = resolve_active_llama_server_exe(data_dir, &cfg, nv.is_some())
        .map(|(_, lbl)| lbl.to_string())
        .unwrap_or_else(|| {
            if nv.is_some() {
                "CUDA / Vulkan (Not Installed)".into()
            } else {
                "Vulkan / CPU (Not Installed)".into()
            }
        });
    HardwareProbe {
        disk_free_bytes,
        ram_total_bytes,
        ram_avail_bytes,
        gpu_summary,
        engine_backend,
        active_tier: get_active_tier_label(),
        gpu_hold: gpu_hold_active(),
    }
}

pub fn inspect_hardware(target_dir: &Path) -> HardwareProbe {
    inspect_hardware_for_data_dir(target_dir, target_dir)
}

pub fn kill_managed_server(slot: &Mutex<Option<Child>>) {
    set_active_tier_label(None);
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

/// Resolve the best official Windows `llama.cpp` release zip(s):
/// on NVIDIA GPUs, prefers CUDA 13 (driver >= 570) or CUDA 12.4 (`cudart` + `llama-bin`),
/// falling back cleanly to Vulkan x64 and CPU x64.
async fn resolve_llama_cpp_win_assets(
    client: &reqwest::Client,
    nvidia: Option<&NvidiaGpuTelemetry>,
) -> Result<Vec<(String, String)>, String> {
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

    let prefer_cuda13 = nvidia.is_some_and(|n| n.driver_major == 0 || n.driver_major >= 570);
    let mut vulkan_fallback: Option<(String, String)> = None;
    let mut cpu_fallback: Option<(String, String)> = None;

    for rel in arr {
        let Some(assets) = rel["assets"].as_array() else {
            continue;
        };
        let mut cuda13_bin = None;
        let mut cuda13_rt = None;
        let mut cuda12_bin = None;
        let mut cuda12_rt = None;

        for asset in assets {
            let name = asset["name"].as_str().unwrap_or("");
            let url = asset["browser_download_url"].as_str().unwrap_or("");
            if name.is_empty() || url.is_empty() {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if nvidia.is_some() {
                if lower.starts_with("llama-")
                    && lower.contains("-bin-win-cuda-13.")
                    && lower.ends_with("-x64.zip")
                {
                    cuda13_bin = Some((name.to_string(), url.to_string()));
                } else if lower.starts_with("cudart-llama-bin-win-cuda-13.")
                    && lower.ends_with("-x64.zip")
                {
                    cuda13_rt = Some((name.to_string(), url.to_string()));
                } else if lower.starts_with("llama-")
                    && lower.contains("-bin-win-cuda-12.")
                    && lower.ends_with("-x64.zip")
                {
                    cuda12_bin = Some((name.to_string(), url.to_string()));
                } else if lower.starts_with("cudart-llama-bin-win-cuda-12.")
                    && lower.ends_with("-x64.zip")
                {
                    cuda12_rt = Some((name.to_string(), url.to_string()));
                }
            }
            if vulkan_fallback.is_none()
                && lower.starts_with("llama-")
                && lower.ends_with("-bin-win-vulkan-x64.zip")
            {
                vulkan_fallback = Some((name.to_string(), url.to_string()));
            }
            if cpu_fallback.is_none()
                && lower.starts_with("llama-")
                && lower.ends_with("-bin-win-cpu-x64.zip")
            {
                cpu_fallback = Some((name.to_string(), url.to_string()));
            }
        }

        if prefer_cuda13 {
            if let (Some(rt), Some(bin)) = (cuda13_rt, cuda13_bin) {
                return Ok(vec![rt, bin]);
            }
        }
        if nvidia.is_some() {
            if let (Some(rt), Some(bin)) = (cuda12_rt, cuda12_bin) {
                return Ok(vec![rt, bin]);
            }
        }
    }

    if let Some(vk) = vulkan_fallback {
        return Ok(vec![vk]);
    }
    cpu_fallback.map(|cpu| vec![cpu]).ok_or_else(|| {
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

    // Step 1: Ensure portable llama.cpp GPU engine is present (reusing local CUDA if available)
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
        let nv = detect_nvidia_gpu();
        let assets = resolve_llama_cpp_win_assets(&client, nv.as_ref()).await?;
        let rdir = runtime_dir(data_dir);
        let total_zips = assets.len();
        for (idx, (asset_name, zip_url)) in assets.into_iter().enumerate() {
            let zip_dest = rdir.join(&asset_name);
            download_streaming_file(
                &client,
                &zip_url,
                &zip_dest,
                &format!(
                    "Step 1/3: Downloading GPU engine ({}/{}: {asset_name})…",
                    idx + 1,
                    total_zips
                ),
                false,
                app,
                cancel_flag,
            )
            .await?;

            emit_progress(
                app,
                format!(
                    "Step 1/3: Extracting ({}/{}) {asset_name}…",
                    idx + 1,
                    total_zips
                ),
                0,
                0,
                99.0,
                false,
                None,
            );
            extract_zip_to_dir(&zip_dest, &rdir)?;
            let _ = fs::remove_file(&zip_dest);
        }

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

fn optimal_cpu_threads() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    (cores / 2).clamp(4, 16)
}

/// ComfyUI-style dynamic VRAM tiered execution planner: calculates safe GPU layer counts from
/// live `nvidia-smi` headroom + model file size and builds a multi-stage fallback ladder.
pub fn plan_vram_execution_ladder(
    mode: &str,
    model_bytes: u64,
    nvidia: Option<&NvidiaGpuTelemetry>,
    caps: &ServerCliCaps,
    gpu_hold: bool,
) -> Vec<LaunchAttempt> {
    if mode == "cpu" || (mode == "auto" && gpu_hold) {
        return vec![LaunchAttempt {
            tier: VramTier::CpuOnly,
            label: if gpu_hold && mode == "auto" {
                "CPU + System RAM (GPU Hold Active)".into()
            } else {
                "CPU + System RAM (Zero-Copy)".into()
            },
        }];
    }

    let model_mib = (model_bytes / (1024 * 1024)).max(1024);
    let per_layer_mib = (model_mib / 62).clamp(180, 340);
    const BASE_OVERHEAD_MIB: u64 = 700;
    const SAFETY_RESERVE_MIB: u64 = 1600;

    if let Some(nv) = nvidia {
        let free = nv.free_mib;
        let full_threshold = model_mib + SAFETY_RESERVE_MIB + 400;
        let usable = free.saturating_sub(SAFETY_RESERVE_MIB);
        let safe_layers =
            ((usable.saturating_sub(BASE_OVERHEAD_MIB)) / per_layer_mib).clamp(4, 58) as u32;

        if mode == "gpu" {
            let mut ladder = vec![LaunchAttempt {
                tier: VramTier::FullGpu,
                label: "Full GPU (99L, FlashAttn, Q8 KV)".into(),
            }];
            if free >= 4_500 && free < full_threshold {
                ladder.push(LaunchAttempt {
                    tier: VramTier::HybridGpu {
                        layers: safe_layers,
                        kv_type: "q4_0",
                        batch_size: 1024,
                        ubatch_size: 256,
                    },
                    label: format!("Hybrid GPU+RAM ({safe_layers}L GPU, Q4 KV)"),
                });
            }
            return ladder;
        }

        // Auto mode with NVIDIA telemetry
        if free >= full_threshold {
            return vec![
                LaunchAttempt {
                    tier: VramTier::FullGpu,
                    label: "Full GPU (99L, FlashAttn, Q8 KV)".into(),
                },
                LaunchAttempt {
                    tier: VramTier::HybridGpu {
                        layers: 32,
                        kv_type: "q4_0",
                        batch_size: 1024,
                        ubatch_size: 256,
                    },
                    label: "Hybrid GPU+RAM Fallback (32L GPU, Q4 KV)".into(),
                },
                LaunchAttempt {
                    tier: VramTier::CpuOnly,
                    label: "CPU + System RAM Fallback".into(),
                },
            ];
        } else if free >= 4_500 {
            let half_layers = (safe_layers / 2).max(4);
            return vec![
                LaunchAttempt {
                    tier: VramTier::HybridGpu {
                        layers: safe_layers,
                        kv_type: "q4_0",
                        batch_size: 1024,
                        ubatch_size: 256,
                    },
                    label: format!("Hybrid GPU+RAM ({safe_layers}L GPU, Q4 KV)"),
                },
                LaunchAttempt {
                    tier: VramTier::HybridGpu {
                        layers: half_layers,
                        kv_type: "q4_0",
                        batch_size: 512,
                        ubatch_size: 128,
                    },
                    label: format!("Conservative Hybrid ({half_layers}L GPU, Q4 KV)"),
                },
                LaunchAttempt {
                    tier: VramTier::CpuOnly,
                    label: "CPU + System RAM Fallback".into(),
                },
            ];
        } else {
            return vec![LaunchAttempt {
                tier: VramTier::CpuOnly,
                label: format!(
                    "CPU + System RAM ({:.1} GB VRAM free)",
                    free as f64 / 1024.0
                ),
            }];
        }
    }

    // No NVIDIA GPU detected (AMD/Intel Vulkan or CPU-only machine)
    if mode == "gpu" {
        return vec![LaunchAttempt {
            tier: VramTier::FullGpu,
            label: "Full GPU (99L)".into(),
        }];
    }
    if caps.has_fit {
        vec![
            LaunchAttempt {
                tier: VramTier::VulkanAutoFit,
                label: "Vulkan Auto-Fit GPU + RAM".into(),
            },
            LaunchAttempt {
                tier: VramTier::CpuOnly,
                label: "CPU + System RAM Fallback".into(),
            },
        ]
    } else {
        vec![LaunchAttempt {
            tier: VramTier::CpuOnly,
            label: "CPU + System RAM".into(),
        }]
    }
}

/// Constructs verified CLI arguments from inspected `ServerCliCaps` and chosen `LaunchAttempt`.
/// Guarantees canonical `--n-gpu-layers` (never invalid `--ngl`) and isolates discrete GPUs.
pub fn build_llama_server_args(
    model: &Path,
    caps: &ServerCliCaps,
    attempt: &LaunchAttempt,
    idle_sleep_secs: u32,
    has_nvidia: bool,
) -> Vec<String> {
    let mut args = vec![
        "--model".to_string(),
        model.to_string_lossy().into_owned(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        BUILTIN_PORT.to_string(),
        "--alias".to_string(),
        cosmic_foundry::vecglypher::VECGLYPHER_RECOMMENDED_GGUF.to_string(),
        "--ctx-size".to_string(),
        "2048".to_string(),
        "--parallel".to_string(),
        "1".to_string(),
    ];

    if caps.has_no_ui {
        args.push("--no-ui".into());
    } else if caps.has_no_webui {
        args.push("--no-webui".into());
    }

    if idle_sleep_secs > 0 && caps.has_sleep_idle {
        args.push("--sleep-idle-seconds".into());
        args.push(idle_sleep_secs.to_string());
    }

    if caps.has_cache_reuse {
        args.push("--cache-reuse".into());
        args.push("64".into());
    }
    if caps.has_slot_similarity {
        args.push("--slot-prompt-similarity".into());
        args.push("0.50".into());
    }

    let threads = optimal_cpu_threads();

    match &attempt.tier {
        VramTier::FullGpu => {
            if caps.has_n_gpu_layers {
                args.push("--n-gpu-layers".into());
                args.push("99".into());
            }
            if !caps.is_cuda_backend && has_nvidia {
                args.push("--device".into());
                args.push("Vulkan0".into());
            }
            if caps.has_split_mode {
                args.push("--split-mode".into());
                args.push("none".into());
            }
            if caps.has_main_gpu {
                args.push("--main-gpu".into());
                args.push("0".into());
            }
            if caps.has_flash_attn {
                args.push("--flash-attn".into());
                args.push(if caps.is_cuda_backend { "on" } else { "auto" }.into());
            }
            if caps.has_cache_type {
                args.extend([
                    "--cache-type-k".into(),
                    "q8_0".into(),
                    "--cache-type-v".into(),
                    "q8_0".into(),
                ]);
            }
            args.extend([
                "--batch-size".into(),
                "2048".into(),
                "--ubatch-size".into(),
                "512".into(),
            ]);
            if caps.has_spec_type {
                args.push("--spec-type".into());
                args.push("ngram-simple".into());
            }
        }
        VramTier::HybridGpu {
            layers,
            kv_type,
            batch_size,
            ubatch_size,
        } => {
            if caps.has_n_gpu_layers {
                args.push("--n-gpu-layers".into());
                args.push(layers.to_string());
            }
            if caps.has_no_repack {
                args.push("--no-repack".into());
            }
            if !caps.is_cuda_backend && has_nvidia {
                args.push("--device".into());
                args.push("Vulkan0".into());
            }
            if caps.has_split_mode {
                args.push("--split-mode".into());
                args.push("none".into());
            }
            if caps.has_main_gpu {
                args.push("--main-gpu".into());
                args.push("0".into());
            }
            if caps.has_flash_attn {
                args.push("--flash-attn".into());
                args.push(if caps.is_cuda_backend { "on" } else { "auto" }.into());
            }
            if caps.has_cache_type {
                args.extend([
                    "--cache-type-k".into(),
                    (*kv_type).into(),
                    "--cache-type-v".into(),
                    (*kv_type).into(),
                ]);
            }
            args.extend([
                "--batch-size".into(),
                batch_size.to_string(),
                "--ubatch-size".into(),
                ubatch_size.to_string(),
                "--threads".into(),
                threads.to_string(),
                "--threads-batch".into(),
                threads.to_string(),
            ]);
        }
        VramTier::VulkanAutoFit => {
            if caps.has_no_repack {
                args.push("--no-repack".into());
            }
            if !caps.is_cuda_backend && has_nvidia {
                args.push("--device".into());
                args.push("Vulkan0".into());
            }
            if caps.has_split_mode {
                args.push("--split-mode".into());
                args.push("none".into());
            }
            if caps.has_main_gpu {
                args.push("--main-gpu".into());
                args.push("0".into());
            }
            if caps.has_flash_attn {
                args.push("--flash-attn".into());
                args.push("auto".into());
            }
            if caps.has_cache_type {
                args.extend([
                    "--cache-type-k".into(),
                    "q8_0".into(),
                    "--cache-type-v".into(),
                    "q8_0".into(),
                ]);
            }
            if caps.has_fit {
                args.extend([
                    "--fit".into(),
                    "on".into(),
                    "--fit-ctx".into(),
                    "2048".into(),
                    "--fit-target".into(),
                    "1536".into(),
                ]);
            }
        }
        VramTier::CpuOnly => {
            args.push("--device".into());
            args.push("none".into());
            if caps.has_n_gpu_layers {
                args.push("--n-gpu-layers".into());
                args.push("0".into());
            }
            if caps.has_no_repack {
                args.push("--no-repack".into());
            }
            if caps.has_flash_attn {
                args.push("--flash-attn".into());
                args.push("auto".into());
            }
            if caps.has_cache_type {
                args.extend([
                    "--cache-type-k".into(),
                    "q8_0".into(),
                    "--cache-type-v".into(),
                    "q8_0".into(),
                ]);
            }
            args.extend([
                "--threads".into(),
                threads.to_string(),
                "--threads-batch".into(),
                threads.to_string(),
                "--batch-size".into(),
                "512".into(),
                "--ubatch-size".into(),
                "128".into(),
            ]);
        }
    }

    args
}

fn spawn_llama_server_attempt(
    data_dir: &Path,
    exe: &Path,
    args: &[String],
) -> Result<Child, String> {
    let rdir = runtime_dir(data_dir);
    let _ = fs::create_dir_all(&rdir);
    let exe_dir = exe.parent().unwrap_or(&rdir);
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
    cmd.current_dir(exe_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to start llama-server.exe ({}): {e}", exe.display()))
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

/// Start `llama-server.exe` using dynamic VRAM orchestration and multi-stage fallback.
pub async fn ensure_builtin_server_running(
    data_dir: &Path,
    llama_child: &Mutex<Option<Child>>,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    if builtin_server_healthy().await {
        return Ok(());
    }

    let cfg = load_config(data_dir);
    let nvidia = detect_nvidia_gpu();
    let Some((exe, backend_label)) =
        resolve_active_llama_server_exe(data_dir, &cfg, nvidia.is_some())
    else {
        return Err("Built-in llama-server.exe is not installed yet.".into());
    };
    let model = effective_gguf_path(data_dir, &cfg);
    if !has_builtin_model(data_dir) {
        return Err("VecGlypher-27b-it GGUF model is not downloaded or valid yet.".into());
    }

    let model_bytes = fs::metadata(&model).map(|m| m.len()).unwrap_or(0);
    let caps = inspect_server_cli_caps(&exe);
    let hold = gpu_hold_active();
    let ladder = plan_vram_execution_ladder(
        cfg.compute_mode.as_str(),
        model_bytes,
        nvidia.as_ref(),
        &caps,
        hold,
    );

    let log_path = llama_server_log_path(data_dir);
    let mut attempt_errors: Vec<String> = Vec::new();

    for (idx, attempt) in ladder.iter().enumerate() {
        let stage_msg = format!(
            "Step 3/3: Starting {backend_label} — {} ({}/{})…",
            attempt.label,
            idx + 1,
            ladder.len()
        );
        emit_progress(app, stage_msg, 0, 0, 98.0, false, None);

        kill_managed_server(llama_child);
        let args = build_llama_server_args(
            &model,
            &caps,
            attempt,
            cfg.idle_sleep_secs,
            nvidia.is_some(),
        );

        let child = spawn_llama_server_attempt(data_dir, &exe, &args)?;
        if let Ok(mut guard) = llama_child.lock() {
            *guard = Some(child);
        }

        match wait_for_server_ready(llama_child).await {
            Ok(()) => {
                set_active_tier_label(Some(format!("{backend_label} · {}", attempt.label)));
                return Ok(());
            }
            Err(exit_opt) => {
                let tail = read_log_tail(&log_path, 6);
                let reason = exit_opt
                    .map(|s| format!("exited ({s})"))
                    .unwrap_or_else(|| "timed out".into());
                let detail = if tail.is_empty() {
                    format!("[{}: {reason}]", attempt.label)
                } else {
                    format!("[{}: {reason}] {tail}", attempt.label)
                };
                attempt_errors.push(detail);
                kill_managed_server(llama_child);
            }
        }
    }

    Err(format!(
        "llama-server.exe ({backend_label}) failed across all {} orchestration tier(s): {}",
        attempt_errors.len(),
        attempt_errors.join(" || ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_caps(is_cuda: bool) -> ServerCliCaps {
        ServerCliCaps {
            has_no_ui: true,
            has_no_webui: true,
            has_n_gpu_layers: true,
            has_no_repack: true,
            has_flash_attn: true,
            has_cache_type: true,
            has_cache_reuse: true,
            has_slot_similarity: true,
            has_spec_type: true,
            has_sleep_idle: true,
            has_split_mode: true,
            has_main_gpu: true,
            has_fit: true,
            is_cuda_backend: is_cuda,
        }
    }

    #[test]
    fn build_args_never_emits_invalid_ngl_or_deprecated_webui() {
        let caps = sample_caps(true);
        let model = Path::new("vecglypher-27b-it-q4_k_m.gguf");
        for attempt in [
            LaunchAttempt {
                tier: VramTier::FullGpu,
                label: "Full".into(),
            },
            LaunchAttempt {
                tier: VramTier::HybridGpu {
                    layers: 12,
                    kv_type: "q4_0",
                    batch_size: 1024,
                    ubatch_size: 256,
                },
                label: "Hybrid".into(),
            },
            LaunchAttempt {
                tier: VramTier::VulkanAutoFit,
                label: "AutoFit".into(),
            },
            LaunchAttempt {
                tier: VramTier::CpuOnly,
                label: "CPU".into(),
            },
        ] {
            let args = build_llama_server_args(model, &caps, &attempt, 900, true);
            assert!(
                !args.iter().any(|a| a == "--ngl"),
                "Invalid double-dash --ngl must never be emitted (got {:?})",
                args
            );
            assert!(
                !args.iter().any(|a| a == "--no-webui"),
                "Prefer --no-ui over deprecated --no-webui (got {:?})",
                args
            );
            assert!(args.iter().any(|a| a == "--no-ui"));
        }
        let cpu_args = build_llama_server_args(
            model,
            &caps,
            &LaunchAttempt {
                tier: VramTier::CpuOnly,
                label: "CPU".into(),
            },
            900,
            true,
        );
        let ngl_pos = cpu_args.iter().position(|a| a == "--n-gpu-layers").unwrap();
        assert_eq!(cpu_args[ngl_pos + 1], "0");
        assert!(cpu_args.iter().any(|a| a == "--no-repack"));
    }

    #[test]
    fn vram_ladder_selects_full_hybrid_or_cpu_by_headroom_and_gpu_hold() {
        let caps = sample_caps(true);
        let model_bytes = 16_546_600_000u64;

        let high_vram = NvidiaGpuTelemetry {
            name: "RTX 5090".into(),
            driver_major: 621,
            free_mib: 26_000,
            total_mib: 32_607,
            util_pct: 2,
        };
        let high_ladder =
            plan_vram_execution_ladder("auto", model_bytes, Some(&high_vram), &caps, false);
        assert_eq!(high_ladder.len(), 3);
        assert_eq!(high_ladder[0].tier, VramTier::FullGpu);
        assert_eq!(high_ladder[2].tier, VramTier::CpuOnly);

        let shared_vram = NvidiaGpuTelemetry {
            name: "RTX 5090".into(),
            driver_major: 621,
            free_mib: 5_124,
            total_mib: 32_607,
            util_pct: 10,
        };
        let shared_ladder =
            plan_vram_execution_ladder("auto", model_bytes, Some(&shared_vram), &caps, false);
        assert!(matches!(shared_ladder[0].tier, VramTier::HybridGpu { .. }));
        assert_eq!(shared_ladder.last().unwrap().tier, VramTier::CpuOnly);

        let low_vram = NvidiaGpuTelemetry {
            name: "RTX 5090".into(),
            driver_major: 621,
            free_mib: 1_800,
            total_mib: 32_607,
            util_pct: 95,
        };
        let low_ladder =
            plan_vram_execution_ladder("auto", model_bytes, Some(&low_vram), &caps, false);
        assert_eq!(low_ladder.len(), 1);
        assert_eq!(low_ladder[0].tier, VramTier::CpuOnly);

        let hold_ladder =
            plan_vram_execution_ladder("auto", model_bytes, Some(&high_vram), &caps, true);
        assert_eq!(hold_ladder.len(), 1);
        assert_eq!(hold_ladder[0].tier, VramTier::CpuOnly);
    }

    #[test]
    fn vulkan_discrete_gpu_isolation_excludes_igpu() {
        let vulkan_caps = sample_caps(false);
        let args = build_llama_server_args(
            Path::new("m.gguf"),
            &vulkan_caps,
            &LaunchAttempt {
                tier: VramTier::FullGpu,
                label: "Full".into(),
            },
            900,
            true,
        );
        let dev_pos = args.iter().position(|a| a == "--device").unwrap();
        assert_eq!(args[dev_pos + 1], "Vulkan0");
        assert!(args.windows(2).any(|w| w == ["--split-mode", "none"]));
        assert!(args.windows(2).any(|w| w == ["--main-gpu", "0"]));
    }
}
