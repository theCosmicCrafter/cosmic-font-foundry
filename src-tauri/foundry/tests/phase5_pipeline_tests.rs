use std::collections::BTreeMap;
use std::fs;

use cosmic_foundry::compiler::compile_ttf;
use cosmic_foundry::ingest::{classify_license, import_svg_folder, triage_path, LicenseClass};
use cosmic_foundry::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphOutline,
};
use cosmic_foundry::outline_hash::{
    find_outline_duplicates, fingerprint_font_bytes, fingerprint_font_file,
};
use cosmic_foundry::styles::{generate_style_variant, StyleVariantKind};
use cosmic_foundry::vibe::{extract_dna_from_bytes, search_fonts_by_vibe};
use tempfile::tempdir;

fn make_metrics() -> FontMetrics {
    FontMetrics {
        upm: 1000,
        ascender: 800,
        descender: -200,
        x_height: 520,
        cap_height: 720,
        line_gap: 0,
    }
}

fn poly(pts: &[(f64, f64)]) -> Contour {
    Contour {
        anchors: pts.iter().map(|&(x, y)| Anchor::on_curve(x, y)).collect(),
        closed: true,
    }
}

fn make_glyph(ch: char, advance_width: i32, contours: Vec<Contour>) -> GlyphOutline {
    let lsb = if contours.is_empty() {
        0
    } else {
        compute_bbox(&contours)[0].round() as i32
    };
    GlyphOutline {
        codepoint: ch as u32,
        name: ch.to_string(),
        advance_width,
        lsb,
        contours,
    }
}

fn make_a_contours() -> Vec<Contour> {
    let outer = poly(&[
        (40.0, 0.0),
        (260.0, 720.0),
        (360.0, 720.0),
        (580.0, 0.0),
        (460.0, 0.0),
        (405.0, 200.0),
        (215.0, 200.0),
        (160.0, 0.0),
    ]);
    let inner = poly(&[(245.0, 300.0), (375.0, 300.0), (310.0, 560.0)]);
    vec![outer, inner]
}

fn make_h_contours() -> Vec<Contour> {
    // True H polygon with left stem, crossbar, and right stem (80-unit strokes)
    vec![poly(&[
        (80.0, 0.0),
        (80.0, 720.0),
        (170.0, 720.0),
        (170.0, 405.0),
        (450.0, 405.0),
        (450.0, 720.0),
        (540.0, 720.0),
        (540.0, 0.0),
        (450.0, 0.0),
        (450.0, 315.0),
        (170.0, 315.0),
        (170.0, 0.0),
    ])]
}

fn make_o_contours() -> Vec<Contour> {
    let outer = Contour {
        anchors: vec![
            Anchor {
                x: 310.0,
                y: 720.0,
                h_in: [140.0, 0.0],
                h_out: [-140.0, 0.0],
                smooth: true,
            },
            Anchor {
                x: 60.0,
                y: 360.0,
                h_in: [0.0, 200.0],
                h_out: [0.0, -200.0],
                smooth: true,
            },
            Anchor {
                x: 310.0,
                y: 0.0,
                h_in: [-140.0, 0.0],
                h_out: [140.0, 0.0],
                smooth: true,
            },
            Anchor {
                x: 560.0,
                y: 360.0,
                h_in: [0.0, -200.0],
                h_out: [0.0, 200.0],
                smooth: true,
            },
        ],
        closed: true,
    };
    let inner = Contour {
        anchors: vec![
            Anchor {
                x: 310.0,
                y: 620.0,
                h_in: [-90.0, 0.0],
                h_out: [90.0, 0.0],
                smooth: true,
            },
            Anchor {
                x: 460.0,
                y: 360.0,
                h_in: [0.0, 140.0],
                h_out: [0.0, -140.0],
                smooth: true,
            },
            Anchor {
                x: 310.0,
                y: 100.0,
                h_in: [90.0, 0.0],
                h_out: [-90.0, 0.0],
                smooth: true,
            },
            Anchor {
                x: 160.0,
                y: 360.0,
                h_in: [0.0, -140.0],
                h_out: [0.0, 140.0],
                smooth: true,
            },
        ],
        closed: true,
    };
    vec![outer, inner]
}

