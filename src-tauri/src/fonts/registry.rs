//! Windows Font Registry helpers (`HKLM` system-wide, `HKCU` per-user).

use winreg::{enums::*, RegKey, HKEY};

pub const FONTS_KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";

pub fn hklm() -> HKEY {
    HKEY_LOCAL_MACHINE
}

pub fn hkcu() -> HKEY {
    HKEY_CURRENT_USER
}

pub fn read_values(root: HKEY, key: &str) -> Vec<(String, String)> {
    let Ok(k) = RegKey::predef(root).open_subkey_with_flags(key, KEY_READ) else {
        return vec![];
    };
    k.enum_values()
        .filter_map(Result::ok)
        .filter_map(|(name, val)| {
            let data: String = winreg::types::FromRegValue::from_reg_value(&val).ok()?;
            Some((name, data))
        })
        .collect()
}

pub fn set_value(root: HKEY, key: &str, name: &str, data: &str) -> Result<(), String> {
    let (k, _) = RegKey::predef(root)
        .create_subkey_with_flags(key, KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    k.set_value(name, &data).map_err(|e| e.to_string())
}

pub fn delete_value(root: HKEY, key: &str, name: &str) -> Result<(), String> {
    let k = RegKey::predef(root)
        .open_subkey_with_flags(key, KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    match k.delete_value(name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn format_value_name(full_names: &[String], is_cff: bool) -> String {
    let mut unique: Vec<&str> = Vec::new();
    for name in full_names {
        if !unique.contains(&name.as_str()) {
            unique.push(name);
        }
    }
    format!(
        "{} ({})",
        unique.join(" & "),
        if is_cff { "OpenType" } else { "TrueType" }
    )
}
