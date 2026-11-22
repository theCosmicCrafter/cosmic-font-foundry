use std::collections::BTreeMap;
use std::path::PathBuf;

use cosmic_foundry::compiler::compile_ttf;
use cosmic_foundry::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphOutline, KerningPair,
    LigatureRule,
};
use cosmic_foundry::qa::{audit_project, autofix_project};
use cosmic_foundry::retype::match_outline_against_fonts;
use cosmic_foundry::styles::{generate_style_variant, StyleVariantKind};
use cosmic_foundry::variable::{compile_variable_ttf, VariableMaster};
use kurbo::{BezPath, Shape};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider, Tag};
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

fn rect_cw(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
    poly(&[(x0, y0), (x0, y1), (x1, y1), (x1, y0)])
}

fn rect_ccw(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
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
    // Distinct triangular 'A' with inner triangular counter hole.
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
    // Inner hole (CCW in Y-up)
    let inner = poly(&[(245.0, 300.0), (375.0, 300.0), (310.0, 560.0)]);
    vec![outer, inner]
}

fn make_o_contours() -> Vec<Contour> {
    // Circular/oval 'O' using cubic curves.
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

fn make_base_project() -> FoundryProject {
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
    glyphs.insert(
        "H".to_string(),
        make_glyph('H', 640, vec![rect_cw(60.0, 0.0, 560.0, 720.0)]),
    );
    glyphs.insert("O".to_string(), make_glyph('O', 620, make_o_contours()));

    FoundryProject {
        version: 3,
        family_name: "Phase4 Foundry".to_string(),
        style_name: "Regular".to_string(),
        designer: "Cosmic Foundry".to_string(),
        license: "MIT".to_string(),
        metrics: make_metrics(),
        glyphs,
        kerning: vec![KerningPair {
            left: 'A' as u32,
            right: 'O' as u32,
            value: -30,
        }],
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    }
}

#[derive(Default)]
struct BboxPen {
    bp: BezPath,
}

impl OutlinePen for BboxPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.bp.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.bp.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.bp
            .quad_to((cx0 as f64, cy0 as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.bp.curve_to(
            (cx0 as f64, cy0 as f64),
            (cx1 as f64, cy1 as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.bp.close_path();
    }
}

#[test]
fn styles_generate_variants_with_expected_outlines_and_metrics() {
    let base = make_base_project();
    let base_h = base.glyphs.get("H").expect("H");
    let base_h_bb = compute_bbox(&base_h.contours);

    // 1. Bold
    let bold = generate_style_variant(&base, StyleVariantKind::Bold, 0.0);
    assert_eq!(bold.style_name, "Bold");
    let bold_h = bold.glyphs.get("H").expect("bold H");
    let bold_h_bb = compute_bbox(&bold_h.contours);
    assert!(bold_h.advance_width > base_h.advance_width);
    assert!(
        (bold_h_bb[3] - bold_h_bb[1]) > (base_h_bb[3] - base_h_bb[1]),
        "Bold height should expand"
    );

    // 2. Light (ensure thin glyphs are preserved and advance shrinks)
    let light = generate_style_variant(&base, StyleVariantKind::Light, 0.0);
    assert_eq!(light.style_name, "Light");
    let light_a = light.glyphs.get("A").expect("light A");
    assert!(!light_a.contours.is_empty());
    assert!(light_a.advance_width < base.glyphs["A"].advance_width);

    // 3. Condensed & Extended
    let condensed = generate_style_variant(&base, StyleVariantKind::Condensed, 0.0);
    assert_eq!(condensed.style_name, "Condensed");
    let cond_h = condensed.glyphs.get("H").expect("condensed H");
    let cond_h_bb = compute_bbox(&cond_h.contours);
    assert!(cond_h.advance_width < base_h.advance_width);
    assert!((cond_h_bb[2] - cond_h_bb[0]) < (base_h_bb[2] - base_h_bb[0]));
    assert!(condensed.kerning[0].value.abs() < base.kerning[0].value.abs());

    let extended = generate_style_variant(&base, StyleVariantKind::Extended, 0.0);
    assert_eq!(extended.style_name, "Extended");
    assert!(extended.glyphs["H"].advance_width > base_h.advance_width);

    // 4. SmallCaps
    let small_caps = generate_style_variant(&base, StyleVariantKind::SmallCaps, 0.0);
    assert_eq!(small_caps.style_name, "Small Caps");
    let sc_a = small_caps.glyphs.get("a").expect("small cap a");
    assert_eq!(sc_a.codepoint, 'a' as u32);
    let sc_a_bb = compute_bbox(&sc_a.contours);
    let sc_h = sc_a_bb[3] - sc_a_bb[1];
    assert!(
        sc_h > 450.0 && sc_h < 600.0,
        "expected small cap height near x_height (~520), got {sc_h}"
    );

    // 5. Oblique
    let oblique = generate_style_variant(&base, StyleVariantKind::Oblique, 12.0);
    assert_eq!(oblique.style_name, "Oblique");
    let ob_h_bb = compute_bbox(&oblique.glyphs["H"].contours);
    assert!(
        ob_h_bb[2] > base_h_bb[2] + 50.0,
        "Oblique should shear top rightwards: {ob_h_bb:?}"
    );

    // 6. Inline
    let inline = generate_style_variant(&base, StyleVariantKind::Inline, 0.0);
    assert_eq!(inline.style_name, "Inline");
    let inl_h = inline.glyphs.get("H").expect("inline H");
    assert!(
        inl_h.contours.len() >= 2,
        "Inline solid H should have outer + inner cavity contour, got {}",
        inl_h.contours.len()
    );
    let compiled_inline = compile_ttf(&inline, false).expect("inline compiles to TTF");
    assert!(!compiled_inline.ttf_bytes.is_empty());
}

#[test]
fn variable_font_compiles_fvar_and_gvar_and_interpolates_with_skrifa() {
    let base = make_base_project();
    let light = generate_style_variant(&base, StyleVariantKind::Light, 14.0);
    let bold = generate_style_variant(&base, StyleVariantKind::Bold, 28.0);

    let masters = vec![
        VariableMaster {
            name: "Light".to_string(),
            wght: 200.0,
            project: light,
        },
        VariableMaster {
            name: "Regular".to_string(),
            wght: 400.0,
            project: base.clone(),
        },
        VariableMaster {
            name: "Bold".to_string(),
            wght: 700.0,
            project: bold,
        },
    ];

    let bytes = compile_variable_ttf(&base, &masters).expect("compile_variable_ttf should succeed");
    let font = FontRef::new(&bytes).expect("skrifa should parse variable TTF");

    let axes = font.axes();
    assert_eq!(axes.len(), 1);
    let wght_axis = axes.get(0).expect("wght axis");
    assert_eq!(wght_axis.tag(), Tag::new(b"wght"));
    assert!((wght_axis.min_value() - 200.0).abs() < 1e-3);
    assert!((wght_axis.default_value() - 400.0).abs() < 1e-3);
    assert!((wght_axis.max_value() - 700.0).abs() < 1e-3);

    let instances = font.named_instances();
    assert!(
        instances.len() >= 2,
        "expected >= 2 named instances, got {}",
        instances.len()
    );
    assert!(font.gvar().is_ok(), "gvar table must be present");

    // Verify outline variation actually moves points between Light (200) and Bold (700).
    let gid_h = font.charmap().map('H').expect("H gid");
    let outline_h = font.outline_glyphs().get(gid_h).expect("H outline");

    let loc_light = font.axes().location([("wght", 200.0)]);
    let mut pen_light = BboxPen::default();
    outline_h
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::from(&loc_light)),
            &mut pen_light,
        )
        .expect("draw light H");

    let loc_bold = font.axes().location([("wght", 700.0)]);
    let mut pen_bold = BboxPen::default();
    outline_h
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::from(&loc_bold)),
            &mut pen_bold,
        )
        .expect("draw bold H");

    let bb_light = pen_light.bp.bounding_box();
    let bb_bold = pen_bold.bp.bounding_box();
    assert!(
        bb_bold.height() > bb_light.height() + 20.0,
        "Bold height ({}) should exceed Light height ({})",
        bb_bold.height(),
        bb_light.height()
    );
}

