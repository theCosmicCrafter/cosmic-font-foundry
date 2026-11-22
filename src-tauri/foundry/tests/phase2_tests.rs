use std::io::Cursor;

use cosmic_foundry::edit::{boolean_op_contours, measure_stem_thickness, slice_contours_with_line};
use cosmic_foundry::model::{compute_bbox, Anchor, Contour};
use cosmic_foundry::trace::{trace_glyph_sheet, trace_image_bytes, TraceOptions};
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use kurbo::Shape;

fn encode_gray_png(img: GrayImage) -> Vec<u8> {
    let mut buf = Vec::new();
    DynamicImage::ImageLuma8(img)
        .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .expect("encode png");
    buf
}

fn make_rect_contour(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
    Contour {
        anchors: vec![
            Anchor::on_curve(x0, y0),
            Anchor::on_curve(x1, y0),
            Anchor::on_curve(x1, y1),
            Anchor::on_curve(x0, y1),
        ],
        closed: true,
    }
}

#[test]
fn trace_outline_ring_produces_outer_and_hole_normalized_to_720() {
    let mut img = GrayImage::from_pixel(128, 128, Luma([255]));
    let cx = 64.0_f64;
    let cy = 64.0_f64;
    for y in 0..128 {
        for x in 0..128 {
            let r = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
            if (22.0..=48.0).contains(&r) {
                img.put_pixel(x, y, Luma([0]));
            }
        }
    }
    let png = encode_gray_png(img);
    let opts = TraceOptions::default();
    let contours = trace_image_bytes(&png, &opts).expect("trace ring");

    assert_eq!(contours.len(), 2, "expected outer + inner hole contour");
    assert!(contours.iter().all(|c| c.closed));

    let mut areas: Vec<f64> = contours.iter().map(|c| c.to_bezpath().area()).collect();
    areas.sort_by(|a, b| a.total_cmp(b));
    // With truetype_cw_outer = true: outer has negative area, inner hole has positive area.
    assert!(
        areas[0] < -1000.0,
        "outer contour should have large CW area: {areas:?}"
    );
    assert!(
        areas[1] > 1000.0,
        "hole contour should have positive CCW area: {areas:?}"
    );

    let bbox = compute_bbox(&contours);
    assert!(bbox[1].abs() < 1e-3, "baseline should sit at y=0: {bbox:?}");
    let height = bbox[3] - bbox[1];
    assert!(
        (height - 720.0).abs() < 2.0,
        "expected normalized height ~720, got {height}"
    );
}

#[test]
fn trace_centerline_bar_produces_open_skeleton() {
    let mut img = GrayImage::from_pixel(120, 120, Luma([255]));
    // Draw a 16px wide vertical bar centered at x = 60, y in [15..105].
    for y in 15..105 {
        for x in 52..68 {
            img.put_pixel(x, y, Luma([0]));
        }
    }
    let png = encode_gray_png(img);
    let opts = TraceOptions {
        mode: "centerline".to_string(),
        ..TraceOptions::default()
    };
    let contours = trace_image_bytes(&png, &opts).expect("trace centerline");

    assert!(!contours.is_empty(), "expected skeleton contour");
    assert!(
        contours.iter().any(|c| !c.closed && c.anchors.len() >= 2),
        "expected at least one open skeleton stroke: {contours:?}"
    );
}

#[test]
fn trace_glyph_sheet_2x2_maps_abcd() {
    let mut img = GrayImage::from_pixel(128, 128, Luma([255]));
    // Paint a solid block inside each of the 4 cells (64x64 each).
    let cells = [(0u32, 0u32), (64, 0), (0, 64), (64, 64)];
    for (ox, oy) in cells {
        for y in (oy + 12)..(oy + 52) {
            for x in (ox + 16)..(ox + 48) {
                img.put_pixel(x, y, Luma([0]));
            }
        }
    }
    let png = encode_gray_png(img);
    let sheet =
        trace_glyph_sheet(&png, 2, 2, "ABCD", &TraceOptions::default()).expect("trace glyph sheet");

    assert_eq!(sheet.len(), 4);
    for key in ["A", "B", "C", "D"] {
        let g = sheet.get(key).expect("missing traced glyph");
        assert!(!g.is_empty(), "glyph {key} should have contours");
    }
}

#[test]
fn measure_stem_thickness_across_100_unit_rect() {
    let rect = vec![make_rect_contour(100.0, 0.0, 200.0, 500.0)];
    let hits = measure_stem_thickness(&rect, [20.0, 250.0], [280.0, 250.0]);
    assert_eq!(hits.len(), 1, "expected 1 stem measurement: {hits:?}");
    assert!(
        (hits[0].distance - 100.0).abs() < 1e-3,
        "expected distance ~100.0, got {}",
        hits[0].distance
    );
}

#[test]
fn slice_contours_with_line_splits_square_into_two() {
    let square = vec![make_rect_contour(0.0, 0.0, 200.0, 200.0)];
    let sliced = slice_contours_with_line(&square, [-50.0, 100.0], [250.0, 100.0]);
    assert_eq!(sliced.len(), 2, "expected 2 closed halves: {sliced:?}");
    assert!(sliced.iter().all(|c| c.closed));
    let total_area: f64 = sliced.iter().map(|c| c.to_bezpath().area().abs()).sum();
    assert!(
        (total_area - 200.0 * 200.0).abs() < 200.0,
        "sliced area should roughly equal original square minus 0.5px slit, got {total_area}"
    );
}

#[test]
fn boolean_op_union_and_difference_produce_expected_areas() {
    let a = vec![make_rect_contour(0.0, 0.0, 100.0, 100.0)];
    let b = vec![make_rect_contour(50.0, 0.0, 150.0, 100.0)];

    let union = boolean_op_contours(&a, &b, "union");
    assert_eq!(
        union.len(),
        1,
        "overlapping rects union should be 1 contour"
    );
    let union_area: f64 = union.iter().map(|c| c.to_bezpath().area().abs()).sum();
    assert!(
        (union_area - 15_000.0).abs() < 1e-2,
        "expected union area 15000, got {union_area}"
    );

    let diff = boolean_op_contours(&a, &b, "difference");
    assert_eq!(diff.len(), 1, "difference should leave 1 contour");
    let diff_area: f64 = diff.iter().map(|c| c.to_bezpath().area().abs()).sum();
    assert!(
        (diff_area - 5_000.0).abs() < 1e-2,
        "expected difference area 5000, got {diff_area}"
    );
}