fn make_base_project(family_name: &str) -> FoundryProject {
    let mut glyphs = BTreeMap::new();
    glyphs.insert(
        "space".to_string(),
        GlyphOutline {
            codepoint: 0x20,
            name: "space".to_string(),
            advance_width: 280,
            lsb: 0,
            contours: Vec::new(),
        },
    );
    glyphs.insert("A".to_string(), make_glyph('A', 620, make_a_contours()));
    glyphs.insert("H".to_string(), make_glyph('H', 620, make_h_contours()));
    glyphs.insert("O".to_string(), make_glyph('O', 620, make_o_contours()));

    FoundryProject {
        version: 3,
        family_name: family_name.to_string(),
        style_name: "Regular".to_string(),
        designer: "Cosmic Foundry".to_string(),
        license: "MIT License".to_string(),
        metrics: make_metrics(),
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    }
}

fn build_bold_and_light_ttfs() -> (Vec<u8>, Vec<u8>) {
    let base = make_base_project("VibeTest");
    let bold = generate_style_variant(&base, StyleVariantKind::Bold, 32.0);
    let light = generate_style_variant(&base, StyleVariantKind::Light, 16.0);
    let bold_ttf = compile_ttf(&bold, false)
        .expect("compile bold TTF")
        .ttf_bytes;
    let light_ttf = compile_ttf(&light, false)
        .expect("compile light TTF")
        .ttf_bytes;
    (bold_ttf, light_ttf)
}

#[test]
fn test1_canonical_outline_hash_matches_renamed_fonts_and_detects_mutation() {
    let dir = tempdir().expect("tempdir");

    let proj_alpha = make_base_project("FontAlpha");
    let mut proj_beta = make_base_project("FontBeta");
    proj_beta.designer = "Different Designer".to_string();
    proj_beta.license = "SIL Open Font License 1.1".to_string();

    let bytes_alpha = compile_ttf(&proj_alpha, false)
        .expect("compile FontAlpha")
        .ttf_bytes;
    let bytes_beta = compile_ttf(&proj_beta, false)
        .expect("compile FontBeta")
        .ttf_bytes;
    assert_ne!(
        bytes_alpha, bytes_beta,
        "metadata differences must yield different raw bytes"
    );

    let fp_alpha = fingerprint_font_bytes(&bytes_alpha, "FontAlpha.ttf").expect("fp alpha");
    let fp_beta = fingerprint_font_bytes(&bytes_beta, "FontBeta.ttf").expect("fp beta");

    assert_eq!(fp_alpha.digest.len(), 32);
    assert_eq!(fp_alpha.family_hint, "FontAlpha");
    assert_eq!(fp_beta.family_hint, "FontBeta");
    assert_eq!(
        fp_alpha.digest, fp_beta.digest,
        "identical outlines under different family names must share digest"
    );

    let mut proj_mutated = make_base_project("FontMutated");
    if let Some(h_glyph) = proj_mutated.glyphs.get_mut("H") {
        h_glyph.contours[0].anchors[1].y = 660.0;
    }
    let bytes_mutated = compile_ttf(&proj_mutated, false)
        .expect("compile mutated")
        .ttf_bytes;
    let fp_mutated = fingerprint_font_bytes(&bytes_mutated, "FontMutated.ttf").expect("fp mutated");
    assert_ne!(
        fp_alpha.digest, fp_mutated.digest,
        "mutated contour must produce distinct digest"
    );

    let p_alpha = dir.path().join("FontAlpha.ttf");
    let p_beta = dir.path().join("FontBeta.ttf");
    let p_mut = dir.path().join("FontMutated.ttf");
    fs::write(&p_alpha, &bytes_alpha).expect("write alpha");
    fs::write(&p_beta, &bytes_beta).expect("write beta");
    fs::write(&p_mut, &bytes_mutated).expect("write mutated");

    let fp_file = fingerprint_font_file(&p_alpha).expect("fingerprint file");
    assert_eq!(fp_file.digest, fp_alpha.digest);

    let groups = find_outline_duplicates(&[p_alpha, p_beta, p_mut]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].digest, fp_alpha.digest);
    assert_eq!(groups[0].files.len(), 2);
}

#[test]
fn test2_classify_license_covers_all_seven_classes() {
    assert_eq!(
        classify_license("Licensed under the SIL Open Font License, Version 1.1"),
        LicenseClass::Ofl
    );
    assert_eq!(
        classify_license("Apache License, Version 2.0"),
        LicenseClass::Apache
    );
    assert_eq!(
        classify_license("Permission is hereby granted, free of charge, to any person"),
        LicenseClass::Mit
    );
    assert_eq!(
        classify_license("MIT License - Copyright 2026"),
        LicenseClass::Mit
    );
    assert_eq!(
        classify_license("CC0 1.0 Universal Public Domain Dedication"),
        LicenseClass::PublicDomain
    );
    assert_eq!(
        classify_license("Free for Personal Use Only - Demo Only"),
        LicenseClass::PersonalOnly
    );
    assert_eq!(
        classify_license("Copyright (c) 2026 Foundry Inc. All Rights Reserved."),
        LicenseClass::Commercial
    );
    assert_eq!(classify_license(""), LicenseClass::Unknown);
    assert_eq!(classify_license("   "), LicenseClass::Unknown);
}