#[test]
fn qa_audit_detects_issues_and_autofix_repairs_them() {
    let mut project = make_base_project();
    // Inject multiple QA faults:
    // 1. Missing space
    project.glyphs.remove("space");
    // 2. Inverted descender
    project.metrics.descender = 200;
    // 3. Wrong winding (CCW outer), negative advance, fractional coordinates
    project.glyphs.insert(
        "B".to_string(),
        GlyphOutline {
            codepoint: 'B' as u32,
            name: "B".to_string(),
            advance_width: -50,
            lsb: 50,
            contours: vec![rect_ccw(50.25, 0.0, 450.75, 720.0)],
        },
    );
    // 4. Curve missing extrema (single arch cubic without apex anchor)
    project.glyphs.insert(
        "C".to_string(),
        GlyphOutline {
            codepoint: 'C' as u32,
            name: "C".to_string(),
            advance_width: 500,
            lsb: 50,
            contours: vec![Contour {
                anchors: vec![
                    Anchor {
                        x: 50.0,
                        y: 0.0,
                        h_in: [0.0, 0.0],
                        h_out: [0.0, 900.0],
                        smooth: false,
                    },
                    Anchor {
                        x: 450.0,
                        y: 0.0,
                        h_in: [0.0, 900.0],
                        h_out: [0.0, 0.0],
                        smooth: false,
                    },
                ],
                closed: true,
            }],
        },
    );
    // 5. Broken ligature rule
    project.ligatures.push(LigatureRule {
        name: "f_f".to_string(),
        components: vec!['f' as u32, 'f' as u32],
        glyph_key: "f_f".to_string(),
    });

    let before = audit_project(&project);
    let codes: Vec<&str> = before.issues.iter().map(|i| i.code.as_str()).collect();
    assert!(codes.contains(&"MISSING_SPACE_GLYPH"));
    assert!(codes.contains(&"METRICS_INVERTED_DESCENDER"));
    assert!(codes.contains(&"GLYPH_NEGATIVE_ADVANCE"));
    assert!(codes.contains(&"GLYPH_WRONG_WINDING"));
    assert!(codes.contains(&"GLYPH_NON_ROUNDED_COORDS"));
    assert!(codes.contains(&"GLYPH_MISSING_EXTREMA"));
    assert!(codes.contains(&"LIGATURE_BROKEN_RULE"));
    assert!(before.errors >= 2);
    assert!(before.warnings >= 4);

    let (fixed_count, after) = autofix_project(&mut project);
    assert!(fixed_count >= 5, "expected >= 5 fixes, got {fixed_count}");
    assert!(
        after.score > before.score,
        "expected score improvement: before={}, after={}",
        before.score,
        after.score
    );
    assert_eq!(after.errors, 0, "remaining errors: {:?}", after.issues);
    assert_eq!(after.warnings, 0, "remaining warnings: {:?}", after.issues);
    assert!(project.glyphs.values().any(|g| g.codepoint == 0x20));
    assert_eq!(project.metrics.descender, -200);
    assert!(project.glyphs["B"].advance_width > 0);
    assert!(project.ligatures.is_empty());
}

