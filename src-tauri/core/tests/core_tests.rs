use cosmic_core::{
    cache::Cache,
    inventory::{group_families, is_essential, summarize_diagnostics, FontEntry, Origin},
    migrate::{detect_tuna_fonts, import_from_tuna_fonts},
    reader::{characters, read_file, FileReadResult, FontFace},
    sync::{
        export_profile_json, import_profile_json, package_collection_zip, resolve_manifest_ref,
        scan_project_manifests, sync_manifest_folder, LibraryState, LocalCollection,
        ManifestFontRef, ZipBuilder,
    },
};
use flate2::read::DeflateDecoder;
use serde_json::json;
use std::{collections::BTreeMap, fs, io::Read, path::Path};

fn fixture_font() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("GochiHand-Regular.ttf")
}

#[test]
fn reads_font_metadata_scripts_and_sha256() {
    let path = fixture_font();
    let res = read_file(&path).expect("fixture font should parse");
    assert_eq!(res.sha256.len(), 64);
    assert_eq!(res.faces.len(), 1);
    let face = &res.faces[0];
    assert_eq!(face.family, "Gochi Hand");
    assert_eq!(face.category, "script");
    assert!(!face.postscript_name.is_empty());
    assert!(face.scripts.contains(&"Latin".to_string()));
    let cps = characters(&path, 500).expect("cmap should parse");
    assert!(!cps.is_empty());
}

#[test]
fn detects_essential_windows_fonts() {
    for fam in [
        "Segoe UI",
        "Segoe UI Variable",
        "Segoe Fluent Icons",
        "Segoe MDL2 Assets",
        "Marlett",
        "Tahoma",
        "Microsoft Sans Serif",
        "Arial",
        "Consolas",
    ] {
        assert!(is_essential(fam), "{fam} should be essential");
    }
    assert!(!is_essential("Gochi Hand"));
    assert!(!is_essential("Arial Narrow"));
}

#[test]
fn detects_exact_duplicates_conflicts_and_corrupt_files() {
    let tmp = tempfile::tempdir().unwrap();
    let src = fixture_font();
    let p1 = tmp.path().join("GochiHand-1.ttf");
    let p2 = tmp.path().join("GochiHand-2.ttf");
    let p_bad = tmp.path().join("broken.ttf");
    fs::copy(&src, &p1).unwrap();
    fs::copy(&src, &p2).unwrap();
    fs::write(&p_bad, b"not a valid font file").unwrap();

    let entries = vec![
        FontEntry {
            path: p1.clone(),
            origin: Origin::Library,
            active: true,
            value: None,
        },
        FontEntry {
            path: p2.clone(),
            origin: Origin::Folder,
            active: true,
            value: None,
        },
        FontEntry {
            path: p_bad.clone(),
            origin: Origin::Folder,
            active: true,
            value: None,
        },
    ];

    let mut cache = Cache::default();
    let parsed = cache.update(&entries);
    assert_eq!(parsed, 3);
    // Second run with unchanged mtime/size should skip parsing.
    assert_eq!(cache.update(&entries), 0);

    let diag = summarize_diagnostics(&cache.families);
    assert_eq!(diag.corrupt_files, 1);
    assert_eq!(diag.exact_duplicates, 1);

    let gochi = cache
        .families
        .iter()
        .find(|f| f.name == "Gochi Hand")
        .unwrap();
    assert!(gochi.has_conflicts);
    assert_eq!(gochi.duplicates.len(), 1);
    assert_eq!(gochi.duplicates[0].kind, "exact");

    // Synthetic style conflict (same family/style with different SHA-256).
    let p3 = tmp.path().join("GochiHand-alt.ttf");
    let conflict_entries = vec![
        FontEntry {
            path: p1,
            origin: Origin::Library,
            active: true,
            value: None,
        },
        FontEntry {
            path: p3.clone(),
            origin: Origin::User,
            active: true,
            value: None,
        },
    ];
    let grouped = group_families(&conflict_entries, &|p| {
        if p == p3 {
            Ok(FileReadResult {
                sha256: "deadbeef".repeat(8),
                faces: vec![FontFace {
                    family: "Gochi Hand".into(),
                    win_family: "Gochi Hand".into(),
                    style: "Regular".into(),
                    full_name: "Gochi Hand Regular".into(),
                    postscript_name: "GochiHand-Regular".into(),
                    weight: 400,
                    italic: false,
                    category: "script".into(),
                    scripts: vec!["Latin".into()],
                    axes: vec![],
                    opentype_features: vec!["liga".into()],
                    foundry: Some("Test Foundry".into()),
                    designer: None,
                    license: Some("OFL".into()),
                    license_url: None,
                }],
            })
        } else {
            read_file(p)
        }
    });
    let diag2 = summarize_diagnostics(&grouped);
    assert_eq!(diag2.style_conflicts, 1);
    assert_eq!(grouped[0].duplicates[0].kind, "conflict");
}

