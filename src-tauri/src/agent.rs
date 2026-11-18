//! Ultra-lightweight, ML-free local agentic control plane & MCP JSON-RPC 2.0 server.
//! Exposes read/write font management on loopback (`127.0.0.1`) and optional BYO-AI orchestration.

use crate::{run_smart_scan, toggle_families_sync, AppState};
use cosmic_core::inventory::{summarize_diagnostics, FontFamily, Origin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

const AGENT_CONFIG_FILE: &str = "agent-config.json";
const MAX_LOGS: usize = 64;
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
const MAX_CONCURRENT_CONNS: usize = 16;
const PORT_RANGE: std::ops::RangeInclusive<u16> = 17420..=17430;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AgentConfig {
    pub server_enabled: bool,
    pub allow_mutations: bool,
    /// `"builtin"` (0 MB deterministic), `"ollama"`, or `"openai"`.
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_enabled: true,
            allow_mutations: true,
            provider: "builtin".into(),
            endpoint: "http://127.0.0.1:11434".into(),
            model: "llama3.2".into(),
            api_key: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub source: String,
    pub action: String,
    pub detail: String,
    pub ok: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentSnapshot {
    pub config: AgentConfig,
    pub port: Option<u16>,
    pub listening: bool,
    pub mcp_url: String,
    pub api_base: String,
    pub logs: Vec<AgentLogEntry>,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentCommandResult {
    pub ok: bool,
    pub provider: String,
    pub model: String,
    pub reply: String,
    pub executed_actions: Vec<Value>,
    pub snapshot: AgentSnapshot,
}

pub struct AgentControl {
    config: Mutex<AgentConfig>,
    config_path: PathBuf,
    port: Mutex<Option<u16>>,
    logs: Mutex<VecDeque<AgentLogEntry>>,
    next_log_id: Mutex<u64>,
    active_conns: Arc<AtomicUsize>,
}

impl AgentControl {
    fn load(data_dir: PathBuf) -> Self {
        let config_path = data_dir.join(AGENT_CONFIG_FILE);
        let config: AgentConfig = fs::read(&config_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut logs = VecDeque::with_capacity(MAX_LOGS);
        logs.push_front(AgentLogEntry {
            id: 1,
            timestamp: utc_clock_now(),
            source: "system".into(),
            action: "agent:boot".into(),
            detail: "Local MCP & REST control plane initialized (0 MB deterministic core)".into(),
            ok: true,
        });
        Self {
            config: Mutex::new(config),
            config_path,
            port: Mutex::new(None),
            logs: Mutex::new(logs),
            next_log_id: Mutex::new(2),
            active_conns: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn save_config(&self, cfg: &AgentConfig) {
        if let Some(parent) = self.config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(cfg) {
            let _ = fs::write(&self.config_path, bytes);
        }
    }

    pub(crate) fn record_log(
        &self,
        app: Option<&AppHandle>,
        source: &str,
        action: &str,
        detail: impl Into<String>,
        ok: bool,
    ) {
        let id = {
            let mut n = self.next_log_id.lock().unwrap();
            let cur = *n;
            *n += 1;
            cur
        };
        let entry = AgentLogEntry {
            id,
            timestamp: utc_clock_now(),
            source: source.to_string(),
            action: action.to_string(),
            detail: detail.into(),
            ok,
        };
        {
            let mut logs = self.logs.lock().unwrap();
            logs.push_front(entry);
            while logs.len() > MAX_LOGS {
                logs.pop_back();
            }
        }
        if let Some(app) = app {
            let _ = app.emit("agent-updated", self.snapshot());
        }
    }

    pub(crate) fn snapshot(&self) -> AgentSnapshot {
        let config = self.config.lock().unwrap().clone();
        let port = *self.port.lock().unwrap();
        let logs = self.logs.lock().unwrap().iter().cloned().collect();
        let listening = port.is_some() && config.server_enabled;
        let base_port = port.unwrap_or(*PORT_RANGE.start());
        let api_base = format!("http://127.0.0.1:{base_port}");
        let mcp_url = format!("{api_base}/mcp");
        AgentSnapshot {
            config,
            port,
            listening,
            mcp_url,
            api_base,
            logs,
        }
    }
}

fn utc_clock_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let tod = secs % 86_400;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    let s = tod % 60;
    format!("{h:02}:{m:02}:{s:02}Z")
}

/// Binds the loopback HTTP/MCP listener on `127.0.0.1:17420..=17430` and registers `AgentControl`.
pub fn init(app: &AppHandle) {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("cosmic-font-foundry"));
    let ctrl = Arc::new(AgentControl::load(data_dir));

    let mut bound_listener: Option<(TcpListener, u16)> = None;
    for port in PORT_RANGE {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            bound_listener = Some((listener, port));
            break;
        }
    }

    if let Some((listener, port)) = bound_listener {
        *ctrl.port.lock().unwrap() = Some(port);
        ctrl.record_log(
            None,
            "system",
            "server:listen",
            format!("Listening on http://127.0.0.1:{port} (MCP + REST)"),
            true,
        );
        let app_handle = app.clone();
        let ctrl_clone = Arc::clone(&ctrl);
        let _ = std::thread::Builder::new()
            .name("cosmic-agent-http".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if ctrl_clone.active_conns.load(Ordering::Relaxed) >= MAX_CONCURRENT_CONNS {
                        continue;
                    }
                    ctrl_clone.active_conns.fetch_add(1, Ordering::Relaxed);
                    let app_h = app_handle.clone();
                    let ctrl_h = Arc::clone(&ctrl_clone);
                    std::thread::spawn(move || {
                        handle_http_connection(stream, &app_h, &ctrl_h);
                        ctrl_h.active_conns.fetch_sub(1, Ordering::Relaxed);
                    });
                }
            });
    } else {
        ctrl.record_log(
            None,
            "system",
            "server:bind_failed",
            "All ports 17420-17430 unavailable; IPC & Agent Director still active",
            false,
        );
    }

    app.manage(ctrl);
}

#[tauri::command]
pub fn agent_get_state(ctrl: tauri::State<Arc<AgentControl>>) -> AgentSnapshot {
    ctrl.snapshot()
}

#[tauri::command]
pub fn agent_set_config(
    app: AppHandle,
    ctrl: tauri::State<Arc<AgentControl>>,
    config: AgentConfig,
) -> AgentSnapshot {
    let sanitized = AgentConfig {
        server_enabled: config.server_enabled,
        allow_mutations: config.allow_mutations,
        provider: match config.provider.as_str() {
            "ollama" | "openai" => config.provider,
            _ => "builtin".into(),
        },
        endpoint: config.endpoint.trim().to_string(),
        model: config.model.trim().to_string(),
        api_key: config.api_key.trim().to_string(),
    };
    {
        let mut cur = ctrl.config.lock().unwrap();
        *cur = sanitized.clone();
    }
    ctrl.save_config(&sanitized);
    ctrl.record_log(
        Some(&app),
        "system",
        "config:saved",
        format!(
            "server={}, mutations={}, provider={}",
            sanitized.server_enabled, sanitized.allow_mutations, sanitized.provider
        ),
        true,
    );
    ctrl.snapshot()
}

#[tauri::command]
pub async fn agent_run_command(
    app: AppHandle,
    ctrl: tauri::State<'_, Arc<AgentControl>>,
    prompt: String,
) -> Result<AgentCommandResult, String> {
    let trimmed = prompt.trim().to_string();
    if trimmed.is_empty() {
        return Err("Enter a command or select a quick-action workflow.".into());
    }
    let cfg = ctrl.config.lock().unwrap().clone();
    let ctrl_arc = Arc::clone(ctrl.inner());

    match cfg.provider.as_str() {
        "ollama" | "openai" => run_external_ai_prompt(&app, &ctrl_arc, &cfg, &trimmed).await,
        _ => {
            let app_c = app.clone();
            let ctrl_c = Arc::clone(&ctrl_arc);
            let prompt_c = trimmed.clone();
            tauri::async_runtime::spawn_blocking(move || {
                Ok(run_deterministic_prompt(&app_c, &ctrl_c, &prompt_c))
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, &'static str> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let mut raw = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).map_err(|_| "read timeout")?;
        if n == 0 {
            return Err("empty request");
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if raw.len() > MAX_HTTP_HEADER_BYTES {
            return Err("headers too large");
        }
    }

    let head = String::from_utf8_lossy(&raw[..header_end - 4]);
    let mut lines = head.lines();
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_ascii_uppercase();
    let target = parts.next().ok_or("missing target")?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };

    let mut content_length: usize = 0;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_HTTP_BODY_BYTES {
        return Err("body too large");
    }

    let mut body = raw[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).map_err(|_| "incomplete body")?;
        if n == 0 {
            break;
        }
        let need = content_length - body.len();
        body.extend_from_slice(&chunk[..n.min(need)]);
    }
    body.truncate(content_length);

    Ok(HttpRequest {
        method,
        path,
        query,
        body,
    })
}

fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    if !body.is_empty() {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
}

fn write_json(stream: &mut TcpStream, status: &str, payload: &Value) {
    let bytes = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    write_http_response(stream, status, "application/json; charset=utf-8", &bytes);
}

fn handle_http_connection(mut stream: TcpStream, app: &AppHandle, ctrl: &Arc<AgentControl>) {
    let req = match read_http_request(&mut stream) {
        Ok(r) => r,
        Err(reason) => {
            write_json(&mut stream, "400 Bad Request", &json!({ "error": reason }));
            return;
        }
    };

    if req.method == "OPTIONS" {
        write_http_response(&mut stream, "204 No Content", "text/plain", b"");
        return;
    }

    let cfg = ctrl.config.lock().unwrap().clone();
    if !cfg.server_enabled {
        write_json(
            &mut stream,
            "503 Service Unavailable",
            &json!({ "error": "Agent control plane is paused in Cosmic Font Foundry." }),
        );
        return;
    }

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/status") => {
            let status = build_status_value(app, ctrl);
            ctrl.record_log(Some(app), "http", "GET /api/status", "Status queried", true);
            write_json(&mut stream, "200 OK", &status);
        }
        ("GET", "/api/fonts") => {
            let params = parse_query_params(&req.query);
            let query = params.get("q").map(String::as_str);
            let category = params.get("category").map(String::as_str);
            let origin = params.get("origin").map(String::as_str);
            let active = params.get("active").and_then(|v| match v.as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            });
            let limit = params
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(80)
                .clamp(1, 500);
            let res = search_fonts_value(app, query, category, active, origin, limit);
            ctrl.record_log(
                Some(app),
                "http",
                "GET /api/fonts",
                format!(
                    "q={:?}, matched={}",
                    query.unwrap_or(""),
                    res["total_matches"].as_u64().unwrap_or(0)
                ),
                true,
            );
            write_json(&mut stream, "200 OK", &res);
        }
        ("POST", "/api/toggle") => {
            if !cfg.allow_mutations {
                ctrl.record_log(
                    Some(app),
                    "http",
                    "POST /api/toggle",
                    "Blocked: Read-Only monitoring mode active",
                    false,
                );
                write_json(
                    &mut stream,
                    "403 Forbidden",
                    &json!({ "error": "Read-Only monitoring mode active (enable Write Control in Agent Hub)." }),
                );
                return;
            }
            let Ok(val) = serde_json::from_slice::<Value>(&req.body) else {
                write_json(
                    &mut stream,
                    "400 Bad Request",
                    &json!({ "error": "Invalid JSON body" }),
                );
                return;
            };
            match execute_tool(app, ctrl, "http", "cosmic_toggle_fonts", &val, true) {
                Ok(out) => write_json(&mut stream, "200 OK", &out),
                Err(msg) => write_json(&mut stream, "400 Bad Request", &json!({ "error": msg })),
            }
        }
        ("POST", "/api/navigate") => {
            if !cfg.allow_mutations {
                ctrl.record_log(
                    Some(app),
                    "http",
                    "POST /api/navigate",
                    "Blocked: Read-Only monitoring mode active",
                    false,
                );
                write_json(
                    &mut stream,
                    "403 Forbidden",
                    &json!({ "error": "Read-Only monitoring mode active." }),
                );
                return;
            }
            let Ok(val) = serde_json::from_slice::<Value>(&req.body) else {
                write_json(
                    &mut stream,
                    "400 Bad Request",
                    &json!({ "error": "Invalid JSON body" }),
                );
                return;
            };
            match execute_tool(app, ctrl, "http", "cosmic_ui_navigate", &val, true) {
                Ok(out) => write_json(&mut stream, "200 OK", &out),
                Err(msg) => write_json(&mut stream, "400 Bad Request", &json!({ "error": msg })),
            }
        }
        ("GET", "/mcp") => {
            let snap = ctrl.snapshot();
            write_json(
                &mut stream,
                "200 OK",
                &json!({
                    "name": "cosmic-font-foundry",
                    "transport": "streamable-http",
                    "protocolVersion": "2024-11-05",
                    "endpoint": snap.mcp_url,
                    "tools": mcp_tools_list().len(),
                }),
            );
        }
        ("POST", "/mcp") | ("POST", "/") => {
            handle_mcp_post(&mut stream, &req.body, app, ctrl, cfg.allow_mutations);
        }
        _ => {
            write_json(
                &mut stream,
                "404 Not Found",
                &json!({ "error": format!("Unknown route {} {}", req.method, req.path) }),
            );
        }
    }
}