#[test]
fn retype_matches_extracted_a_outline_to_source_font_first() {
    let dir = tempdir().expect("tempdir");

    // Target font with distinctive 'A'
    let target_project = make_base_project();
    let target_ttf = compile_ttf(&target_project, false).expect("compile target");
    let target_path = dir.path().join("target.ttf");
    std::fs::write(&target_path, &target_ttf.ttf_bytes).expect("write target ttf");

    // Differing font where 'A' is a simple wide block (no triangular notch or counter)
    let mut other_project = make_base_project();
    other_project.family_name = "Other Block".to_string();
    other_project.glyphs.insert(
        "A".to_string(),
        make_glyph('A', 900, vec![rect_cw(40.0, 0.0, 860.0, 420.0)]),
    );
    let other_ttf = compile_ttf(&other_project, false).expect("compile other");
    let other_path = dir.path().join("other.ttf");
    std::fs::write(&other_path, &other_ttf.ttf_bytes).expect("write other ttf");

    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/GochiHand-Regular.ttf");

    let mut candidates = vec![
        (
            "Other Block".to_string(),
            other_path.to_string_lossy().to_string(),
        ),
        (
            "Phase4 Foundry".to_string(),
            target_path.to_string_lossy().to_string(),
        ),
    ];
    if fixture_path.exists() {
        candidates.push((
            "Gochi Hand".to_string(),
            fixture_path.to_string_lossy().to_string(),
        ));
    }

    let query_a = make_a_contours();
    let hits = match_outline_against_fonts(&query_a, &candidates, "AHO", 5);
    assert!(!hits.is_empty(), "expected at least one match hit");
    assert_eq!(hits[0].family, "Phase4 Foundry");
    assert_eq!(hits[0].matched_char, "A");
    assert!(
        hits[0].score > 80.0,
        "expected top match score > 80.0, got {} (dist={})",
        hits[0].score,
        hits[0].distance
    );
    if hits.len() > 1 {
        assert!(hits[0].score > hits[1].score);
    }

    // Verify Y-flipped (e.g., SVG-coordinate) query still matches the same font #1.
    let flipped_a: Vec<Contour> = query_a
        .iter()
        .map(|c| Contour {
            anchors: c
                .anchors
                .iter()
                .map(|a| Anchor {
                    x: a.x,
                    y: 720.0 - a.y,
                    h_in: [a.h_in[0], -a.h_in[1]],
                    h_out: [a.h_out[0], -a.h_out[1]],
                    smooth: a.smooth,
                })
                .collect(),
            closed: c.closed,
        })
        .collect();
    let flipped_hits = match_outline_against_fonts(&flipped_a, &candidates, "AHO", 5);
    assert!(!flipped_hits.is_empty());
    assert_eq!(flipped_hits[0].family, "Phase4 Foundry");
    assert!(flipped_hits[0].score > 80.0);
}