#[test]
fn sync_manifest_resolves_and_packages_collection_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let src = fixture_font();
    let p1 = tmp.path().join("GochiHand-Regular.ttf");
    fs::copy(&src, &p1).unwrap();

    let mut cache = Cache::default();
    cache.update(&[FontEntry {
        path: p1,
        origin: Origin::Library,
        active: true,
        value: None,
    }]);
    let fam = &cache.families[0];

    // Resolution priority: SHA-256 -> PostScript -> family name.
    let by_hash = ManifestFontRef {
        family: "Wrong Name".into(),
        postscript_name: "Wrong-PS".into(),
        sha256: fam.sha256.clone(),
    };
    assert_eq!(
        resolve_manifest_ref(&by_hash, &cache.families).as_deref(),
        Some("Gochi Hand")
    );

    let by_ps = ManifestFontRef {
        family: "Renamed".into(),
        postscript_name: fam.postscript_name.clone(),
        sha256: "0".repeat(64),
    };
    assert_eq!(
        resolve_manifest_ref(&by_ps, &cache.families).as_deref(),
        Some("Gochi Hand")
    );

    // Push & merge BYO-Sync manifest.
    let sync_dir = tmp.path().join("sync");
    let mut tags = BTreeMap::new();
    tags.insert(
        "gochi hand".into(),
        vec!["handwriting".into(), "display".into()],
    );
    let local = LibraryState {
        favorites: vec!["Gochi Hand".into()],
        collections: vec![LocalCollection {
            id: "col-1".into(),
            name: "Brand Kit".into(),
            fonts: vec!["Gochi Hand".into()],
            pinned: true,
        }],
        tags,
    };

    let res = sync_manifest_folder(&sync_dir, &local, &cache.families, "WORKSTATION-A", true)
        .expect("sync push should succeed");
    assert!(sync_dir.join("cosmic-manifest.json").is_file());
    assert_eq!(res.library.favorites, vec!["Gochi Hand"]);

    // Import onto an empty machine profile.
    let bundle_path = tmp.path().join("profile.json");
    export_profile_json(&bundle_path, &local, &cache.families, "WORKSTATION-A").unwrap();
    let imported =
        import_profile_json(&bundle_path, &LibraryState::default(), &cache.families).unwrap();
    assert_eq!(imported.library.favorites, vec!["Gochi Hand"]);
    assert_eq!(imported.library.collections.len(), 1);
    assert!(imported.library.collections[0].pinned);

    // Package collection ZIP and verify PKZIP contents + Deflate stream.
    let zip_path = tmp.path().join("BrandKit.zip");
    let pkg = package_collection_zip(
        &zip_path,
        "Brand Kit",
        &["Gochi Hand".into(), "Missing Font".into()],
        &cache.families,
    )
    .expect("zip packaging should succeed");
    assert_eq!(pkg.font_files, 1);
    assert_eq!(pkg.families_included, 1);
    assert_eq!(pkg.missing_families, vec!["Missing Font"]);

    let zip_bytes = fs::read(&zip_path).unwrap();
    assert_eq!(&zip_bytes[0..4], b"PK\x03\x04");
    assert!(zip_bytes.windows(12).any(|w| w == b"LICENSES.txt"));
    assert!(zip_bytes
        .windows(27)
        .any(|w| w == b"fonts/GochiHand-Regular.ttf"));
    let extracted =
        cosmic_core::sync::extract_zip_fonts(&zip_bytes).expect("extract_zip_fonts should succeed");
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].0, "GochiHand-Regular.ttf");
    assert!(!extracted[0].1.is_empty());

    // Verify ZIP round-trip decompression on a minimal synthetic ZipBuilder payload.
    let mut zb = ZipBuilder::new();
    let payload = b"Cosmic Font Foundry license text";
    zb.add_file("LICENSES.txt", payload).unwrap();
    let raw = zb.finish().unwrap();
    let name_len = u16::from_le_bytes([raw[26], raw[27]]) as usize;
    let comp_len = u32::from_le_bytes([raw[18], raw[19], raw[20], raw[21]]) as usize;
    let data_start = 30 + name_len;
    let mut dec = DeflateDecoder::new(&raw[data_start..data_start + comp_len]);
    let mut unpacked = Vec::new();
    dec.read_to_end(&mut unpacked).unwrap();
    assert_eq!(unpacked.as_slice(), payload);
}