fn handle_mcp_post(
    stream: &mut TcpStream,
    body: &[u8],
    app: &AppHandle,
    ctrl: &Arc<AgentControl>,
    allow_mutations: bool,
) {
    let Ok(rpc) = serde_json::from_slice::<Value>(body) else {
        write_json(
            stream,
            "200 OK",
            &json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": "Parse error" }
            }),
        );
        return;
    };

    let method = rpc["method"].as_str().unwrap_or("");
    let id = rpc.get("id").cloned();

    // JSON-RPC notifications without an `id` return 202 Accepted per MCP Streamable HTTP.
    if id.is_none() || method == "notifications/initialized" || method.starts_with("notifications/")
    {
        write_http_response(stream, "202 Accepted", "application/json", b"");
        return;
    }

    let id_val = id.unwrap_or(Value::Null);
    let resp = match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "cosmic-font-foundry",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
        "ping" => json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "result": {}
        }),
        "tools/list" => {
            ctrl.record_log(
                Some(app),
                "mcp",
                "tools/list",
                "Listed 6 MCP font tools",
                true,
            );
            json!({
                "jsonrpc": "2.0",
                "id": id_val,
                "result": {
                    "tools": mcp_tools_list()
                }
            })
        }
        "tools/call" => {
            let name = rpc["params"]["name"].as_str().unwrap_or("");
            let args = rpc["params"]
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match execute_tool(app, ctrl, "mcp", name, &args, allow_mutations) {
                Ok(out) => {
                    let text = serde_json::to_string_pretty(&out).unwrap_or_default();
                    json!({
                        "jsonrpc": "2.0",
                        "id": id_val,
                        "result": {
                            "content": [{ "type": "text", "text": text }],
                            "isError": false
                        }
                    })
                }
                Err(err_msg) => json!({
                    "jsonrpc": "2.0",
                    "id": id_val,
                    "result": {
                        "content": [{ "type": "text", "text": err_msg }],
                        "isError": true
                    }
                }),
            }
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {method}")
            }
        }),
    };

    write_json(stream, "200 OK", &resp);
}

