//! Elevated worker for system-wide (`HKLM` / `C:\Windows\Fonts`) font operations.
//! Relaunches the binary via UAC (`--elevated <request.json>`) so regular runs stay non-elevated.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{is_essential, reader, registry, Environment};
use crate::win;

pub const CANCELLED: &str = "cancelled";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum SystemOp {
    Deactivate {
        path: PathBuf,
        backup: PathBuf,
        value: String,
        data: String,
    },
    Activate {
        backup: PathBuf,
        path: PathBuf,
        value: String,
        data: String,
    },
    Install {
        source: PathBuf,
        path: PathBuf,
        value: String,
        data: String,
    },
}

#[derive(Serialize, Deserialize)]
struct ElevatedRequest {
    key: String,
    fonts_dir: PathBuf,
    backup_dir: PathBuf,
    ops: Vec<SystemOp>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ElevatedResponse {
    pub results: Vec<Option<String>>,
    pub reboot_required: bool,
}

pub fn request_elevated(env: &Environment, ops: &[SystemOp]) -> Result<ElevatedResponse, String> {
    fs::create_dir_all(env.system_backup_dir()).map_err(|e| e.to_string())?;
    let req_path = env.data_dir.join("elevated-request.json");
    let resp_path = env.data_dir.join("elevated-response.json");
    let _ = fs::remove_file(&resp_path);

    let req = ElevatedRequest {
        key: env.system_key.clone(),
        fonts_dir: env.system_fonts.clone(),
        backup_dir: env.system_backup_dir(),
        ops: ops.to_vec(),
    };
    fs::write(&req_path, serde_json::to_vec(&req).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let code = win::run_elevated(&format!("--elevated \"{}\"", req_path.display()));
    let _ = fs::remove_file(&req_path);
    if code.is_none() {
        return Err(CANCELLED.into());
    }
    let bytes = fs::read(&resp_path)
        .map_err(|_| "failed to complete elevated administrator operation".to_string())?;
    let _ = fs::remove_file(&resp_path);
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn is_direct_child(path: &Path, dir: &Path) -> bool {
    path.parent()
        .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case(&dir.to_string_lossy()))
}

fn is_protected(path: &Path) -> bool {
    reader::read_faces(path)
        .map(|faces| faces.iter().any(|f| is_essential(&f.family)))
        .unwrap_or(false)
}

fn execute_one(req: &ElevatedRequest, op: &SystemOp, reboot: &mut bool) -> Result<(), String> {
    let hklm = registry::hklm();
    match op {
        SystemOp::Deactivate {
            path,
            backup,
            value,
            data,
        } => {
            if !is_direct_child(path, &req.fonts_dir) || !is_direct_child(backup, &req.backup_dir) {
                return Err("path outside allowed system or backup directory".into());
            }
            if is_protected(path) {
                return Err("essential Windows font is protected".into());
            }
            fs::copy(path, backup).map_err(|e| e.to_string())?;
            let reg_meta = serde_json::json!({
                "key": format!(r"HKLM\{}", req.key),
                "value": value,
                "data": data
            });
            fs::write(backup.with_extension("reg.json"), reg_meta.to_string())
                .map_err(|e| e.to_string())?;
            if !value.is_empty() {
                registry::delete_value(hklm, &req.key, value)?;
            }
            if fs::remove_file(path).is_err() {
                if !win::move_on_reboot(path, None) {
                    let _ = registry::set_value(hklm, &req.key, value, data);
                    return Err("font file is currently locked by Windows".into());
                }
                *reboot = true;
            }
            Ok(())
        }
        SystemOp::Activate {
            backup,
            path,
            value,
            data,
        } => {
            if !is_direct_child(path, &req.fonts_dir) || !is_direct_child(backup, &req.backup_dir) {
                return Err("path outside allowed system or backup directory".into());
            }
            if path.exists() {
                return Err("restart Windows before reactivating this system font".into());
            }
            fs::copy(backup, path).map_err(|e| e.to_string())?;
            if !value.is_empty() {
                registry::set_value(hklm, &req.key, value, data)?;
            }
            Ok(())
        }
        SystemOp::Install {
            source,
            path,
            value,
            data,
        } => {
            if !is_direct_child(path, &req.fonts_dir) || path.exists() {
                return Err("destination path is invalid or already exists".into());
            }
            reader::read_faces(source)?;
            fs::copy(source, path).map_err(|e| e.to_string())?;
            registry::set_value(hklm, &req.key, value, data)
        }
    }
}

pub fn handle_request(req_path: &Path) -> i32 {
    let Ok(bytes) = fs::read(req_path) else {
        return 2;
    };
    let Ok(req) = serde_json::from_slice::<ElevatedRequest>(&bytes) else {
        return 2;
    };
    let mut resp = ElevatedResponse::default();
    for op in &req.ops {
        resp.results
            .push(execute_one(&req, op, &mut resp.reboot_required).err());
    }
    let resp_path = req_path.with_file_name("elevated-response.json");
    match serde_json::to_vec(&resp)
        .ok()
        .and_then(|b| fs::write(resp_path, b).ok())
    {
        Some(()) => 0,
        None => 3,
    }
}
