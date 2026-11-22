use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use cosmic_foundry::compiler::{compile_ttf, export_ufo};
use cosmic_foundry::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphOutline,
};
use cosmic_foundry::styles::{generate_style_variant, StyleVariantKind};
use image::{GrayImage, ImageFormat, Luma};
use skrifa::FontRef;
use tempfile::tempdir;

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_cosmic-foundry")
}

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

fn make_sample_project(family: &str, thick: bool) -> FoundryProject {
    let mut glyphs = BTreeMap::new();
    glyphs.insert("space".to_string(), make_glyph(' ', 320, Vec::new()));

    let stem = if thick { 180.0 } else { 70.0 };
    let h_outer = poly(&[
        (80.0, 0.0),
        (80.0, 720.0),
        (80.0 + stem, 720.0),
        (80.0 + stem, 400.0),
        (540.0 - stem, 400.0),
        (540.0 - stem, 720.0),
        (540.0, 720.0),
        (540.0, 0.0),
        (540.0 - stem, 0.0),
        (540.0 - stem, 320.0),
        (80.0 + stem, 320.0),
        (80.0 + stem, 0.0),
    ]);
    glyphs.insert("H".to_string(), make_glyph('H', 620, vec![h_outer]));

    let o_outer = poly(&[(80.0, 0.0), (80.0, 720.0), (540.0, 720.0), (540.0, 0.0)]);
    let o_inner = poly(&[
        (80.0 + stem, stem),
        (540.0 - stem, stem),
        (540.0 - stem, 720.0 - stem),
        (80.0 + stem, 720.0 - stem),
    ]);
    glyphs.insert(
        "O".to_string(),
        make_glyph('O', 620, vec![o_outer, o_inner]),
    );

    let i_outer = poly(&[
        (180.0, 0.0),
        (180.0, 720.0),
        (180.0 + stem, 720.0),
        (180.0 + stem, 0.0),
    ]);
    glyphs.insert("I".to_string(), make_glyph('I', 400, vec![i_outer]));

    FoundryProject {
        version: 3,
        family_name: family.to_string(),
        style_name: "Regular".to_string(),
        designer: "CLI Test".to_string(),
        license: "SIL Open Font License 1.1".to_string(),
        metrics: make_metrics(),
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    }
}

fn write_svg_glyphs(dir: &Path) {
    let svg_a = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000">
        <path d="M 100 900 L 450 100 L 800 900 L 660 900 L 580 700 L 320 700 L 240 900 Z"/>
    </svg>"#;
    let svg_h = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000">
        <path d="M 120 100 L 260 100 L 260 440 L 640 440 L 640 100 L 780 100 L 780 900 L 640 900 L 640 560 L 260 560 L 260 900 L 120 900 Z"/>
    </svg>"#;
    fs::write(dir.join("A.svg"), svg_a).expect("write A.svg");
    fs::write(dir.join("H.svg"), svg_h).expect("write H.svg");
}

#[test]
fn help_exits_zero_and_lists_all_subcommands() {
    let out = Command::new(cli_bin())
        .arg("--help")
        .output()
        .expect("run --help");

    assert!(out.status.success(), "--help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for cmd in [
        "trace-sheet",
        "svg-dir",
        "qa",
        "autofix",
        "style",
        "dedup",
        "triage",
        "vibe",
    ] {
        assert!(
            stdout.contains(cmd),
            "--help output should mention '{cmd}', got:\n{stdout}"
        );
    }
}