pub fn mcp_tools_list() -> Vec<Value> {
    vec![
        json!({
            "name": "cosmic_get_status",
            "description": "Inspect live Cosmic Font Foundry inventory metrics, category counts, diagnostics (duplicates, style conflicts, corrupt files), and server status.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "cosmic_search_fonts",
            "description": "Search and filter installed/cached fonts by name, category (sans, serif, mono, display, handwriting, symbol), active state, or origin.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Substring match against family name, foundry, or designer." },
                    "category": { "type": "string", "description": "Optional category: sans, serif, mono, display, handwriting, symbol." },
                    "active": { "type": "boolean", "description": "Filter by active (enabled) state." },
                    "origin": { "type": "string", "description": "Filter by origin: sys, lib, usr, dir." },
                    "limit": { "type": "integer", "description": "Maximum number of font families to return (1-500, default 50)." }
                }
            }
        }),
        json!({
            "name": "cosmic_toggle_fonts",
            "description": "Activate or deactivate one or more font families in the OS/foundry safely (protected Windows essentials cannot be disabled unless system_unlocked=true).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "families": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of exact or matching font family names."
                    },
                    "activate": { "type": "boolean", "description": "true to enable fonts, false to deactivate." },
                    "system_unlocked": { "type": "boolean", "description": "Defaults to false." }
                },
                "required": ["families", "activate"]
            }
        }),
        json!({
            "name": "cosmic_scan_pc_fonts",
            "description": "Detect OS default font directories and scan Downloads, Desktop, Documents, Adobe, or a custom folder for font kits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "custom_dir": { "type": "string", "description": "Optional custom directory path to scan." }
                }
            }
        }),
        json!({
            "name": "cosmic_ui_navigate",
            "description": "Navigate the Cosmic Font Foundry desktop UI to a section (installed, discover, forge, diagnostics, favorites, agent), set search query, category filter, or live sample text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": { "type": "string" },
                    "query": { "type": "string" },
                    "category": { "type": "string" },
                    "sample": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "cosmic_auto_organize",
            "description": "Algorithmically organize OS fonts or build a curated smart collection (e.g. Coding & Terminal, Serif Editorial, Variable Fonts, or custom query) with zero ML models.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["os", "smart_collections"], "description": "Organization mode: 'os' for OS System/User/Mono/Display collections, or 'smart_collections' for targeted grouping." },
                    "collection_name": { "type": "string", "description": "Target collection name when mode='smart_collections'." },
                    "query": { "type": "string", "description": "Search filter for matching fonts into the smart collection." },
                    "category": { "type": "string", "description": "Category filter (sans, serif, mono, display, handwriting)." },
                    "variable_only": { "type": "boolean", "description": "Include only variable fonts." }
                }
            }
        }),
    ]
}

