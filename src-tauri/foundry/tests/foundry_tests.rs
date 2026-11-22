use std::collections::BTreeMap;
use std::path::PathBuf;

use cosmic_foundry::compiler::{compile_ttf, compile_woff, export_ufo, import_ufo, subset_font};
use cosmic_foundry::import::{fork_font_file, import_svg_glyph, inspect_glyph_outline};
use cosmic_foundry::model::{GlyphTransformOp, ModularForgeRequest};
use cosmic_foundry::modular::modular_to_project;
use cosmic_foundry::pathops::apply_transform;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::tables::glyf::Glyph as RawGlyph;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider};

fn fixture_font_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/GochiHand-Regular.ttf")
}

fn two_touching_cells_request(
    shape: &str,
    corner: f64,
    remove_overlap: bool,
    smooth_corners: bool,
) -> ModularForgeRequest {
    let mut grid = vec![vec![0u8; 7]; 9];
    // Two horizontally adjacent cells on row 4; at weight=100 they touch along their shared edge.
    grid[4][2] = 1;
    grid[4][3] = 1;

    let mut glyphs = BTreeMap::new();
    glyphs.insert("A".to_string(), grid);

    ModularForgeRequest {
        family_name: "Test Modular".to_string(),
        style_name: "Regular".to_string(),
        weight: 100.0,
        contrast: 100.0,
        slant: 0.0,
        tracking: 80.0,
        corner,
        shape: shape.to_string(),
        glyphs,
        glyph_widths: BTreeMap::new(),
        cols: 7,
        rows: 9,
        remove_overlap,
        smooth_corners,
        optical_offset: 0.0,
    }
}

#[test]
fn touching_modular_cells_fuse_when_remove_overlap_enabled() {
    let unfused_req = two_touching_cells_request("square", 0.0, false, false);
    let unfused = modular_to_project(&unfused_req);
    let a_unfused = unfused.glyphs.get("A").expect("glyph A should exist");
    assert_eq!(a_unfused.contours.len(), 2);

    let fused_req = two_touching_cells_request("square", 0.0, true, false);
    let fused = modular_to_project(&fused_req);
    let a_fused = fused.glyphs.get("A").expect("glyph A should exist");
    assert_eq!(a_fused.contours.len(), 1);
}

#[derive(Default)]
struct CurveCounter {
    quads: usize,
    cubics: usize,
}

impl OutlinePen for CurveCounter {
    fn move_to(&mut self, _x: f32, _y: f32) {}
    fn line_to(&mut self, _x: f32, _y: f32) {}
    fn quad_to(&mut self, _cx0: f32, _cy0: f32, _x: f32, _y: f32) {
        self.quads += 1;
    }
    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, _x: f32, _y: f32) {
        self.cubics += 1;
    }
    fn close(&mut self) {}
}

#[test]
fn rounded_corners_produce_offcurve_quadratic_points_in_ttf() {
    let req = two_touching_cells_request("rounded", 40.0, true, true);
    let project = modular_to_project(&req);
    let a_glyph = project.glyphs.get("A").expect("glyph A should exist");
    assert!(
        a_glyph
            .contours
            .iter()
            .flat_map(|c| c.anchors.iter())
            .any(|a| a.has_h_in() || a.has_h_out()),
        "rounded modular glyph should have cubic Bezier handles"
    );

    let compiled = compile_ttf(&project, false).expect("compile_ttf should succeed");
    let font = FontRef::new(&compiled.ttf_bytes).expect("compiled TTF should parse with skrifa");
    let gid = font.charmap().map('A').expect("charmap should map 'A'");

    let loca = font.loca(None).expect("loca table");
    let glyf = font.glyf().expect("glyf table");
    let raw_glyph = loca
        .get_glyf(gid, &glyf)
        .expect("read glyph")
        .expect("non-empty glyph");
    match raw_glyph {
        RawGlyph::Simple(sg) => {
            let offcurve_count = sg.points().filter(|p| !p.on_curve).count();
            assert!(
                offcurve_count > 0,
                "expected off-curve quadratic points in compiled glyf, got 0"
            );
        }
        RawGlyph::Composite(_) => panic!("expected simple glyph for 'A'"),
    }

    let mut counter = CurveCounter::default();
    let outline = font.outline_glyphs().get(gid).expect("outline for 'A'");
    outline
        .draw(
            DrawSettings::unhinted(
                skrifa::instance::Size::unscaled(),
                skrifa::instance::LocationRef::default(),
            ),
            &mut counter,
        )
        .expect("draw outline");
    assert!(
        counter.quads > 0,
        "skrifa outline draw should emit quadratic segments"
    );

    let woff = compile_woff(&compiled.ttf_bytes).expect("WOFF conversion should succeed");
    assert_eq!(&woff[0..4], b"wOFF");
}