#[test]
fn svg_dir_and_qa_clean_and_failing_behave_as_expected() {
    let tmp = tempdir().expect("tempdir");
    let svg_dir = tmp.path().join("svgs");
    fs::create_dir_all(&svg_dir).expect("create svg dir");
    write_svg_glyphs(&svg_dir);

    let compiled_ttf = tmp.path().join("out").join("svg_font.ttf");
    let svg_out = Command::new(cli_bin())
        .args([
            "svg-dir",
            svg_dir.to_str().expect("utf8 path"),
            "--out",
            compiled_ttf.to_str().expect("utf8 path"),
            "--family",
            "SvgCliTest",
            "--upm",
            "1000",
        ])
        .output()
        .expect("run svg-dir");

    assert!(
        svg_out.status.success(),
        "svg-dir failed: stderr={}",
        String::from_utf8_lossy(&svg_out.stderr)
    );
    let ttf_bytes = fs::read(&compiled_ttf).expect("read compiled ttf");
    assert!(!ttf_bytes.is_empty(), "compiled ttf should be non-empty");
    assert!(
        FontRef::from_index(&ttf_bytes, 0).is_ok(),
        "compiled ttf should parse as valid OpenType font"
    );

    // Clean QA pass
    let qa_clean = Command::new(cli_bin())
        .args(["qa", compiled_ttf.to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run qa clean");
    assert!(
        qa_clean.status.success(),
        "clean qa should exit 0: stderr={}",
        String::from_utf8_lossy(&qa_clean.stderr)
    );
    let clean_json: serde_json::Value =
        serde_json::from_slice(&qa_clean.stdout).expect("qa clean stdout should be valid JSON");
    assert!(clean_json.get("score").and_then(|v| v.as_u64()).is_some());
    assert!(clean_json
        .get("issues")
        .and_then(|v| v.as_array())
        .is_some());
    assert_eq!(clean_json.get("errors").and_then(|v| v.as_u64()), Some(0));

    // Deliberately broken UFO failing QA with errors > 0
    let mut broken = make_sample_project("BrokenFont", false);
    broken.metrics.descender = 200; // Positive descender triggers METRICS_INVERTED_DESCENDER (Error)
    if let Some(g) = broken.glyphs.get_mut("H") {
        g.advance_width = -50; // Negative advance triggers GLYPH_NEGATIVE_ADVANCE (Error)
    }
    let broken_ufo = tmp.path().join("broken.ufo");
    export_ufo(&broken, &broken_ufo).expect("export broken UFO");

    let qa_fail = Command::new(cli_bin())
        .args(["qa", broken_ufo.to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run qa failing");
    assert!(
        !qa_fail.status.success(),
        "failing qa must exit non-zero, got {:?}",
        qa_fail.status.code()
    );
    assert_eq!(qa_fail.status.code(), Some(2));
    let fail_json: serde_json::Value =
        serde_json::from_slice(&qa_fail.stdout).expect("qa fail stdout should be valid JSON");
    assert!(fail_json.get("score").and_then(|v| v.as_u64()).is_some());
    assert!(fail_json.get("issues").and_then(|v| v.as_array()).is_some());
    let err_count = fail_json
        .get("errors")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(err_count >= 2, "expected >= 2 QA errors, got {err_count}");

    // Verify autofix repairs the broken UFO and produces a valid TTF
    let fixed_ttf = tmp.path().join("fixed.ttf");
    let fix_out = Command::new(cli_bin())
        .args([
            "autofix",
            broken_ufo.to_str().expect("utf8 path"),
            "--out",
            fixed_ttf.to_str().expect("utf8 path"),
            "--json",
        ])
        .output()
        .expect("run autofix");
    assert!(
        fix_out.status.success(),
        "autofix failed: stderr={}",
        String::from_utf8_lossy(&fix_out.stderr)
    );
    let fix_json: serde_json::Value =
        serde_json::from_slice(&fix_out.stdout).expect("autofix stdout should be JSON");
    assert!(
        fix_json
            .get("fixed_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            >= 2
    );
    assert_eq!(
        fix_json
            .get("report")
            .and_then(|r| r.get("errors"))
            .and_then(|v| v.as_u64()),
        Some(0)
    );
    let fixed_bytes = fs::read(&fixed_ttf).expect("read fixed.ttf");
    assert!(FontRef::from_index(&fixed_bytes, 0).is_ok());

    // Verify style subcommand on the fixed TTF
    let bold_ttf = tmp.path().join("bold.ttf");
    let style_out = Command::new(cli_bin())
        .args([
            "style",
            fixed_ttf.to_str().expect("utf8 path"),
            "--kind",
            "bold",
            "--amount",
            "20",
            "--out",
            bold_ttf.to_str().expect("utf8 path"),
            "--json",
        ])
        .output()
        .expect("run style");
    assert!(
        style_out.status.success(),
        "style failed: stderr={}",
        String::from_utf8_lossy(&style_out.stderr)
    );
    let bold_bytes = fs::read(&bold_ttf).expect("read bold.ttf");
    assert!(FontRef::from_index(&bold_bytes, 0).is_ok());
}

#[test]
fn dedup_triage_and_vibe_json_work_end_to_end() {
    let tmp = tempdir().expect("tempdir");

    let regular = make_sample_project("RegularTone", false);
    let regular_ttf = compile_ttf(&regular, true)
        .expect("compile regular")
        .ttf_bytes;

    let bold_proj = generate_style_variant(
        &make_sample_project("HeavyTone", true),
        StyleVariantKind::Bold,
        30.0,
    );
    let bold_ttf = compile_ttf(&bold_proj, true)
        .expect("compile bold")
        .ttf_bytes;

    let a_path = tmp.path().join("RegularTone.ttf");
    let b_path = tmp.path().join("RenamedDuplicate.ttf");
    let c_path = tmp.path().join("HeavyBoldTone.ttf");
    fs::write(&a_path, &regular_ttf).expect("write a");
    fs::write(&b_path, &regular_ttf).expect("write b");
    fs::write(&c_path, &bold_ttf).expect("write c");
    fs::write(tmp.path().join("payload.exe"), b"MZ").expect("write unsafe file");

    // 1. dedup --json
    let dedup_out = Command::new(cli_bin())
        .args(["dedup", tmp.path().to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run dedup");
    assert!(
        dedup_out.status.success(),
        "dedup failed: stderr={}",
        String::from_utf8_lossy(&dedup_out.stderr)
    );
    let dedup_json: serde_json::Value =
        serde_json::from_slice(&dedup_out.stdout).expect("dedup stdout should be JSON");
    assert_eq!(
        dedup_json.get("group_count").and_then(|v| v.as_u64()),
        Some(1)
    );
    let files = dedup_json["groups"][0]["files"]
        .as_array()
        .expect("group files array");
    assert_eq!(files.len(), 2);

    // 2. triage --json
    let triage_out = Command::new(cli_bin())
        .args(["triage", tmp.path().to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run triage");
    assert!(
        triage_out.status.success(),
        "triage failed: stderr={}",
        String::from_utf8_lossy(&triage_out.stderr)
    );
    let triage_json: serde_json::Value =
        serde_json::from_slice(&triage_out.stdout).expect("triage stdout should be JSON");
    assert_eq!(
        triage_json.get("rejected_unsafe").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        triage_json.get("duplicate_count").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(triage_json.get("license_breakdown").is_some());

    // 3. vibe "heavy bold" --dir <TMP> --json
    let vibe_out = Command::new(cli_bin())
        .args([
            "vibe",
            "heavy bold",
            "--dir",
            tmp.path().to_str().expect("utf8 path"),
            "--top",
            "3",
            "--json",
        ])
        .output()
        .expect("run vibe");
    assert!(
        vibe_out.status.success(),
        "vibe failed: stderr={}",
        String::from_utf8_lossy(&vibe_out.stderr)
    );
    let vibe_json: serde_json::Value =
        serde_json::from_slice(&vibe_out.stdout).expect("vibe stdout should be JSON");
    let hits = vibe_json["hits"].as_array().expect("vibe hits array");
    assert!(!hits.is_empty(), "expected vibe hits");
    assert_eq!(
        hits[0]["family"].as_str(),
        Some("HeavyBoldTone"),
        "heavy bold font should rank first: {vibe_json}"
    );
}

#[test]
fn trace_sheet_compiles_valid_ttf() {
    let tmp = tempdir().expect("tempdir");
    let mut img = GrayImage::from_pixel(128, 128, Luma([255]));
    for (row, col) in [(0u32, 0u32), (0, 1), (1, 0), (1, 1)] {
        let ox = col * 64;
        let oy = row * 64;
        for y in (oy + 12)..(oy + 52) {
            for x in (ox + 16)..(ox + 48) {
                img.put_pixel(x, y, Luma([0]));
            }
        }
    }
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .expect("encode png");
    let sheet_path = tmp.path().join("sheet.png");
    fs::write(&sheet_path, &png_bytes).expect("write sheet.png");

    let out_ttf = tmp.path().join("traced.ttf");
    let out = Command::new(cli_bin())
        .args([
            "trace-sheet",
            sheet_path.to_str().expect("utf8 path"),
            "--cols",
            "2",
            "--rows",
            "2",
            "--chars",
            "ABCD",
            "--out",
            out_ttf.to_str().expect("utf8 path"),
            "--family",
            "SheetCliFont",
            "--threshold",
            "140",
            "--json",
        ])
        .output()
        .expect("run trace-sheet");

    assert!(
        out.status.success(),
        "trace-sheet failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ttf_bytes = fs::read(&out_ttf).expect("read traced.ttf");
    assert!(!ttf_bytes.is_empty());
    assert!(FontRef::from_index(&ttf_bytes, 0).is_ok());
}