#[test]
fn detects_project_manifest_and_migrates_tuna_fonts() {
    let tmp = tempfile::tempdir().unwrap();
    let proj_dir = tmp.path().join("AcmeBrand");
    fs::create_dir_all(&proj_dir).unwrap();
    fs::write(
        proj_dir.join(".cosmicfonts.json"),
        serde_json::to_vec_pretty(&json!({
            "name": "Acme Brand",
            "fonts": ["Gochi Hand"],
            "auto_activate": true
        }))
        .unwrap(),
    )
    .unwrap();
    let detected = scan_project_manifests(&[proj_dir]);
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].name, "Acme Brand");
    assert!(detected[0].auto_activate);

    // Mock tuna-fonts AppData folder.
    let appdata = tmp.path().join("appdata");
    let tuna_dir = appdata.join("es.atunfrito.tunafonts");
    let cosmic_dir = appdata.join("com.thecosmiccrafter.cosmicfontfoundry");
    fs::create_dir_all(tuna_dir.join("desactivadas")).unwrap();
    let deact_src = tuna_dir.join("desactivadas").join("GochiHand-Regular.ttf");
    fs::copy(fixture_font(), &deact_src).unwrap();

    fs::write(
        tuna_dir.join("biblioteca.json"),
        serde_json::to_vec(&json!({
            "favoritas": ["Gochi Hand"],
            "colecciones": [{ "id": "c1", "nombre": "Editorial", "fuentes": ["Gochi Hand"] }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        tuna_dir.join("carpetas.json"),
        serde_json::to_vec(&json!(["C:\\Fonts\\Studio"])).unwrap(),
    )
    .unwrap();
    fs::write(
        tuna_dir.join("desactivadas.json"),
        serde_json::to_vec(&json!([{
            "archivo": deact_src,
            "original": "C:\\Users\\Demo\\AppData\\Local\\Microsoft\\Windows\\Fonts\\GochiHand-Regular.ttf",
            "valor": "Gochi Hand (TrueType)",
            "datos": "C:\\Users\\Demo\\AppData\\Local\\Microsoft\\Windows\\Fonts\\GochiHand-Regular.ttf",
            "origen": "bib"
        }]))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        tuna_dir.join("ajustes.json"),
        serde_json::to_vec(&json!({
            "ajustes": { "tema": "dark", "idioma": "en", "vista": "cuadricula", "tamano": 32 }
        }))
        .unwrap(),
    )
    .unwrap();

    let status = detect_tuna_fonts(&tuna_dir);
    assert!(status.available);
    assert_eq!(status.favorites_count, 1);
    assert_eq!(status.collections_count, 1);
    assert_eq!(status.folders_count, 1);
    assert_eq!(status.deactivated_count, 1);

    let (report, lib, folders) = import_from_tuna_fonts(&tuna_dir, &cosmic_dir).unwrap();
    assert_eq!(report.imported_favorites, 1);
    assert_eq!(report.imported_collections, 1);
    assert_eq!(report.imported_folders, 1);
    assert_eq!(report.copied_deactivated_files, 1);
    assert!(report.imported_settings);
    assert_eq!(lib.favorites, vec!["Gochi Hand"]);
    assert_eq!(folders.len(), 1);
    assert!(cosmic_dir
        .join("deactivated")
        .join("GochiHand-Regular.ttf")
        .is_file());
    // Original tuna-fonts directory remains intact.
    assert!(deact_src.is_file());
}
