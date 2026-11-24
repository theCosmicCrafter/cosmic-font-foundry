//! Non-destructive font activation, deactivation, installation, and corrupt-font quarantine.

use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::{
    cache::Cache, is_essential, load_app_installed, load_deactivated, load_recorded_windows_fonts,
    load_system_backups, origin_in_system_dir, path_key, reader::FontFace, registry,
    save_app_installed, save_deactivated, save_recorded_windows_fonts, save_system_backups, system,
    Environment, FontEntry, Origin, SavedFont,
};
use crate::win;

#[derive(Serialize, Default, Debug)]
pub struct ToggleOutcome {
    pub changed: usize,
    pub locked: usize,
    pub errors: Vec<String>,
    pub reboot_required: bool,
    pub cancelled: bool,
    pub installed: Vec<String>,
}

fn filename(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub use cosmic_core::inventory::unique_path;

pub fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(in_use_err)?;
    if let Err(e) = fs::remove_file(from) {
        let _ = fs::remove_file(to);
        return Err(in_use_err(e));
    }
    Ok(())
}

fn in_use_err(e: std::io::Error) -> String {
    if e.raw_os_error() == Some(32) {
        "file is currently open in another application".into()
    } else {
        e.to_string()
    }
}

fn is_otf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("otf"))
}

pub fn deactivate_user(env: &Environment, entry: &FontEntry) -> Result<(), String> {
    let records = super::load_install_records(env);
    if let Some(expected_sha) = records.get(&path_key(&entry.path)) {
        let actual_sha = super::reader::sha256_file(&entry.path)?;
        if &actual_sha != expected_sha {
            return Err(format!(
                "{}: file changed externally since installation; deactivation cancelled",
                filename(&entry.path)
            ));
        }
    }
    if env.touch_windows {
        win::unload_font(&entry.path, false);
    }
    let dest = unique_path(&env.deactivated_dir(), &filename(&entry.path));
    if let Err(err) = move_file(&entry.path, &dest) {
        if env.touch_windows {
            win::load_font(&entry.path, false);
        }
        return Err(err);
    }
    let (value, data) = entry.value.clone().unwrap_or_default();
    if !value.is_empty() {
        if let Err(err) = registry::delete_value(registry::hkcu(), &env.user_key, &value) {
            let _ = move_file(&dest, &entry.path);
            return Err(err);
        }
    }
    let mut saved = load_deactivated(env);
    saved.push(SavedFont {
        file: dest,
        original: entry.path.clone(),
        value,
        data,
        origin: entry.origin,
    });
    save_deactivated(env, &saved)
}

pub fn activate_user(
    env: &Environment,
    entry: &FontEntry,
    faces: &[FontFace],
) -> Result<PathBuf, String> {
    let mut saved = load_deactivated(env);
    let idx = saved
        .iter()
        .position(|s| path_key(&s.file) == path_key(&entry.path));
    let (dest_path, prev_value) = match idx {
        Some(i) => {
            let s = &saved[i];
            let target = if s.original.exists() {
                unique_path(
                    s.original.parent().unwrap_or(&env.user_fonts),
                    &filename(&s.original),
                )
            } else {
                s.original.clone()
            };
            move_file(&s.file, &target)?;
            (target, s.value.clone())
        }
        None => (entry.path.clone(), String::new()),
    };

    let value = if prev_value.is_empty() {
        registry::format_value_name(
            &faces
                .iter()
                .map(|f| f.full_name.clone())
                .collect::<Vec<_>>(),
            is_otf(&dest_path),
        )
    } else {
        prev_value
    };
    registry::set_value(
        registry::hkcu(),
        &env.user_key,
        &value,
        &dest_path.to_string_lossy(),
    )?;

    if let Some(i) = idx {
        saved.remove(i);
        save_deactivated(env, &saved)?;
    }
    if env.touch_windows {
        win::load_font(&dest_path, false);
    }
    Ok(dest_path)
}