fn is_mutating_tool(name: &str) -> bool {
    matches!(
        name,
        "cosmic_toggle_fonts"
            | "cosmic_scan_pc_fonts"
            | "cosmic_ui_navigate"
            | "cosmic_auto_organize"
    )
}

fn execute_tool(
    app: &AppHandle,
    ctrl: &Arc<AgentControl>,
    source: &str,
    name: &str,
    args: &Value,
    allow_mutations: bool,
) -> Result<Value, String> {
    if is_mutating_tool(name) && !allow_mutations {
        let msg = format!("Tool '{name}' blocked because Read-Only monitoring mode is active.");
        ctrl.record_log(
            Some(app),
            source,
            format!("tool:{name}").as_str(),
            &msg,
            false,
        );
        return Err(msg);
    }

    let result = match name {
        "cosmic_get_status" => Ok(build_status_value(app, ctrl)),
        "cosmic_search_fonts" => {
            let query = args["query"].as_str();
            let category = args["category"].as_str();
            let origin = args["origin"].as_str();
            let active = args["active"].as_bool();
            let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as usize;
            Ok(search_fonts_value(
                app, query, category, active, origin, limit,
            ))
        }
        "cosmic_toggle_fonts" => {
            let raw_families: Vec<String> = args["families"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::trim))
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            if raw_families.is_empty() {
                return Err("Expected non-empty 'families' string array.".into());
            }
            let activate = args["activate"].as_bool().unwrap_or(true);
            let system_unlocked = args["system_unlocked"].as_bool().unwrap_or(false);
            let resolved = resolve_family_names(app, &raw_families);
            let targets = if resolved.is_empty() {
                raw_families
            } else {
                resolved
            };
            let resp = toggle_families_sync(app, targets.clone(), activate, system_unlocked)?;
            Ok(json!({
                "ok": resp.outcome.errors.is_empty() && !resp.outcome.cancelled,
                "families": targets,
                "activate": activate,
                "changed": resp.outcome.changed,
                "locked": resp.outcome.locked,
                "errors": resp.outcome.errors,
                "reboot_required": resp.outcome.reboot_required,
                "cancelled": resp.outcome.cancelled,
                "diagnostics": resp.diagnostics,
            }))
        }
        "cosmic_scan_pc_fonts" => {
            let custom_dir = args["custom_dir"].as_str();
            let state = app.state::<AppState>();
            let report = run_smart_scan(&state, custom_dir);
            let _ = app.emit(
                "agent-ui-command",
                json!({ "action": "scan_report", "report": &report }),
            );
            Ok(serde_json::to_value(report).unwrap_or_else(|_| json!({})))
        }
        "cosmic_ui_navigate" => {
            let payload = json!({
                "action": "navigate",
                "section": args["section"].as_str(),
                "query": args["query"].as_str(),
                "category": args["category"].as_str(),
                "sample": args["sample"].as_str(),
            });
            let _ = app.emit("agent-ui-command", &payload);
            Ok(json!({ "ok": true, "navigated": payload }))
        }
        "cosmic_auto_organize" => {
            let mode = args["mode"].as_str().unwrap_or("os");
            if mode == "os" {
                let _ = app.emit(
                    "agent-ui-command",
                    json!({ "action": "auto_organize", "mode": "os" }),
                );
                let state = app.state::<AppState>();
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
                let mono = cache
                    .families
                    .iter()
                    .filter(|f| f.category == "mono")
                    .count();
                let disp = cache
                    .families
                    .iter()
                    .filter(|f| f.category == "display")
                    .count();
                Ok(json!({
                    "ok": true,
                    "mode": "os",
                    "groups": {
                        "System Core Fonts": sys,
                        "User & Installed": usr,
                        "Monospace & Code": mono,
                        "Display & Headlines": disp,
                    }
                }))
            } else {
                let col_name = args["collection_name"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Agent Curated Kit");
                let query = args["query"].as_str().map(str::to_lowercase);
                let category = args["category"].as_str().map(str::to_lowercase);
                let variable_only = args["variable_only"].as_bool().unwrap_or(false);
                let state = app.state::<AppState>();
                let matched: Vec<String> = {
                    let cache = state.cache.lock().unwrap();
                    cache
                        .families
                        .iter()
                        .filter(|f| {
                            if variable_only && !f.is_variable {
                                return false;
                            }
                            if let Some(cat) = &category {
                                if f.category.to_lowercase() != *cat {
                                    return false;
                                }
                            }
                            if let Some(q) = &query {
                                if !f.name.to_lowercase().contains(q)
                                    && !f.category.to_lowercase().contains(q)
                                {
                                    return false;
                                }
                            }
                            true
                        })
                        .map(|f| f.name.clone())
                        .collect()
                };
                let _ = app.emit(
                    "agent-ui-command",
                    json!({
                        "action": "auto_organize",
                        "mode": "smart_collections",
                        "collection_name": col_name,
                        "families": &matched,
                    }),
                );
                Ok(json!({
                    "ok": true,
                    "mode": "smart_collections",
                    "collection_name": col_name,
                    "count": matched.len(),
                    "families": matched,
                }))
            }
        }
        _ => Err(format!("Unknown MCP tool: {name}")),
    };

    match &result {
        Ok(_) => ctrl.record_log(
            Some(app),
            source,
            format!("tool:{name}").as_str(),
            summarize_tool_log(name, args),
            true,
        ),
        Err(e) => ctrl.record_log(
            Some(app),
            source,
            format!("tool:{name}").as_str(),
            e.clone(),
            false,
        ),
    }

    result
}