#[test]
fn fork_offset_and_compile_gochi_hand() {
    let path = fixture_font_path();
    let mut project = fork_font_file(&path, None, 64).expect("fork_font_file should succeed");
    assert!(project.family_name.ends_with("Remix"));
    assert!(!project.glyphs.is_empty());

    let inspect = inspect_glyph_outline(&path, 'A' as u32).expect("inspect_glyph_outline");
    assert!(!inspect.svg_path.is_empty());
    assert!(inspect.contour_count > 0);

    let op = GlyphTransformOp::Offset {
        delta: 12.0,
        round_joins: true,
    };
    for glyph in project.glyphs.values_mut() {
        if !glyph.contours.is_empty() {
            glyph.contours = apply_transform(&glyph.contours, &op);
        }
    }

    let compiled = compile_ttf(&project, true).expect("compile_ttf after offset should succeed");
    let font = FontRef::new(&compiled.ttf_bytes).expect("compiled TTF should parse cleanly");
    assert!(font.charmap().map('A').is_some());
    assert!(compiled.glyph_count >= 2);
}

#[test]
fn import_svg_glyph_parses_raw_d_and_svg_xml() {
    let raw_d = "M 10 10 L 110 10 L 110 110 A 20 20 0 0 1 90 130 L 10 130 Z";
    let xml = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 150 150"><path fill="black" d="{raw_d}"/></svg>"#
    );

    let from_d = import_svg_glyph(raw_d, 1000).expect("raw d should parse");
    let from_xml = import_svg_glyph(&xml, 1000).expect("svg xml should parse");

    assert_eq!(from_d.len(), 1);
    assert_eq!(from_xml.len(), 1);
    assert_eq!(from_d[0].anchors.len(), from_xml[0].anchors.len());

    let bbox = cosmic_foundry::model::compute_bbox(&from_xml);
    let height = bbox[3] - bbox[1];
    assert!(
        (height - 720.0).abs() < 2.0,
        "expected normalized height ~720, got {height}"
    );
    assert!(
        bbox[1].abs() < 1e-3,
        "expected baseline at y=0, got {}",
        bbox[1]
    );
}

#[test]
fn ufo_round_trip_preserves_metadata_and_contours() {
    let req = two_touching_cells_request("rounded", 35.0, true, true);
    let project = modular_to_project(&req);

    let temp_dir = tempfile::tempdir().expect("create tempdir");
    let ufo_path = temp_dir.path().join("TestRoundTrip.ufo");
    export_ufo(&project, &ufo_path).expect("export_ufo should succeed");

    let reimported = import_ufo(&ufo_path).expect("import_ufo should succeed");
    assert_eq!(reimported.family_name, project.family_name);
    assert_eq!(reimported.style_name, project.style_name);
    assert_eq!(reimported.metrics.upm, project.metrics.upm);
    assert_eq!(reimported.metrics.ascender, project.metrics.ascender);
    assert_eq!(reimported.metrics.descender, project.metrics.descender);
    assert_eq!(reimported.glyphs.len(), project.glyphs.len());

    let orig_a = project.glyphs.get("A").expect("original A");
    let rt_a = reimported.glyphs.get("A").expect("roundtrip A");
    assert_eq!(rt_a.contours.len(), orig_a.contours.len());
    assert_eq!(rt_a.advance_width, orig_a.advance_width);
}

#[test]
fn subset_font_produces_smaller_valid_ttf() {
    let path = fixture_font_path();
    let orig_len = std::fs::metadata(&path).expect("fixture metadata").len() as usize;

    let sub = subset_font(&path, "Cosmic", &[]).expect("subset_font should succeed");
    assert!(!sub.is_empty());
    assert!(
        sub.len() < orig_len,
        "expected subset ({}) < original ({orig_len})",
        sub.len()
    );

    let parsed = FontRef::new(&sub).expect("subsetted TTF should parse with skrifa");
    let glyph_count = parsed
        .metrics(
            skrifa::instance::Size::unscaled(),
            skrifa::instance::LocationRef::default(),
        )
        .glyph_count;
    assert!(glyph_count > 0 && glyph_count <= 10);
}

#[test]
fn calligraphic_sweep_and_stroke_produce_closed_contours() {
    use cosmic_foundry::model::{Anchor, Contour};
    use cosmic_foundry::pathops::{calligraphic_sweep, outline_stroke};

    let stroke = vec![Contour {
        anchors: vec![
            Anchor::on_curve(100.0, 100.0),
            Anchor::on_curve(300.0, 400.0),
            Anchor::on_curve(500.0, 100.0),
        ],
        closed: false,
    }];

    let swept = calligraphic_sweep(&stroke, 40.0, 30.0, 0.3);
    assert_eq!(swept.len(), 1);
    assert!(swept[0].closed);

    let outlined = outline_stroke(&stroke, 24.0, true);
    assert_eq!(outlined.len(), 1);
    assert!(outlined[0].closed);
}