pub fn install_user(env: &Environment, src: &Path, faces: &[FontFace]) -> Result<PathBuf, String> {
    let src_sha = super::reader::sha256_file(src)?;
    let fname = filename(src);
    let dest = env.user_fonts.join(&fname);
    fs::create_dir_all(&env.user_fonts).map_err(|e| e.to_string())?;

    let mut copied_new = false;
    if dest.exists() {
        let existing_sha = super::reader::sha256_file(&dest)?;
        if existing_sha != src_sha {
            return Err(format!(
                "{fname}: a different font file already exists at {}; install cancelled to avoid overwriting",
                dest.display()
            ));
        }
    } else {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    format!(
                        "{fname}: a font file was created concurrently at {}; install cancelled",
                        dest.display()
                    )
                } else {
                    e.to_string()
                }
            })?;
        let mut inp = fs::File::open(src).map_err(|e| e.to_string())?;
        if let Err(e) = std::io::copy(&mut inp, &mut out) {
            drop(out);
            let _ = fs::remove_file(&dest);
            return Err(e.to_string());
        }
        copied_new = true;
    }

    let value = registry::format_value_name(
        &faces
            .iter()
            .map(|f| f.full_name.clone())
            .collect::<Vec<_>>(),
        is_otf(&dest),
    );
    if let Err(e) = registry::set_value(
        registry::hkcu(),
        &env.user_key,
        &value,
        &dest.to_string_lossy(),
    ) {
        if copied_new {
            let _ = fs::remove_file(&dest);
        }
        return Err(e);
    }
    let key = path_key(&dest);
    let mut app_installed = load_app_installed(env);
    app_installed.insert(key.clone());
    save_app_installed(env, &app_installed)?;
    let mut records = super::load_install_records(env);
    records.insert(key, src_sha);
    super::save_install_records(env, &records)?;
    if env.touch_windows {
        win::load_font(&dest, false);
    }
    Ok(dest)
}

pub fn quarantine_corrupt_file(env: &Environment, target: &Path) -> Result<PathBuf, String> {
    if !target.is_file() {
        return Err("file does not exist".into());
    }
    if target.starts_with(&env.system_fonts) {
        return Err(
            "system folder files cannot be quarantined directly without administrator recovery"
                .into(),
        );
    }
    if env.touch_windows {
        win::unload_font(target, false);
    }
    let key = path_key(target);
    for (val_name, val_data) in registry::read_values(registry::hkcu(), &env.user_key) {
        let reg_path = PathBuf::from(&val_data);
        let resolved = if reg_path.is_absolute() {
            reg_path
        } else {
            env.user_fonts.join(reg_path)
        };
        if path_key(&resolved) == key {
            let _ = registry::delete_value(registry::hkcu(), &env.user_key, &val_name);
        }
    }
    let dest = unique_path(&env.quarantine_dir(), &filename(target));
    move_file(target, &dest)?;
    Ok(dest)
}

fn families_for(cache: &Cache, entry: &FontEntry) -> Vec<String> {
    cache
        .faces(&entry.path)
        .map(|faces| faces.into_iter().map(|f| f.family).collect())
        .unwrap_or_default()
}

pub fn toggle_families(
    env: &Environment,
    entries: &[FontEntry],
    cache: &Cache,
    families: &[String],
    activate: bool,
    system_unlocked: bool,
    disabled_folder_fonts: &mut HashSet<String>,
) -> ToggleOutcome {
    let wanted: HashSet<String> = families.iter().map(|f| f.to_lowercase()).collect();
    let mut out = ToggleOutcome::default();
    let mut sys_pending: Vec<(&FontEntry, system::SystemOp)> = Vec::new();

    for entry in entries {
        let fams = families_for(cache, entry);
        if !fams.iter().any(|f| wanted.contains(&f.to_lowercase())) || entry.active == activate {
            continue;
        }
        let label = fams
            .first()
            .cloned()
            .unwrap_or_else(|| filename(&entry.path));
        let res = match entry.origin {
            Origin::User | Origin::Library
                if entry.path.starts_with(&env.system_fonts) || is_in_system_backup(env, entry) =>
            {
                if let Some(op) = build_system_op(env, entry, activate) {
                    sys_pending.push((entry, op));
                }
                continue;
            }
            Origin::User | Origin::Library => {
                if activate {
                    activate_user(env, entry, &cache.faces(&entry.path).unwrap_or_default())
                        .map(|_| ())
                } else {
                    deactivate_user(env, entry)
                }
            }
            Origin::Folder => {
                let k = path_key(&entry.path);
                if activate {
                    disabled_folder_fonts.remove(&k);
                } else {
                    disabled_folder_fonts.insert(k);
                }
                Ok(())
            }
            Origin::System => {
                if !system_unlocked || fams.iter().any(|f| is_essential(f)) {
                    out.locked += 1;
                    continue;
                }
                if let Some(op) = build_system_op(env, entry, activate) {
                    sys_pending.push((entry, op));
                }
                continue;
            }
        };
        match res {
            Ok(()) => out.changed += 1,
            Err(e) => out.errors.push(format!("{label}: {e}")),
        }
    }

    if !sys_pending.is_empty() {
        for (entry, _) in &sys_pending {
            if !activate && env.touch_windows {
                win::unload_font(&entry.path, false);
            }
        }
        let ops: Vec<system::SystemOp> = sys_pending.iter().map(|(_, op)| op.clone()).collect();
        apply_system_ops(env, &ops, &mut out);
    }

    if env.touch_windows && out.changed > 0 {
        win::broadcast_font_change();
    }
    out
}