fn summarize_tool_log(name: &str, args: &Value) -> String {
    match name {
        "cosmic_toggle_fonts" => format!(
            "families={:?}, activate={}",
            args["families"],
            args["activate"].as_bool().unwrap_or(true)
        ),
        "cosmic_search_fonts" => format!(
            "q={:?}, category={:?}",
            args["query"].as_str().unwrap_or(""),
            args["category"].as_str().unwrap_or("all")
        ),
        "cosmic_auto_organize" => format!(
            "mode={}, collection={}",
            args["mode"].as_str().unwrap_or("os"),
            args["collection_name"].as_str().unwrap_or("OS Auto")
        ),
        _ => format!("Executed {name}"),
    }
}

fn build_status_value(app: &AppHandle, ctrl: &Arc<AgentControl>) -> Value {
    let state = app.state::<AppState>();
    let families = state.cache.lock().unwrap().families.clone();
    let diag = summarize_diagnostics(&families);
    let active_families = families.iter().filter(|f| f.active).count();
    let variable_families = families.iter().filter(|f| f.is_variable).count();
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    for fam in &families {
        *categories.entry(fam.category.clone()).or_insert(0) += 1;
    }
    let snap = ctrl.snapshot();
    // Intentionally excludes `api_key` from unauthenticated HTTP status responses.
    json!({
        "app": "Cosmic Font Foundry",
        "version": env!("CARGO_PKG_VERSION"),
        "listening": snap.listening,
        "mcp_url": snap.mcp_url,
        "api_base": snap.api_base,
        "allow_mutations": snap.config.allow_mutations,
        "provider": snap.config.provider,
        "total_families": families.len(),
        "active_families": active_families,
        "inactive_families": families.len().saturating_sub(active_families),
        "variable_families": variable_families,
        "categories": categories,
        "diagnostics": diag,
    })
}

fn compact_family_json(fam: &FontFamily) -> Value {
    json!({
        "name": fam.name,
        "category": fam.category,
        "origin": fam.origin,
        "active": fam.active,
        "essential": fam.essential,
        "styles_count": fam.styles.len(),
        "scripts": fam.scripts,
        "is_variable": fam.is_variable,
        "foundry": fam.foundry,
        "has_conflicts": fam.has_conflicts,
        "error": fam.error,
        "file": fam.file,
    })
}

fn search_fonts_value(
    app: &AppHandle,
    query: Option<&str>,
    category: Option<&str>,
    active: Option<bool>,
    origin: Option<&str>,
    limit: usize,
) -> Value {
    let state = app.state::<AppState>();
    let families = state.cache.lock().unwrap().families.clone();
    let q_lower = query
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);
    let cat_lower = category
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "all")
        .map(str::to_lowercase);
    let org_lower = origin
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);

    let matches: Vec<Value> = families
        .iter()
        .filter(|f| {
            if let Some(req_active) = active {
                if f.active != req_active {
                    return false;
                }
            }
            if let Some(cat) = &cat_lower {
                if f.category.to_lowercase() != *cat {
                    return false;
                }
            }
            if let Some(org) = &org_lower {
                let f_org = match f.origin {
                    Origin::System => "sys",
                    Origin::Library => "lib",
                    Origin::User => "usr",
                    Origin::Folder => "dir",
                };
                if f_org != *org {
                    return false;
                }
            }
            if let Some(q) = &q_lower {
                let hit = f.name.to_lowercase().contains(q)
                    || f.foundry
                        .as_deref()
                        .is_some_and(|x| x.to_lowercase().contains(q))
                    || f.designer
                        .as_deref()
                        .is_some_and(|x| x.to_lowercase().contains(q));
                if !hit {
                    return false;
                }
            }
            true
        })
        .map(compact_family_json)
        .collect();

    let total_matches = matches.len();
    let truncated: Vec<Value> = matches.into_iter().take(limit).collect();
    json!({
        "total_matches": total_matches,
        "returned": truncated.len(),
        "fonts": truncated,
    })
}

