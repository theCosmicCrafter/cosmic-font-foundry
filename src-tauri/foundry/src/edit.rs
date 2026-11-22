use kurbo::{flatten, BezPath, PathEl, Point, Shape};
use linesweeper::{binary_op, BinaryOp, FillRule};
use serde::{Deserialize, Serialize};

use crate::model::{compute_bbox, Anchor, Contour};
use crate::pathops::normalize_winding;

const EPS_AREA: f64 = 1e-4;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StemMeasurement {
    pub p0: [f64; 2],
    pub p1: [f64; 2],
    pub distance: f64,
}

fn is_valid_closed_contour(c: &Contour) -> bool {
    if !c.closed || c.anchors.len() < 2 {
        return false;
    }
    let all_finite = c.anchors.iter().all(|a| {
        a.x.is_finite()
            && a.y.is_finite()
            && a.h_in[0].is_finite()
            && a.h_in[1].is_finite()
            && a.h_out[0].is_finite()
            && a.h_out[1].is_finite()
    });
    if !all_finite {
        return false;
    }
    let bp = c.to_bezpath();
    let bb = bp.bounding_box();
    (bb.width() > 1e-5 || bb.height() > 1e-5) && bp.area().abs() > EPS_AREA
}

fn collect_closed_bezpath(contours: &[Contour]) -> Option<BezPath> {
    let mut combined = BezPath::new();
    let mut has_any = false;
    for c in contours {
        if is_valid_closed_contour(c) {
            combined.extend(c.to_bezpath().elements().iter().copied());
            has_any = true;
        }
    }
    has_any.then_some(combined)
}

/// Measures stem thicknesses along `[ray_start, ray_end]` by flattening closed contours,
/// intersecting with the caliper segment, sorting crossings, and pairing `(in, out)` hits.
pub fn measure_stem_thickness(
    contours: &[Contour],
    ray_start: [f64; 2],
    ray_end: [f64; 2],
) -> Vec<StemMeasurement> {
    let a = Point::new(ray_start[0], ray_start[1]);
    let b = Point::new(ray_end[0], ray_end[1]);
    let d = b - a;
    let ray_len = d.hypot();
    if ray_len <= 1e-6 || !ray_len.is_finite() {
        return Vec::new();
    }
    let unit_d = d / ray_len;

    let mut crossings: Vec<f64> = Vec::new();
    let mut push_edge = |p0: Point, p1: Point| {
        let e = p1 - p0;
        if e.hypot2() <= 1e-12 {
            return;
        }
        // Signed perpendicular distance from the ray line (half-open on s == 0 handles shared vertices).
        let s0 = (p0 - a).cross(unit_d);
        let s1 = (p1 - a).cross(unit_d);
        let crosses = (s0 > 0.0 && s1 <= 0.0) || (s1 > 0.0 && s0 <= 0.0);
        if !crosses {
            return;
        }
        let denom = d.cross(e);
        if denom.abs() <= 1e-12 {
            return;
        }
        let t = (p0 - a).cross(e) / denom;
        if (-1e-6..=1.0 + 1e-6).contains(&t) {
            crossings.push(t.clamp(0.0, 1.0));
        }
    };

    for contour in contours {
        if !contour.closed || contour.anchors.len() < 2 {
            continue;
        }
        let bp = contour.to_bezpath();
        let mut subpath_start: Option<Point> = None;
        let mut cur: Option<Point> = None;
        flatten(bp.elements().iter().copied(), 0.25, |el| match el {
            PathEl::MoveTo(p) => {
                subpath_start = Some(p);
                cur = Some(p);
            }
            PathEl::LineTo(p) => {
                if let Some(c) = cur {
                    push_edge(c, p);
                }
                cur = Some(p);
            }
            PathEl::ClosePath => {
                if let (Some(c), Some(s)) = (cur, subpath_start) {
                    if (c - s).hypot() > 1e-9 {
                        push_edge(c, s);
                    }
                    cur = Some(s);
                }
            }
            PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => {}
        });
    }

    crossings.sort_by(|x, y| x.total_cmp(y));

    // Cancel adjacent coincident hits (e.g. grazing a convex vertex) so real (in, out) pairs align.
    let eps_t = 1e-4 / ray_len;
    let mut cleaned: Vec<f64> = Vec::with_capacity(crossings.len());
    for t in crossings {
        if let Some(&prev) = cleaned.last() {
            if (t - prev).abs() <= eps_t {
                cleaned.pop();
                continue;
            }
        }
        cleaned.push(t);
    }

    let mut measurements = Vec::new();
    for &[t0, t1] in cleaned.as_chunks::<2>().0 {
        let p0 = a + d * t0;
        let p1 = a + d * t1;
        let distance = (p1 - p0).hypot();
        if distance > 1e-4 {
            measurements.push(StemMeasurement {
                p0: [p0.x, p0.y],
                p1: [p1.x, p1.y],
                distance,
            });
        }
    }
    measurements
}