fn is_in_system_backup(env: &Environment, entry: &FontEntry) -> bool {
    entry.path.starts_with(env.system_backup_dir())
}

fn build_system_op(
    env: &Environment,
    entry: &FontEntry,
    activate: bool,
) -> Option<system::SystemOp> {
    let (value, data) = entry.value.clone().unwrap_or_default();
    if activate {
        let saved = load_system_backups(env)
            .into_iter()
            .find(|s| path_key(&s.file) == path_key(&entry.path))?;
        Some(system::SystemOp::Activate {
            backup: saved.file,
            path: saved.original,
            value: saved.value,
            data: saved.data,
        })
    } else {
        Some(system::SystemOp::Deactivate {
            backup: unique_path(&env.system_backup_dir(), &filename(&entry.path)),
            path: entry.path.clone(),
            value,
            data,
        })
    }
}

pub fn apply_system_ops(env: &Environment, ops: &[system::SystemOp], out: &mut ToggleOutcome) {
    let app_before = load_app_installed(env);
    let mut recorded_win = load_recorded_windows_fonts(env);
    let origins: Vec<Origin> = ops
        .iter()
        .map(|op| match op {
            system::SystemOp::Deactivate { path, .. } => {
                origin_in_system_dir(env, path, &app_before, &recorded_win)
            }
            _ => Origin::System,
        })
        .collect();

    let resp = match system::request_elevated(env, ops) {
        Ok(r) => r,
        Err(e) => {
            if e == system::CANCELLED {
                out.cancelled = true;
            } else {
                out.errors.push(e);
            }
            for op in ops {
                if let system::SystemOp::Deactivate { path, .. } = op {
                    if env.touch_windows {
                        win::load_font(path, false);
                    }
                }
            }
            return;
        }
    };

    out.reboot_required |= resp.reboot_required;
    let mut backups = load_system_backups(env);
    let mut app_installed = load_app_installed(env);

    for ((op, err_opt), origin) in ops.iter().zip(resp.results.iter()).zip(origins) {
        if let Some(err_msg) = err_opt {
            let name = match op {
                system::SystemOp::Deactivate { path, .. }
                | system::SystemOp::Activate { path, .. }
                | system::SystemOp::Install { path, .. } => filename(path),
            };
            out.errors.push(format!("{name}: {err_msg}"));
            if let system::SystemOp::Deactivate { path, .. } = op {
                if env.touch_windows {
                    win::load_font(path, false);
                }
            }
            continue;
        }
        out.changed += 1;
        match op {
            system::SystemOp::Deactivate {
                path,
                backup,
                value,
                data,
            } => {
                if origin == Origin::System {
                    recorded_win.insert(path_key(path));
                }
                backups.push(SavedFont {
                    file: backup.clone(),
                    original: path.clone(),
                    value: value.clone(),
                    data: data.clone(),
                    origin,
                });
            }
            system::SystemOp::Activate { path, backup, .. } => {
                backups.retain(|s| path_key(&s.file) != path_key(backup));
                let _ = fs::remove_file(backup);
                let _ = fs::remove_file(backup.with_extension("reg.json"));
                if env.touch_windows {
                    win::load_font(path, false);
                }
            }
            system::SystemOp::Install { path, .. } => {
                app_installed.insert(path_key(path));
                if env.touch_windows {
                    win::load_font(path, false);
                }
            }
        }
    }

    if let Err(e) = save_recorded_windows_fonts(env, &recorded_win) {
        out.errors.push(e);
    }
    if let Err(e) = save_system_backups(env, &backups) {
        out.errors.push(e);
    }
    let _ = save_app_installed(env, &app_installed);
}
