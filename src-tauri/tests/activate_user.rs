//! Integration tests for user font activation/deactivation using temporary directories and
//! isolated HKCU test registry keys (without modifying real Windows fonts).

use cosmic_fonts_lib::fonts::{
    self, cache::Cache, changes, load_deactivated, reader, registry, save_recorded_windows_fonts,
    Environment, Origin,
};
use std::{collections::HashSet, path::Path};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

struct RegCleanup(String);
impl Drop for RegCleanup {
    fn drop(&mut self) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&self.0);
        let _ = hkcu.delete_subkey(r"Software\cosmic-font-foundry-tests");
    }
}

#[test]
fn install_deactivate_and_activate_user_font() {
    let tmp = tempfile::tempdir().unwrap();
    let base = format!(r"Software\cosmic-font-foundry-tests\{}", std::process::id());
    let _cleanup = RegCleanup(base.clone());
    let env = Environment {
        system_fonts: tmp.path().join("system"),
        user_fonts: tmp.path().join("user"),
        system_key: format!(r"{base}\System"),
        user_key: format!(r"{base}\Fonts"),
        data_dir: tmp.path().join("data"),
        touch_windows: false,
        is_windows_owned: |_| true,
        app_bundled_fonts: HashSet::new(),
    };
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/GochiHand-Regular.ttf");

    let faces = reader::read_faces(&font).unwrap();
    assert_eq!(faces[0].family, "Gochi Hand");
    assert_eq!(faces[0].category, "script");
    assert!(!reader::characters(&font, 500).unwrap().is_empty());

    let installed = changes::install_user(&env, &font, &faces).unwrap();
    assert!(installed.is_file());
    let reg = registry::read_values(registry::hkcu(), &env.user_key);
    assert_eq!(reg.len(), 1);
    assert_eq!(reg[0].0, "Gochi Hand (TrueType)");
    assert_eq!(Path::new(&reg[0].1), installed);

    let mut cache = Cache::default();
    let entries = fonts::collect_entries(&env, &[]);
    cache.update(&entries);
    let fam = cache.families.iter().find(|f| f.name == "Gochi Hand").unwrap();
    assert!(fam.active);
    assert_eq!(fam.origin, Origin::Library);

    let mut disabled = HashSet::new();
    let outcome = changes::toggle_families(
        &env,
        &entries,
        &cache,
        &["Gochi Hand".into()],
        false,
        false,
        &mut disabled,
    );
    assert_eq!(outcome.changed, 1, "{:?}", outcome.errors);
    assert!(!installed.exists());
    assert!(registry::read_values(registry::hkcu(), &env.user_key).is_empty());
    let saved = load_deactivated(&env);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].file.starts_with(env.deactivated_dir()) && saved[0].file.is_file());

    let entries = fonts::collect_entries(&env, &[]);
    cache.update(&entries);
    assert!(!cache.families.iter().find(|f| f.name == "Gochi Hand").unwrap().active);

    let outcome = changes::toggle_families(
        &env,
        &entries,
        &cache,
        &["Gochi Hand".into()],
        false,
        false,
        &mut disabled,
    );
    assert_eq!(outcome.changed, 0);

    let outcome = changes::toggle_families(
        &env,
        &entries,
        &cache,
        &["Gochi Hand".into()],
        true,
        false,
        &mut disabled,
    );
    assert_eq!(outcome.changed, 1, "{:?}", outcome.errors);
    assert!(installed.is_file());
    let reg = registry::read_values(registry::hkcu(), &env.user_key);
    assert_eq!(
        reg,
        vec![(
            "Gochi Hand (TrueType)".to_string(),
            installed.to_string_lossy().into_owned()
        )]
    );
    assert!(load_deactivated(&env).is_empty());

    let entries = fonts::collect_entries(&env, &[]);
    cache.update(&entries);
    assert!(cache.families.iter().find(|f| f.name == "Gochi Hand").unwrap().active);
}

#[test]
fn essential_windows_fonts_are_protected() {
    for fam in [
        "Segoe UI",
        "Segoe UI Variable",
        "Segoe UI Emoji",
        "Segoe Fluent Icons",
        "Segoe MDL2 Assets",
        "Marlett",
        "Tahoma",
        "Microsoft Sans Serif",
        "Arial",
        "Consolas",
    ] {
        assert!(fonts::is_essential(fam), "{fam}");
    }
    for fam in ["Arial Narrow", "Georgia", "Times New Roman", "Segoe Print"] {
        assert!(!fonts::is_essential(fam), "{fam}");
    }
}

#[test]
fn manual_all_users_fonts_are_classified_as_user_origin() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Environment {
        system_fonts: tmp.path().join("system"),
        user_fonts: tmp.path().join("user"),
        system_key: r"Software\cosmic-font-foundry-tests\nonexistent".into(),
        user_key: r"Software\cosmic-font-foundry-tests\nonexistent".into(),
        data_dir: tmp.path().join("data"),
        touch_windows: false,
        is_windows_owned: |p| p.file_name().is_some_and(|n| n == "win-font.ttf"),
        app_bundled_fonts: ["office-font.ttf".to_string()].into(),
    };
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/GochiHand-Regular.ttf");
    std::fs::create_dir_all(&env.system_fonts).unwrap();
    std::fs::copy(&font, env.system_fonts.join("win-font.ttf")).unwrap();
    std::fs::copy(&font, env.system_fonts.join("manual-font.ttf")).unwrap();
    std::fs::copy(&font, env.system_fonts.join("OFFICE-FONT.TTF")).unwrap();

    let origin_of = |name: &str| {
        fonts::collect_entries(&env, &[])
            .into_iter()
            .find(|e| e.path.file_name().unwrap() == name)
            .unwrap()
            .origin
    };
    assert_eq!(origin_of("win-font.ttf"), Origin::System);
    assert_eq!(origin_of("manual-font.ttf"), Origin::User);
    assert_eq!(origin_of("OFFICE-FONT.TTF"), Origin::System);

    let mut recorded = HashSet::new();
    recorded.insert(fonts::path_key(&env.system_fonts.join("manual-font.ttf")));
    save_recorded_windows_fonts(&env, &recorded).unwrap();
    assert_eq!(origin_of("manual-font.ttf"), Origin::System);
}

#[test]
fn arial_is_owned_by_trusted_installer() {
    let windir = std::env::var_os("WINDIR").unwrap();
    let arial = Path::new(&windir).join("Fonts").join("arial.ttf");
    assert_eq!(cosmic_fonts_lib::win::is_trusted_installer(&arial), Some(true));
}
