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
    let fam = cache
        .families
        .iter()
        .find(|f| f.name == "Gochi Hand")
        .unwrap();
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
    assert!(
        !cache
            .families
            .iter()
            .find(|f| f.name == "Gochi Hand")
            .unwrap()
            .active
    );

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
    assert!(
        cache
            .families
            .iter()
            .find(|f| f.name == "Gochi Hand")
            .unwrap()
            .active
    );
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
    assert_eq!(
        cosmic_fonts_lib::win::is_trusted_installer(&arial),
        Some(true)
    );
}

#[test]
fn vault_imports_dedup_and_reports_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("scattered");
    let nested = src_dir.join("nested");
    let vault_dir = tmp.path().join("Vault");
    std::fs::create_dir_all(&nested).unwrap();

    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/GochiHand-Regular.ttf");
    std::fs::copy(&fixture, src_dir.join("GochiHand-Regular.ttf")).unwrap();
    std::fs::copy(&fixture, nested.join("GochiHand-copy.ttf")).unwrap();

    let report = cosmic_fonts_lib::vault::import_to_vault(&vault_dir, &[src_dir], false).unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped_duplicates, 1);
    assert!(report.bytes_copied > 0);
    assert!(report.errors.is_empty());

    let stats = cosmic_fonts_lib::vault::compute_vault_stats(&vault_dir);
    assert!(stats.exists);
    assert_eq!(stats.total_files, 1);
    assert_eq!(stats.total_families, 1);
    assert_eq!(stats.total_bytes, report.bytes_copied);
}

#[test]
fn install_user_guards_foreign_collisions_and_hash_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let base = format!(
        r"Software\cosmic-font-foundry-tests\{}-guard",
        std::process::id()
    );
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

    // Pre-existing different file with same name should block overwrite.
    std::fs::create_dir_all(&env.user_fonts).unwrap();
    let target = env.user_fonts.join("GochiHand-Regular.ttf");
    std::fs::write(&target, b"different-foreign-font").unwrap();
    let err = changes::install_user(&env, &font, &faces).unwrap_err();
    assert!(err.contains("different font file already exists"));

    // Clean install succeeds, then external modification blocks deactivate_user.
    std::fs::remove_file(&target).unwrap();
    let installed = changes::install_user(&env, &font, &faces).unwrap();
    assert_eq!(installed, target);
    std::fs::write(&installed, b"tampered").unwrap();

    let entries = fonts::collect_entries(&env, &[]);
    let entry = entries
        .iter()
        .find(|e| fonts::path_key(&e.path) == fonts::path_key(&installed))
        .unwrap();
    let deact_err = changes::deactivate_user(&env, entry).unwrap_err();
    assert!(deact_err.contains("changed externally"));
}

#[test]
fn vault_cleanup_protects_system_and_recycles_verified_duplicates() {
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let env = Environment {
        system_fonts: tmp.path().join("system"),
        user_fonts: tmp.path().join("user"),
        system_key: r"Software\cosmic-font-foundry-tests\unused-sys".into(),
        user_key: r"Software\cosmic-font-foundry-tests\unused-usr".into(),
        data_dir: tmp.path().join("data"),
        touch_windows: false,
        is_windows_owned: |_| true,
        app_bundled_fonts: HashSet::new(),
    };
    let vault_dir = tmp.path().join("Vault");
    let downloads = tmp.path().join("Downloads");
    let project_dir = downloads.join("ClientProject");
    std::fs::create_dir_all(&env.user_fonts).unwrap();
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join(".cosmicfonts.json"), "{}").unwrap();

    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/GochiHand-Regular.ttf");
    let loose_dup = downloads.join("GochiHand-Regular.ttf");
    let project_font = project_dir.join("GochiHand-Regular.ttf");
    let user_font = env.user_fonts.join("GochiHand-Regular.ttf");
    std::fs::copy(&fixture, &loose_dup).unwrap();
    std::fs::copy(&fixture, &project_font).unwrap();
    std::fs::copy(&fixture, &user_font).unwrap();

    // Import loose_dup into Vault (copy only first).
    cosmic_fonts_lib::vault::import_to_vault(&vault_dir, std::slice::from_ref(&loose_dup), false)
        .unwrap();

    // Cleanup scan should find loose_dup, but skip project_font (.cosmicfonts.json) and user_font.
    let scan = cosmic_fonts_lib::vault::scan_vault_duplicates(
        &vault_dir,
        &[downloads.clone(), env.user_fonts.clone()],
        Some(&env),
    );
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(Path::new(&scan.candidates[0].source_path), loose_dup);

    // Recycling both loose_dup and user_font should recycle loose_dup and skip user_font as protected.
    let recycled_paths = std::sync::Mutex::new(Vec::new());
    let report = cosmic_fonts_lib::vault::recycle_vault_duplicates(
        &vault_dir,
        &[loose_dup.clone(), user_font.clone()],
        Some(&env),
        &|p| {
            recycled_paths.lock().unwrap().push(p.to_path_buf());
            std::fs::remove_file(p).map_err(|e| e.to_string())
        },
    );
    assert_eq!(report.recycled, 1);
    assert_eq!(report.skipped_protected, 1);
    assert_eq!(*recycled_paths.lock().unwrap(), vec![loose_dup]);
    assert!(user_font.is_file());
    assert!(project_font.is_file());
}