/// Applies a 2D boolean operation (`"union"`, `"difference"`, `"intersection"`, `"xor"`)
/// between two contour sets using `linesweeper::binary_op` and normalizes winding.
pub fn boolean_op_contours(a: &[Contour], b: &[Contour], op: &str) -> Vec<Contour> {
    let bin_op = match op.trim().to_ascii_lowercase().as_str() {
        "union" => BinaryOp::Union,
        "difference" | "subtract" => BinaryOp::Difference,
        "intersection" | "intersect" => BinaryOp::Intersection,
        "xor" => BinaryOp::Xor,
        _ => return normalize_winding(a, true),
    };

    let path_a = collect_closed_bezpath(a);
    let path_b = collect_closed_bezpath(b);

    match (path_a, path_b) {
        (None, None) => Vec::new(),
        (Some(_), None) => match bin_op {
            BinaryOp::Union | BinaryOp::Difference | BinaryOp::Xor => normalize_winding(a, true),
            BinaryOp::Intersection => Vec::new(),
        },
        (None, Some(_)) => match bin_op {
            BinaryOp::Union | BinaryOp::Xor => normalize_winding(b, true),
            BinaryOp::Difference | BinaryOp::Intersection => Vec::new(),
        },
        (Some(pa), Some(pb)) => match binary_op(&pa, &pb, FillRule::NonZero, bin_op) {
            Ok(ls_contours) => {
                let mut out = Vec::new();
                for c in ls_contours.into_contours() {
                    for mut mc in Contour::from_bezpath(&c.path) {
                        mc.closed = true;
                        if is_valid_closed_contour(&mc) {
                            out.push(mc);
                        }
                    }
                }
                normalize_winding(&out, true)
            }
            Err(_) => match bin_op {
                BinaryOp::Union => {
                    let mut combined = a.to_vec();
                    combined.extend(b.iter().cloned());
                    normalize_winding(&combined, true)
                }
                BinaryOp::Difference | BinaryOp::Xor => normalize_winding(a, true),
                BinaryOp::Intersection => Vec::new(),
            },
        },
    }
}

/// Slices closed contours along the infinite line passing through `p0` and `p1`
/// by subtracting a razor-thin `0.5` UPM rectangle centered on the line.
pub fn slice_contours_with_line(contours: &[Contour], p0: [f64; 2], p1: [f64; 2]) -> Vec<Contour> {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = dx.hypot(dy);
    if len < 1e-3 || !len.is_finite() || contours.is_empty() {
        return contours.to_vec();
    }

    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;

    let bbox = compute_bbox(contours);
    let mid_x = 0.5 * (p0[0] + p1[0]);
    let mid_y = 0.5 * (p0[1] + p1[1]);
    let diag = (bbox[2] - bbox[0]).hypot(bbox[3] - bbox[1]);
    let dist_to_box = (mid_x - 0.5 * (bbox[0] + bbox[2])).hypot(mid_y - 0.5 * (bbox[1] + bbox[3]));
    let ext = diag + dist_to_box + len + 1000.0;
    let half_w = 0.25;

    let sx = mid_x - ux * ext;
    let sy = mid_y - uy * ext;
    let ex = mid_x + ux * ext;
    let ey = mid_y + uy * ext;

    let slit = Contour {
        anchors: vec![
            Anchor::on_curve(sx - nx * half_w, sy - ny * half_w),
            Anchor::on_curve(ex - nx * half_w, ey - ny * half_w),
            Anchor::on_curve(ex + nx * half_w, ey + ny * half_w),
            Anchor::on_curve(sx + nx * half_w, sy + ny * half_w),
        ],
        closed: true,
    };

    boolean_op_contours(contours, &[slit], "difference")
}
