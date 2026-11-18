//! Windows startup registration (`HKCU\...\Run`) so watched folders load at sign-in
//! while starting minimized to the system tray (`--hidden`).

use std::path::{Path, PathBuf};
use winreg::{
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
    RegKey,
};

use crate::err;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
/// Matches `productName` so the NSIS uninstaller cleans this value automatically.
const VALUE_NAME: &str = "Cosmic Font Foundry";
pub const HIDDEN_ARG: &str = "--hidden";
const SHOW_ON_OPEN_MARKER: &str = "show-on-open";

fn exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(err)
}

fn startup_command(exe: &Path) -> String {
    format!("\"{}\" {HIDDEN_ARG}", exe.display())
}

fn is_disabled_in_task_manager(hkcu: &RegKey) -> bool {
    hkcu.open_subkey(APPROVED_KEY)
        .and_then(|k| k.get_raw_value(VALUE_NAME))
        .map(|v| v.bytes.first().is_some_and(|b| b & 1 == 1))
        .unwrap_or(false)
}

pub fn is_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(val) = hkcu
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
    else {
        return false;
    };
    let Ok(exe) = exe_path() else { return false };
    val.to_lowercase()
        .contains(&exe.to_string_lossy().to_lowercase())
        && !is_disabled_in_task_manager(&hkcu)
}

pub fn set_enabled(enable: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enable {
        let (run, _) = hkcu.create_subkey(RUN_KEY).map_err(err)?;
        run.set_value(VALUE_NAME, &startup_command(&exe_path()?))
            .map_err(err)?;
        if let Ok(k) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_READ | KEY_WRITE) {
            let _ = k.delete_value(VALUE_NAME);
        }
    } else {
        if let Ok(run) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) {
            if let Err(e) = run.delete_value(VALUE_NAME) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(err(e));
                }
            }
        }
        if let Ok(k) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_READ | KEY_WRITE) {
            let _ = k.delete_value(VALUE_NAME);
        }
    }
    Ok(())
}

/// Consumes the `show-on-open` marker (used before auto-update restart) and decides whether to launch hidden.
pub fn should_start_hidden(data_dir: &Path) -> bool {
    let marker = data_dir.join(SHOW_ON_OPEN_MARKER);
    let had_marker = marker.exists();
    let _ = std::fs::remove_file(&marker);
    std::env::args_os().skip(1).any(|a| a == HIDDEN_ARG) && !had_marker
}

pub fn write_show_marker(data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(err)?;
    std::fs::write(data_dir.join(SHOW_ON_OPEN_MARKER), b"").map_err(err)
}

#[tauri::command]
pub fn windows_startup() -> bool {
    is_enabled()
}

#[tauri::command]
pub fn set_windows_startup(enable: bool) -> Result<bool, String> {
    set_enabled(enable)?;
    Ok(is_enabled())
}

#[tauri::command]
pub fn mark_show_on_open(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    write_show_marker(&app.path().app_data_dir().map_err(err)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_marker_is_consumed_on_launch() {
        let tmp = tempfile::tempdir().unwrap();
        write_show_marker(tmp.path()).unwrap();
        assert!(tmp.path().join(SHOW_ON_OPEN_MARKER).exists());
        assert!(!should_start_hidden(tmp.path()));
        assert!(!tmp.path().join(SHOW_ON_OPEN_MARKER).exists());
    }
}