#[test]
fn test3_import_svg_folder_builds_compilable_project() {
    let dir = tempdir().expect("tempdir");

    let svg_a = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <path d="M 10 90 L 50 10 L 90 90 L 70 90 L 50 45 L 30 90 Z" />
    </svg>"#;
    let svg_b = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <path d="M 15 10 H 80 V 90 H 15 Z" />
    </svg>"#;

    fs::write(dir.path().join("A.svg"), svg_a).expect("write A.svg");
    fs::write(dir.path().join("U+0042.svg"), svg_b).expect("write U+0042.svg");

    let project = import_svg_folder(dir.path(), "SvgBatchTest", 1000).expect("import_svg_folder");
    assert_eq!(project.family_name, "SvgBatchTest");
    assert!(project.glyphs.values().any(|g| g.codepoint == 'A' as u32));
    assert!(project.glyphs.values().any(|g| g.codepoint == 'B' as u32));
    assert!(project.glyphs.values().any(|g| g.codepoint == 0x20));

    let compiled = compile_ttf(&project, false).expect("compile imported SVG project");
    assert!(!compiled.ttf_bytes.is_empty());

    let fp = fingerprint_font_bytes(&compiled.ttf_bytes, "SvgBatchTest.ttf")
        .expect("fallback charmap fingerprint succeeds with 2 non-probe/probe glyphs");
    assert_eq!(fp.glyphs_hashed, 2);
}

#[test]
fn test4_triage_path_reports_duplicates_and_unsafe_files() {
    let dir = tempdir().expect("tempdir");
    let proj_a = make_base_project("TriageOne");
    let proj_b = make_base_project("TriageTwo");

    let ttf_a = compile_ttf(&proj_a, false).expect("compile a").ttf_bytes;
    let ttf_b = compile_ttf(&proj_b, false).expect("compile b").ttf_bytes;

    fs::write(dir.path().join("one.ttf"), &ttf_a).expect("write one.ttf");
    fs::write(dir.path().join("two.ttf"), &ttf_b).expect("write two.ttf");
    fs::write(dir.path().join("payload.exe"), b"MZ_DUMMY").expect("write payload.exe");

    let report = triage_path(dir.path()).expect("triage directory");
    assert_eq!(report.scanned_files, 3);
    assert!(report.rejected_unsafe >= 1, "expected rejected unsafe >= 1");
    assert!(report.duplicate_count >= 1, "expected duplicate_count >= 1");
    assert_eq!(report.fonts.len(), 2);
    assert!(report.fonts.iter().any(|f| f.is_duplicate));
    assert!(report
        .fonts
        .iter()
        .all(|f| f.license_class == LicenseClass::Mit));
}

#[test]
fn test5_vibe_dna_and_search_ranks_bold_above_light() {
    let dir = tempdir().expect("tempdir");
    let (bold_ttf, light_ttf) = build_bold_and_light_ttfs();

    let dna_bold = extract_dna_from_bytes(&bold_ttf).expect("extract bold DNA");
    let dna_light = extract_dna_from_bytes(&light_ttf).expect("extract light DNA");

    assert!(
        dna_bold.weight > dna_light.weight,
        "expected dna_bold.weight ({}) > dna_light.weight ({})",
        dna_bold.weight,
        dna_light.weight
    );

    let bold_path = dir.path().join("AlphaDisplay.ttf");
    let light_path = dir.path().join("BetaDisplay.ttf");
    fs::write(&bold_path, &bold_ttf).expect("write bold");
    fs::write(&light_path, &light_ttf).expect("write light");

    let candidates = vec![
        (
            "BetaDisplay".to_string(),
            light_path.to_string_lossy().into_owned(),
            "sans".to_string(),
        ),
        (
            "AlphaDisplay".to_string(),
            bold_path.to_string_lossy().into_owned(),
            "sans".to_string(),
        ),
    ];

    let hits = search_fonts_by_vibe("chunky heavy bold", &candidates, 5);
    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0].family, "AlphaDisplay",
        "expected Bold font ranked #1, got hits: {hits:?}"
    );
    assert!(hits[0].score > hits[1].score);
    assert!(!hits[0].matched_traits.is_empty());
}
