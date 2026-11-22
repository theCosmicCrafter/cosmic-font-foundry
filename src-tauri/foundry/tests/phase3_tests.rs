use std::collections::BTreeMap;

use cosmic_core::reader::read_faces;
use cosmic_foundry::compiler::compile_ttf;
use cosmic_foundry::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphAnchor, GlyphOutline,
    LigatureRule, LinkedMetricRule,
};
use cosmic_foundry::typography::{
    apply_linked_metrics, auto_kern_project, auto_space_project, generate_latin_composites,
};
use skrifa::raw::tables::gsub::{LigatureSubst, SubstitutionSubtables};
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

fn make_empty_project() -> FoundryProject {
    FoundryProject {
        version: 3,
        family_name: "Phase3 Test".to_string(),
        style_name: "Regular".to_string(),
        designer: "Cosmic Foundry".to_string(),
        license: "MIT".to_string(),
        metrics: make_metrics(),
        glyphs: BTreeMap::new(),
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    }
}

fn poly(pts: &[(f64, f64)]) -> Contour {
    Contour {
        anchors: pts.iter().map(|&(x, y)| Anchor::on_curve(x, y)).collect(),
        closed: true,
    }
}

fn rect_contour(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
    poly(&[(x0, y0), (x0, y1), (x1, y1), (x1, y0)])
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

#[test]
fn auto_space_project_balances_asymmetric_sidebearings() {
    let mut project = make_empty_project();
    // Asymmetric 'H'-like block shifted far right (lsb=210, rsb=490).
    project.glyphs.insert(
        "H".to_string(),
        make_glyph('H', 1000, vec![rect_contour(210.0, 0.0, 510.0, 720.0)]),
    );
    // Asymmetric 'T': stem [250..350] up to 640, top crossbar [40..560] at [640..720].
    project.glyphs.insert(
        "T".to_string(),
        make_glyph(
            'T',
            900,
            vec![poly(&[
                (250.0, 0.0),
                (250.0, 640.0),
                (40.0, 640.0),
                (40.0, 720.0),
                (560.0, 720.0),
                (560.0, 640.0),
                (350.0, 640.0),
                (350.0, 0.0),
            ])],
        ),
    );
    // Space glyph should be skipped.
    project.glyphs.insert(
        " ".to_string(),
        GlyphOutline {
            codepoint: 0x20,
            name: "space".to_string(),
            advance_width: 300,
            lsb: 0,
            contours: Vec::new(),
        },
    );

    let spaced = auto_space_project(&mut project, 70.0);
    assert_eq!(spaced, 2);

    let h = project.glyphs.get("H").expect("H");
    let h_bb = compute_bbox(&h.contours);
    let h_lsb = h_bb[0];
    let h_rsb = h.advance_width as f64 - h_bb[2];
    assert!(
        (h_lsb - 70.0).abs() < 1.5,
        "expected H lsb ~70, got {h_lsb}"
    );
    assert!(
        (h_rsb - 70.0).abs() < 1.5,
        "expected H rsb ~70, got {h_rsb}"
    );

    // T's stem is weighted much more heavily than the top crossbar tip,
    // so optical spacing shifts the crossbar tip left of raw stem LSB.
    let t = project.glyphs.get("T").expect("T");
    let t_bb = compute_bbox(&t.contours);
    assert!(
        t_bb[0] < 0.0,
        "expected T crossbar tip to overhang left optically: {t_bb:?}"
    );
    assert_eq!(
        project.glyphs.get(" ").expect("space").advance_width,
        300,
        "space advance should remain untouched"
    );
}

#[test]
fn auto_kern_project_produces_negative_kerning_for_overhang_pairs() {
    let mut project = make_empty_project();

    // Clean geometric 'A' triangle (wide base [50..550], apex at (300, 720)).
    project.glyphs.insert(
        "A".to_string(),
        make_glyph(
            'A',
            620,
            vec![poly(&[(50.0, 0.0), (300.0, 720.0), (550.0, 0.0)])],
        ),
    );
    // Clean geometric 'V' inverted triangle (top [50..550] at 720, tip at (300, 0)).
    project.glyphs.insert(
        "V".to_string(),
        make_glyph(
            'V',
            620,
            vec![poly(&[(50.0, 720.0), (550.0, 720.0), (300.0, 0.0)])],
        ),
    );
    // Geometric 'T': stem [250..350], top bar [50..550] at [640..720].
    project.glyphs.insert(
        "T".to_string(),
        make_glyph(
            'T',
            620,
            vec![poly(&[
                (250.0, 0.0),
                (250.0, 640.0),
                (50.0, 640.0),
                (50.0, 720.0),
                (550.0, 720.0),
                (550.0, 640.0),
                (350.0, 640.0),
                (350.0, 0.0),
            ])],
        ),
    );
    // Geometric 'o' box [60..460] at [0..520].
    project.glyphs.insert(
        "o".to_string(),
        make_glyph('o', 520, vec![rect_contour(60.0, 0.0, 460.0, 520.0)]),
    );

    let pairs = auto_kern_project(&mut project, 120.0, 10);
    assert!(!pairs.is_empty(), "expected optical kerning pairs");

    let av = pairs
        .iter()
        .find(|p| p.left == 'A' as u32 && p.right == 'V' as u32)
        .expect("expected AV kerning pair");
    assert!(
        av.value < 0,
        "expected negative kerning for AV, got {}",
        av.value
    );

    let to = pairs
        .iter()
        .find(|p| p.left == 'T' as u32 && p.right == 'o' as u32)
        .expect("expected To kerning pair");
    assert!(
        to.value < 0,
        "expected negative kerning for To, got {}",
        to.value
    );
    assert_eq!(project.kerning.len(), pairs.len());
}

#[test]
fn generate_latin_composites_builds_accented_glyphs_from_bases() {
    let mut project = make_empty_project();
    for (ch, top_y, adv) in [
        ('A', 720.0, 600),
        ('E', 720.0, 560),
        ('N', 720.0, 620),
        ('U', 720.0, 600),
        ('C', 720.0, 580),
        ('a', 520.0, 500),
        ('i', 520.0, 280),
    ] {
        project.glyphs.insert(
            ch.to_string(),
            make_glyph(
                ch,
                adv,
                vec![rect_contour(60.0, 0.0, adv as f64 - 60.0, top_y)],
            ),
        );
    }

    // Add a custom top anchor on 'A' to verify anchor alignment is respected.
    project.anchors.insert(
        "A".to_string(),
        vec![GlyphAnchor {
            name: "top".to_string(),
            x: 300.0,
            y: 780.0,
        }],
    );

    let count = generate_latin_composites(&mut project);
    assert!(
        count >= 25,
        "expected composites for A,E,N,U,C,a,i bases, got {count}"
    );

    for target in ['Á', 'É', 'Ñ', 'Ü', 'á', 'í'] {
        let key = target.to_string();
        let glyph = project
            .glyphs
            .get(&key)
            .unwrap_or_else(|| panic!("missing composite glyph '{target}'"));
        assert_eq!(glyph.codepoint, target as u32);
        assert!(
            glyph.contours.len() >= 2,
            "expected base + diacritic contours for '{target}', got {}",
            glyph.contours.len()
        );
    }

    let c_cedilla = project
        .glyphs
        .get("Ç")
        .expect("missing composite glyph 'Ç'");
    assert_eq!(c_cedilla.codepoint, 'Ç' as u32);
    assert!(
        !c_cedilla.contours.is_empty(),
        "expected non-empty contours for 'Ç'"
    );
    let c_cedilla_bb = compute_bbox(&c_cedilla.contours);
    assert!(
        c_cedilla_bb[1] < -50.0 && c_cedilla_bb[3] >= 720.0,
        "expected Ç cedilla fused with C extending below baseline, bbox={c_cedilla_bb:?}"
    );

    let a_acute = project.glyphs.get("Á").expect("Á");
    assert_eq!(a_acute.advance_width, 600);
    let a_acute_bb = compute_bbox(&a_acute.contours);
    assert!(
        a_acute_bb[3] > 850.0,
        "expected Á diacritic above custom y=780 anchor, bbox={a_acute_bb:?}"
    );
}

#[test]
fn apply_linked_metrics_matches_lsb_and_rsb_including_mirror() {
    let mut project = make_empty_project();
    // Reference 'b': lsb=80, bbox=[80..480], adv=580 -> rsb=100.
    project.glyphs.insert(
        "b".to_string(),
        make_glyph('b', 580, vec![rect_contour(80.0, 0.0, 480.0, 720.0)]),
    );
    // Target 'p': starts with lsb=200, width=400, adv=700 -> rsb=100.
    project.glyphs.insert(
        "p".to_string(),
        make_glyph('p', 700, vec![rect_contour(200.0, -200.0, 600.0, 520.0)]),
    );
    // Target 'd': starts with lsb=150, width=400, adv=650.
    project.glyphs.insert(
        "d".to_string(),
        make_glyph('d', 650, vec![rect_contour(150.0, 0.0, 550.0, 720.0)]),
    );

    project.linked_metrics = vec![
        LinkedMetricRule {
            target: "p".to_string(),
            lsb_ref: Some("b".to_string()),
            rsb_ref: Some("b".to_string()),
            mirror: false,
        },
        LinkedMetricRule {
            target: "d".to_string(),
            lsb_ref: Some("b".to_string()),
            rsb_ref: Some("b".to_string()),
            mirror: true,
        },
    ];

    let updated = apply_linked_metrics(&mut project);
    assert_eq!(updated, 2);

    let p = project.glyphs.get("p").expect("p");
    let p_bb = compute_bbox(&p.contours);
    assert!((p_bb[0] - 80.0).abs() < 1e-6);
    assert!((p.advance_width as f64 - p_bb[2] - 100.0).abs() < 1e-6);

    let d = project.glyphs.get("d").expect("d");
    let d_bb = compute_bbox(&d.contours);
    assert!(
        (d_bb[0] - 100.0).abs() < 1e-6,
        "mirrored lsb should match b rsb"
    );
    assert!(
        (d.advance_width as f64 - d_bb[2] - 80.0).abs() < 1e-6,
        "mirrored rsb should match b lsb"
    );
}

#[test]
fn compile_ttf_with_fi_ligature_emits_valid_gsub_liga_feature() {
    let mut project = make_empty_project();
    project.glyphs.insert(
        "f".to_string(),
        make_glyph('f', 340, vec![rect_contour(40.0, 0.0, 280.0, 720.0)]),
    );
    project.glyphs.insert(
        "i".to_string(),
        make_glyph('i', 260, vec![rect_contour(60.0, 0.0, 200.0, 520.0)]),
    );
    // Unencoded ligature glyph (codepoint == 0)
    project.glyphs.insert(
        "f_i".to_string(),
        GlyphOutline {
            codepoint: 0,
            name: "f_i".to_string(),
            advance_width: 540,
            lsb: 40,
            contours: vec![rect_contour(40.0, 0.0, 500.0, 720.0)],
        },
    );
    project.ligatures.push(LigatureRule {
        name: "f_i".to_string(),
        components: vec!['f' as u32, 'i' as u32],
        glyph_key: "f_i".to_string(),
    });

    let compiled = compile_ttf(&project, false).expect("compile_ttf with GSUB ligature");
    // .notdef, space, f_i (cp=0), f, i -> 5 glyphs
    assert_eq!(compiled.glyph_count, 5);

    let font = FontRef::new(&compiled.ttf_bytes).expect("skrifa parse compiled TTF");
    let f_gid = font.charmap().map('f').expect("map f");
    let i_gid = font.charmap().map('i').expect("map i");
    let fi_gid = font
        .glyph_names()
        .iter()
        .find_map(|(gid, name)| (name == "f_i").then_some(gid))
        .expect("f_i glyph present in post table");

    let gsub = font.gsub().expect("GSUB table present");
    let scripts = gsub.script_list().expect("script list");
    assert!(
        scripts
            .script_records()
            .iter()
            .any(|sr| sr.script_tag() == Tag::new(b"DFLT")),
        "expected DFLT script in GSUB"
    );

    let features = gsub.feature_list().expect("feature list");
    assert!(
        features
            .feature_records()
            .iter()
            .any(|fr| fr.feature_tag() == Tag::new(b"liga")),
        "expected liga feature in GSUB"
    );

    let lookups = gsub.lookup_list().expect("lookup list");
    let lookup = lookups.lookups().get(0).expect("lookup 0");
    let subtables = lookup.subtables().expect("subtables");
    let mut found_fi = false;
    if let SubstitutionSubtables::Ligature(lig_subtables) = subtables {
        for sub in lig_subtables.iter().flatten() {
            if let LigatureSubst::Format1(sub1) = sub {
                if let Some(cov_idx) = sub1.coverage().ok().and_then(|c| c.get(f_gid)) {
                    if let Ok(lig_set) = sub1.ligature_sets().get(cov_idx as usize) {
                        for lig in lig_set.ligatures().iter().flatten() {
                            if lig.ligature_glyph() == fi_gid
                                && lig.component_glyph_ids().len() == 1
                                && lig.component_glyph_ids()[0].get() == i_gid
                            {
                                found_fi = true;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(found_fi, "expected f + i -> f_i ligature rule in GSUB");

    // Verify cosmic_core::reader::read_faces detects "liga" in opentype_features.
    let dir = tempdir().expect("tempdir");
    let ttf_path = dir.path().join("liga_test.ttf");
    std::fs::write(&ttf_path, &compiled.ttf_bytes).expect("write ttf");
    let faces = read_faces(&ttf_path).expect("cosmic_core read_faces");
    assert_eq!(faces.len(), 1);
    assert!(
        faces[0].opentype_features.iter().any(|f| f == "liga"),
        "expected cosmic_core reader to detect 'liga', got {:?}",
        faces[0].opentype_features
    );
}

#[test]
fn foundry_project_deserializes_legacy_json_without_phase3_fields() {
    let legacy_json = r#"{
        "version": 2,
        "family_name": "Legacy",
        "style_name": "Regular",
        "designer": "Test",
        "license": "MIT",
        "metrics": {
            "upm": 1000,
            "ascender": 800,
            "descender": -200,
            "x_height": 520,
            "cap_height": 720,
            "line_gap": 0
        },
        "glyphs": {},
        "kerning": []
    }"#;
    let project: FoundryProject =
        serde_json::from_str(legacy_json).expect("legacy v1/v2 JSON should deserialize");
    assert!(project.ligatures.is_empty());
    assert!(project.linked_metrics.is_empty());
    assert!(project.anchors.is_empty());
}