fn resolve_family_names(app: &AppHandle, names: &[String]) -> Vec<String> {
    let state = app.state::<AppState>();
    let families = state.cache.lock().unwrap().families.clone();
    let mut out = Vec::new();
    for needle in names {
        let lower = needle.to_lowercase();
        if let Some(exact) = families.iter().find(|f| f.name.to_lowercase() == lower) {
            out.push(exact.name.clone());
        } else if let Some(part) = families
            .iter()
            .find(|f| f.name.to_lowercase().contains(&lower))
        {
            out.push(part.name.clone());
        }
    }
    out
}

fn url_decode(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(byte) = u8::from_str_radix(hex, 16) {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query_params(query: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(url_decode(k), url_decode(v));
    }
    map
}

#[derive(Debug, PartialEq)]
enum DeterministicIntent {
    AuditHealth,
    OrganizeOs,
    CreateCollection {
        name: String,
        category: Option<String>,
        query: Option<String>,
        variable_only: bool,
    },
    ScanPc(Option<String>),
    Navigate {
        section: Option<String>,
        query: Option<String>,
        category: Option<String>,
    },
    Toggle {
        activate: bool,
        families: Vec<String>,
    },
    Search {
        query: Option<String>,
        category: Option<String>,
        variable_only: bool,
    },
}

fn parse_deterministic_intent(prompt: &str) -> DeterministicIntent {
    let trimmed = prompt.trim();
    let lower = trimmed.to_lowercase();

    if lower.contains("audit")
        || lower.contains("health")
        || lower.contains("diagnostic")
        || lower.contains("duplicate")
        || lower.contains("corrupt")
    {
        return DeterministicIntent::AuditHealth;
    }

    if (lower.contains("organize") || lower.contains("auto"))
        && (lower.contains("os") || lower.contains("system") || lower.contains("default"))
    {
        return DeterministicIntent::OrganizeOs;
    }

    if lower.contains("coding") || lower.contains("terminal") || lower.contains("programmer") {
        return DeterministicIntent::CreateCollection {
            name: "Coding & Terminal".into(),
            category: Some("mono".into()),
            query: None,
            variable_only: false,
        };
    }

    if lower.contains("collection")
        || lower.contains("group")
        || lower.contains("organize")
        || lower.contains("kit")
    {
        let cat = detect_category_keyword(&lower);
        let variable_only = lower.contains("variable");
        let name = if variable_only {
            "Variable Typography".into()
        } else if let Some(c) = &cat {
            format!("{} Curated", capitalize(c))
        } else {
            "Agent Curated Fonts".into()
        };
        return DeterministicIntent::CreateCollection {
            name,
            category: cat,
            query: None,
            variable_only,
        };
    }

    if lower.contains("scan") {
        return DeterministicIntent::ScanPc(None);
    }

    for prefix in [
        "enable ",
        "activate ",
        "turn on ",
        "disable ",
        "deactivate ",
        "turn off ",
    ] {
        if let Some(rest) = trimmed
            .get(..prefix.len())
            .filter(|head| head.eq_ignore_ascii_case(prefix))
            .and_then(|_| trimmed.get(prefix.len()..))
        {
            let activate = prefix.starts_with("en")
                || prefix.starts_with("ac")
                || prefix.starts_with("turn on");
            let families: Vec<String> = rest
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            if !families.is_empty() {
                return DeterministicIntent::Toggle { activate, families };
            }
        }
    }

    if lower.contains("open ") || lower.contains("go to ") || lower.contains("navigate") {
        let section = if lower.contains("forge") || lower.contains("studio") {
            Some("forge".into())
        } else if lower.contains("discover") || lower.contains("catalog") {
            Some("discover".into())
        } else if lower.contains("diagnostic") || lower.contains("health") {
            Some("diagnostics".into())
        } else if lower.contains("favorite") {
            Some("favorites".into())
        } else if lower.contains("agent") {
            Some("agent".into())
        } else {
            Some("installed".into())
        };
        return DeterministicIntent::Navigate {
            section,
            query: None,
            category: detect_category_keyword(&lower),
        };
    }

    let cat = detect_category_keyword(&lower);
    let variable_only = lower.contains("variable");
    let query = extract_search_query(&lower);
    DeterministicIntent::Search {
        query,
        category: cat,
        variable_only,
    }
}

fn detect_category_keyword(lower: &str) -> Option<String> {
    if lower.contains("sans") {
        Some("sans".into())
    } else if lower.contains("serif") {
        Some("serif".into())
    } else if lower.contains("mono") || lower.contains("code") {
        Some("mono".into())
    } else if lower.contains("display") || lower.contains("headline") {
        Some("display".into())
    } else if lower.contains("handwriting") || lower.contains("script") {
        Some("handwriting".into())
    } else if lower.contains("symbol") || lower.contains("icon") {
        Some("symbol".into())
    } else {
        None
    }
}

fn extract_search_query(lower: &str) -> Option<String> {
    let mut s = lower.to_string();
    for noise in [
        "find ",
        "search ",
        "show ",
        "list ",
        "filter ",
        "all ",
        "fonts",
        "font",
        "variable",
        "sans",
        "serif",
        "mono",
        "display",
        "handwriting",
        "symbol",
    ] {
        s = s.replace(noise, " ");
    }
    let clean = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn run_deterministic_prompt(
    app: &AppHandle,
    ctrl: &Arc<AgentControl>,
    prompt: &str,
) -> AgentCommandResult {
    let allow_mutations = ctrl.config.lock().unwrap().allow_mutations;
    let intent = parse_deterministic_intent(prompt);
    let mut executed = Vec::new();
    let mut ok = true;
    let reply = match intent {
        DeterministicIntent::AuditHealth => {
            let status = build_status_value(app, ctrl);
            executed.push(json!({ "tool": "cosmic_get_status", "result": status }));
            if allow_mutations {
                let _ = execute_tool(
                    app,
                    ctrl,
                    "director",
                    "cosmic_ui_navigate",
                    &json!({ "section": "diagnostics" }),
                    true,
                );
            }
            let d = &status["diagnostics"];
            format!(
                "Health Audit complete (0 MB local engine): {} families total ({} active, {} variable), {} exact duplicates, {} style conflicts, {} corrupt files.",
                status["total_families"],
                status["active_families"],
                status["variable_families"],
                d["exact_duplicates"],
                d["style_conflicts"],
                d["corrupt_files"],
            )
        }
        DeterministicIntent::OrganizeOs => {
            match execute_tool(
                app,
                ctrl,
                "director",
                "cosmic_auto_organize",
                &json!({ "mode": "os" }),
                allow_mutations,
            ) {
                Ok(out) => {
                    executed.push(json!({ "tool": "cosmic_auto_organize", "result": out }));
                    format!(
                        "OS Collections organized algorithmically: System ({}), User ({}), Monospace ({}), Display ({}).",
                        out["groups"]["System Core Fonts"],
                        out["groups"]["User & Installed"],
                        out["groups"]["Monospace & Code"],
                        out["groups"]["Display & Headlines"],
                    )
                }
                Err(e) => {
                    ok = false;
                    e
                }
            }
        }
        DeterministicIntent::CreateCollection {
            name,
            category,
            query,
            variable_only,
        } => {
            let args = json!({
                "mode": "smart_collections",
                "collection_name": name,
                "category": category,
                "query": query,
                "variable_only": variable_only,
            });
            match execute_tool(
                app,
                ctrl,
                "director",
                "cosmic_auto_organize",
                &args,
                allow_mutations,
            ) {
                Ok(out) => {
                    executed.push(json!({ "tool": "cosmic_auto_organize", "result": out }));
                    format!(
                        "Created/updated collection '{}' with {} matching families.",
                        out["collection_name"].as_str().unwrap_or("Collection"),
                        out["count"].as_u64().unwrap_or(0)
                    )
                }
                Err(e) => {
                    ok = false;
                    e
                }
            }
        }
        DeterministicIntent::ScanPc(dir) => {
            match execute_tool(
                app,
                ctrl,
                "director",
                "cosmic_scan_pc_fonts",
                &json!({ "custom_dir": dir }),
                allow_mutations,
            ) {
                Ok(out) => {
                    executed.push(json!({ "tool": "cosmic_scan_pc_fonts", "result": out }));
                    let folders = out["discovered_folders"].as_array().map_or(0, Vec::len);
                    format!(
                        "Scanned {} & user directories: {} system fonts, {} user fonts, {} extra font folders discovered.",
                        out["os_name"].as_str().unwrap_or("OS"),
                        out["system_font_count"],
                        out["user_font_count"],
                        folders
                    )
                }
                Err(e) => {
                    ok = false;
                    e
                }
            }
        }
        DeterministicIntent::Navigate {
            section,
            query,
            category,
        } => {
            let args = json!({
                "section": section,
                "query": query,
                "category": category,
            });
            match execute_tool(
                app,
                ctrl,
                "director",
                "cosmic_ui_navigate",
                &args,
                allow_mutations,
            ) {
                Ok(out) => {
                    executed.push(json!({ "tool": "cosmic_ui_navigate", "result": out }));
                    format!(
                        "Navigated UI to '{}'.",
                        section.as_deref().unwrap_or("installed")
                    )
                }
                Err(e) => {
                    ok = false;
                    e
                }
            }
        }
        DeterministicIntent::Toggle { activate, families } => {
            let args = json!({
                "families": families,
                "activate": activate,
            });
            match execute_tool(
                app,
                ctrl,
                "director",
                "cosmic_toggle_fonts",
                &args,
                allow_mutations,
            ) {
                Ok(out) => {
                    executed.push(json!({ "tool": "cosmic_toggle_fonts", "result": out }));
                    let verb = if activate { "Activated" } else { "Deactivated" };
                    format!(
                        "{verb} {} font files across {}.",
                        out["changed"], out["families"]
                    )
                }
                Err(e) => {
                    ok = false;
                    e
                }
            }
        }
        DeterministicIntent::Search {
            query,
            category,
            variable_only,
        } => {
            let mut out =
                search_fonts_value(app, query.as_deref(), category.as_deref(), None, None, 25);
            if variable_only {
                if let Some(arr) = out["fonts"].as_array_mut() {
                    arr.retain(|f| f["is_variable"].as_bool() == Some(true));
                    let len = arr.len();
                    out["total_matches"] = json!(len);
                    out["returned"] = json!(len);
                }
            }
            ctrl.record_log(
                Some(app),
                "director",
                "intent:search",
                format!(
                    "query={:?}, category={:?}, matched={}",
                    query, category, out["total_matches"]
                ),
                true,
            );
            if allow_mutations {
                let _ = execute_tool(
                    app,
                    ctrl,
                    "director",
                    "cosmic_ui_navigate",
                    &json!({
                        "section": "installed",
                        "query": query,
                        "category": category.as_deref().unwrap_or("all"),
                    }),
                    true,
                );
            }
            let preview_names: Vec<&str> = out["fonts"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .take(5)
                        .filter_map(|v| v["name"].as_str())
                        .collect()
                })
                .unwrap_or_default();
            executed.push(json!({ "tool": "cosmic_search_fonts", "result": out }));
            format!(
                "Found {} matching fonts{}.",
                out["total_matches"],
                if preview_names.is_empty() {
                    String::new()
                } else {
                    format!(": {}", preview_names.join(", "))
                }
            )
        }
    };

    AgentCommandResult {
        ok,
        provider: "builtin".into(),
        model: "deterministic-v1".into(),
        reply,
        executed_actions: executed,
        snapshot: ctrl.snapshot(),
    }
}

async fn run_external_ai_prompt(
    app: &AppHandle,
    ctrl: &Arc<AgentControl>,
    cfg: &AgentConfig,
    prompt: &str,
) -> Result<AgentCommandResult, String> {
    let status = build_status_value(app, ctrl);
    let system_prompt = format!(
        "You are the Agentic Font Controller for Cosmic Font Foundry. \
         Respond ONLY with strict JSON matching: \
         {{\"reply\": string, \"actions\": [{{\"tool\": string, \"arguments\": object}}]}}. \
         Available tools: cosmic_get_status, cosmic_search_fonts, cosmic_toggle_fonts, \
         cosmic_scan_pc_fonts, cosmic_ui_navigate, cosmic_auto_organize. \
         Current status: {}",
        status
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let base = cfg.endpoint.trim_end_matches('/');
    let raw_content = if cfg.provider == "ollama" {
        let url = format!("{base}/api/chat");
        let resp = client
            .post(&url)
            .json(&json!({
                "model": cfg.model,
                "stream": false,
                "format": "json",
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .map_err(|e| format!("Ollama request failed ({url}): {e}"))?;
        let val: Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid Ollama JSON response: {e}"))?;
        val["message"]["content"].as_str().unwrap_or("").to_string()
    } else {
        let url = if base.ends_with("/v1") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        };
        let mut req = client.post(&url).json(&json!({
            "model": cfg.model,
            "temperature": 0.1,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": prompt }
            ]
        }));
        if !cfg.api_key.is_empty() {
            req = req.bearer_auth(&cfg.api_key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("OpenAI-compatible request failed ({url}): {e}"))?;
        let val: Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid OpenAI JSON response: {e}"))?;
        val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };

    let json_text = extract_json_object(&raw_content).unwrap_or(&raw_content);
    let plan: Value = serde_json::from_str(json_text)
        .unwrap_or_else(|_| json!({ "reply": raw_content, "actions": [] }));

    let mut executed = Vec::new();
    let mut all_ok = true;
    if let Some(actions) = plan["actions"].as_array() {
        for act in actions {
            let tool = act["tool"].as_str().unwrap_or("");
            let args = act.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if tool.is_empty() {
                continue;
            }
            match execute_tool(app, ctrl, &cfg.provider, tool, &args, cfg.allow_mutations) {
                Ok(res) => executed.push(json!({ "tool": tool, "result": res })),
                Err(err) => {
                    all_ok = false;
                    executed.push(json!({ "tool": tool, "error": err }));
                }
            }
        }
    }

    let reply = plan["reply"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Completed AI agent request.")
        .to_string();

    Ok(AgentCommandResult {
        ok: all_ok,
        provider: cfg.provider.clone(),
        model: cfg.model.clone(),
        reply,
        executed_actions: executed,
        snapshot: ctrl.snapshot(),
    })
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end >= start {
        Some(&raw[start..=end])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tools_list_and_status_schema_are_valid() {
        let tools = mcp_tools_list();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"cosmic_get_status"));
        assert!(names.contains(&"cosmic_search_fonts"));
        assert!(names.contains(&"cosmic_toggle_fonts"));
        assert!(names.contains(&"cosmic_scan_pc_fonts"));
        assert!(names.contains(&"cosmic_ui_navigate"));
        assert!(names.contains(&"cosmic_auto_organize"));
        for tool in tools {
            assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn deterministic_intent_router_parses_commands() {
        assert_eq!(
            parse_deterministic_intent("Audit Font Health"),
            DeterministicIntent::AuditHealth
        );
        assert_eq!(
            parse_deterministic_intent("Organize OS Fonts"),
            DeterministicIntent::OrganizeOs
        );
        assert_eq!(
            parse_deterministic_intent("Create Coding & Terminal Collection"),
            DeterministicIntent::CreateCollection {
                name: "Coding & Terminal".into(),
                category: Some("mono".into()),
                query: None,
                variable_only: false,
            }
        );
        assert_eq!(
            parse_deterministic_intent("enable Inter, JetBrains Mono"),
            DeterministicIntent::Toggle {
                activate: true,
                families: vec!["Inter".into(), "JetBrains Mono".into()],
            }
        );
        assert_eq!(
            parse_deterministic_intent("find variable sans fonts"),
            DeterministicIntent::Search {
                query: None,
                category: Some("sans".into()),
                variable_only: true,
            }
        );
    }
}
