use std::f64::consts::{PI, SQRT_2};

use kurbo::{
    BezPath, Cap, CubicBez, Join, ParamCurve, ParamCurveArclen, ParamCurveDeriv, ParamCurveExtrema,
    PathEl, PathSeg, Point, Shape, Stroke, StrokeOpts, Vec2,
};
use linesweeper::{binary_op, topology::Topology, BinaryOp, FillRule};

use crate::model::{Anchor, Contour, GlyphTransformOp};

const KAPPA: f64 = 4.0 / 3.0 * (SQRT_2 - 1.0);
const EPS_AREA: f64 = 1e-4;
const EPS_LEN: f64 = 1e-6;

fn is_contour_finite_and_valid(c: &Contour) -> bool {
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

fn compute_sweep_eps(paths: &[BezPath]) -> f64 {
    let mut combined: Option<kurbo::Rect> = None;
    for p in paths {
        let b = p.bounding_box();
        combined = Some(match combined {
            Some(acc) => acc.union(b),
            None => b,
        });
    }
    let Some(bbox) = combined else {
        return 1e-6;
    };
    let min_v = bbox.min_x().min(bbox.min_y());
    let max_v = bbox.max_x().max(bbox.max_y());
    let m_2 = min_v.abs().max(max_v.abs());
    (m_2 * (f64::EPSILON * 64.0)).max(1e-6)
}

fn linesweeper_contours_to_model(ls_contours: linesweeper::topology::Contours) -> Vec<Contour> {
    let mut out = Vec::new();
    for c in ls_contours.into_contours() {
        for mut mc in Contour::from_bezpath(&c.path) {
            mc.closed = true;
            if is_contour_finite_and_valid(&mc) {
                out.push(mc);
            }
        }
    }
    out
}

/// Fuses overlapping closed contours via `linesweeper` non-zero fill topology,
/// returning clean non-overlapping outer and hole contours.
pub fn remove_overlap(contours: &[Contour]) -> Vec<Contour> {
    if contours.is_empty() {
        return Vec::new();
    }

    let mut open_contours = Vec::new();
    let mut closed_paths = Vec::new();
    for c in contours {
        if !c.closed {
            open_contours.push(c.clone());
        } else if is_contour_finite_and_valid(c) {
            closed_paths.push(c.to_bezpath());
        }
    }

    if closed_paths.is_empty() {
        return open_contours;
    }

    let eps = compute_sweep_eps(&closed_paths);
    let mut result = match Topology::<i32>::from_paths(closed_paths.iter().map(|p| (p, ())), eps) {
        Ok(top) => {
            let ls_contours = top.contours(|w| *w != 0);
            linesweeper_contours_to_model(ls_contours)
        }
        Err(_) => contours
            .iter()
            .filter(|c| c.closed && is_contour_finite_and_valid(c))
            .cloned()
            .collect(),
    };

    result.extend(open_contours);
    result
}

#[derive(Clone, Copy)]
enum SegGeom {
    Line(Point, Point),
    Cubic(CubicBez),
}

impl SegGeom {
    fn from_anchors(a: &Anchor, b: &Anchor) -> Self {
        if !a.has_h_out() && !b.has_h_in() {
            Self::Line(a.point(), b.point())
        } else {
            Self::Cubic(CubicBez::new(
                a.point(),
                Point::new(a.x + a.h_out[0], a.y + a.h_out[1]),
                Point::new(b.x + b.h_in[0], b.y + b.h_in[1]),
                b.point(),
            ))
        }
    }

    fn arclen(&self) -> f64 {
        match *self {
            Self::Line(p0, p1) => (p1 - p0).hypot(),
            Self::Cubic(c) => c.arclen(0.1),
        }
    }

    fn start_tangent(&self) -> Vec2 {
        match *self {
            Self::Line(p0, p1) => normalize_or_zero(p1 - p0),
            Self::Cubic(c) => {
                let d = c.deriv().eval(0.0).to_vec2();
                if d.hypot() > EPS_LEN {
                    normalize_or_zero(d)
                } else {
                    let v1 = c.p1 - c.p0;
                    if v1.hypot() > EPS_LEN {
                        normalize_or_zero(v1)
                    } else {
                        let v2 = c.p2 - c.p0;
                        if v2.hypot() > EPS_LEN {
                            normalize_or_zero(v2)
                        } else {
                            normalize_or_zero(c.p3 - c.p0)
                        }
                    }
                }
            }
        }
    }

    fn end_tangent(&self) -> Vec2 {
        match *self {
            Self::Line(p0, p1) => normalize_or_zero(p1 - p0),
            Self::Cubic(c) => {
                let d = c.deriv().eval(1.0).to_vec2();
                if d.hypot() > EPS_LEN {
                    normalize_or_zero(d)
                } else {
                    let v1 = c.p3 - c.p2;
                    if v1.hypot() > EPS_LEN {
                        normalize_or_zero(v1)
                    } else {
                        let v2 = c.p3 - c.p1;
                        if v2.hypot() > EPS_LEN {
                            normalize_or_zero(v2)
                        } else {
                            normalize_or_zero(c.p3 - c.p0)
                        }
                    }
                }
            }
        }
    }

    fn param_at_arclen(&self, dist: f64, total_len: f64) -> f64 {
        if total_len <= EPS_LEN || dist <= 0.0 {
            return 0.0;
        }
        if dist >= total_len {
            return 1.0;
        }
        match *self {
            Self::Line(_, _) => (dist / total_len).clamp(0.0, 1.0),
            Self::Cubic(c) => {
                let mut lo = 0.0;
                let mut hi = 1.0;
                for _ in 0..18 {
                    let mid = 0.5 * (lo + hi);
                    let len = c.subsegment(0.0..mid).arclen(0.1);
                    if len < dist {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                0.5 * (lo + hi)
            }
        }
    }

    fn subsegment(&self, t0: f64, t1: f64) -> Self {
        match *self {
            Self::Line(p0, p1) => Self::Line(p0.lerp(p1, t0), p0.lerp(p1, t1)),
            Self::Cubic(c) => Self::Cubic(c.subsegment(t0..t1)),
        }
    }

    fn start_point(&self) -> Point {
        match *self {
            Self::Line(p0, _) => p0,
            Self::Cubic(c) => c.p0,
        }
    }

    fn end_point(&self) -> Point {
        match *self {
            Self::Line(_, p1) => p1,
            Self::Cubic(c) => c.p3,
        }
    }
}

fn normalize_or_zero(v: Vec2) -> Vec2 {
    let len = v.hypot();
    if len > EPS_LEN {
        v / len
    } else {
        Vec2::ZERO
    }
}

fn append_seg_to_bezpath(bp: &mut BezPath, seg: SegGeom, need_move: bool) {
    if need_move {
        bp.move_to(seg.start_point());
    }
    match seg {
        SegGeom::Line(_, p1) => bp.line_to(p1),
        SegGeom::Cubic(c) => bp.curve_to(c.p1, c.p2, c.p3),
    }
}

/// Applies true cubic Bézier Live Corners (or chamfer cuts) to sharp vertices.
pub fn apply_corners(contours: &[Contour], radius: f64, chamfer: bool) -> Vec<Contour> {
    if radius <= 1e-5 || contours.is_empty() {
        return contours.to_vec();
    }

    let mut out = Vec::with_capacity(contours.len());
    for contour in contours {
        let n = contour.anchors.len();
        if n < 3 {
            out.push(contour.clone());
            continue;
        }

        let seg_count = if contour.closed { n } else { n - 1 };
        let mut segs = Vec::with_capacity(seg_count);
        let mut lens = Vec::with_capacity(seg_count);
        for i in 0..seg_count {
            let a = &contour.anchors[i];
            let b = &contour.anchors[(i + 1) % n];
            let seg = SegGeom::from_anchors(a, b);
            lens.push(seg.arclen());
            segs.push(seg);
        }

        // Compute trim distance d_corner[i] at vertex i (where segs[i-1] meets segs[i]).
        let mut corner_trim = vec![0.0_f64; n];
        let mut corner_handle = vec![0.0_f64; n];
        let mut corner_tangents = vec![(Vec2::ZERO, Vec2::ZERO); n];

        for i in 0..n {
            if !contour.closed && (i == 0 || i == n - 1) {
                continue;
            }
            if contour.anchors[i].smooth {
                continue;
            }
            let in_idx = if i == 0 { seg_count - 1 } else { i - 1 };
            let out_idx = i;
            let len_in = lens[in_idx];
            let len_out = lens[out_idx];
            if len_in <= 1e-4 || len_out <= 1e-4 {
                continue;
            }

            let t_in = segs[in_idx].end_tangent();
            let t_out = segs[out_idx].start_tangent();
            if t_in.hypot() <= 0.5 || t_out.hypot() <= 0.5 {
                continue;
            }
            // Vector u points back along incoming edge; v points forward along outgoing edge.
            let u = -t_in;
            let v = t_out;
            let cos_alpha = u.dot(v).clamp(-1.0, 1.0);
            // Skip nearly straight (alpha near PI) or cusp reversals (alpha near 0).
            if cos_alpha < -0.998 || cos_alpha > 0.998 {
                continue;
            }

            let d = radius.min(0.48 * len_in).min(0.48 * len_out);
            if d <= 1e-4 {
                continue;
            }

            let handle_len = if cos_alpha.abs() < 1e-6 {
                KAPPA * d
            } else {
                let alpha = cos_alpha.acos();
                let theta = PI - alpha;
                (4.0 / 3.0) * d * (0.5 * alpha).tan() * (0.25 * theta).tan()
            };

            corner_trim[i] = d;
            corner_handle[i] = handle_len;
            corner_tangents[i] = (t_in, t_out);
        }

        let mut bp = BezPath::new();
        let mut started = false;

        for i in 0..seg_count {
            let next_v = (i + 1) % n;
            let trim_start = corner_trim[i];
            let trim_end = corner_trim[next_v];
            let len = lens[i];

            let t0 = if trim_start > 0.0 {
                segs[i].param_at_arclen(trim_start, len)
            } else {
                0.0
            };
            let t1 = if trim_end > 0.0 {
                segs[i].param_at_arclen((len - trim_end).max(0.0), len)
            } else {
                1.0
            };

            let trimmed = segs[i].subsegment(t0, t1.max(t0));
            if !started {
                bp.move_to(trimmed.start_point());
                started = true;
            }
            if t1 - t0 > 1e-6 {
                append_seg_to_bezpath(&mut bp, trimmed, false);
            }

            // Insert corner fillet/chamfer at vertex next_v if it was trimmed.
            if trim_end > 0.0 {
                let next_seg_len = lens[next_v];
                let t_next_start = segs[next_v].param_at_arclen(trim_end, next_seg_len);
                let p_in = trimmed.end_point();
                let p_out = segs[next_v].subsegment(t_next_start, 1.0).start_point();
                if chamfer {
                    bp.line_to(p_out);
                } else {
                    let (t_in, t_out) = corner_tangents[next_v];
                    let hl = corner_handle[next_v];
                    let c1 = p_in + t_in * hl;
                    let c2 = p_out - t_out * hl;
                    bp.curve_to(c1, c2, p_out);
                }
            }
        }

        if contour.closed && started {
            bp.close_path();
        }

        let mut converted = Contour::from_bezpath(&bp);
        for c in &mut converted {
            c.closed = contour.closed;
        }
        out.extend(converted);
    }

    out
}

fn combine_closed_bezpath(contours: &[Contour]) -> Option<BezPath> {
    let mut combined = BezPath::new();
    let mut has_any = false;
    for c in contours {
        if c.closed && is_contour_finite_and_valid(c) {
            combined.extend(c.to_bezpath().elements().iter().copied());
            has_any = true;
        }
    }
    has_any.then_some(combined)
}

/// Expands (`delta > 0`) or erodes (`delta < 0`) closed outlines via `kurbo::stroke`
/// combined with `linesweeper` Union / Difference and overlap removal.
pub fn offset_outline(contours: &[Contour], delta: f64, round_joins: bool) -> Vec<Contour> {
    if delta.abs() <= 1e-5 || contours.is_empty() {
        return contours.to_vec();
    }

    let cleaned = remove_overlap(contours);
    let Some(orig_path) = combine_closed_bezpath(&cleaned) else {
        return contours.to_vec();
    };

    let join = if round_joins {
        Join::Round
    } else {
        Join::Miter
    };
    let style = Stroke::new(delta.abs() * 2.0)
        .with_join(join)
        .with_miter_limit(4.0);
    let stroke_path = kurbo::stroke(
        orig_path.elements().iter().copied(),
        &style,
        &StrokeOpts::default(),
        0.25,
    );

    let op = if delta > 0.0 {
        BinaryOp::Union
    } else {
        BinaryOp::Difference
    };

    match binary_op(&orig_path, &stroke_path, FillRule::NonZero, op) {
        Ok(ls_contours) => {
            let model_contours = linesweeper_contours_to_model(ls_contours);
            if delta > 0.0 && model_contours.is_empty() {
                cleaned
            } else {
                remove_overlap(&model_contours)
            }
        }
        Err(_) => cleaned,
    }
}

/// Expands open (or closed) skeleton centerlines into closed outlines via `kurbo::stroke`.
pub fn outline_stroke(centerlines: &[Contour], width: f64, round_caps: bool) -> Vec<Contour> {
    if width <= 1e-5 || centerlines.is_empty() {
        return Vec::new();
    }

    let mut path = BezPath::new();
    for c in centerlines {
        if c.anchors.len() >= 2 {
            path.extend(c.to_bezpath().elements().iter().copied());
        }
    }
    if path.elements().is_empty() {
        return Vec::new();
    }

    let (cap, join) = if round_caps {
        (Cap::Round, Join::Round)
    } else {
        (Cap::Butt, Join::Miter)
    };
    let style = Stroke::new(width)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(4.0);
    let stroked = kurbo::stroke(
        path.elements().iter().copied(),
        &style,
        &StrokeOpts::default(),
        0.25,
    );

    let mut contours = Contour::from_bezpath(&stroked);
    for c in &mut contours {
        c.closed = true;
    }
    remove_overlap(&contours)
}

#[derive(Clone, Copy)]
struct NibParams {
    a: f64,
    b: f64,
    u: Vec2,
    v: Vec2,
}

fn elliptical_nib_support(d: Vec2, nib: &NibParams) -> Vec2 {
    let ud = nib.u.dot(d);
    let vd = nib.v.dot(d);
    let denom = (nib.a * nib.a * ud * ud + nib.b * nib.b * vd * vd).sqrt();
    if denom <= 1e-9 {
        return Vec2::ZERO;
    }
    (nib.u * (nib.a * nib.a * ud) + nib.v * (nib.b * nib.b * vd)) / denom
}

fn sample_contour_centerline(contour: &Contour) -> Vec<(Point, Vec2)> {
    let bp = contour.to_bezpath();
    let mut samples: Vec<(Point, Vec2)> = Vec::new();

    for seg in bp.segments() {
        let (steps, p_start, p_end) = match seg {
            PathSeg::Line(l) => (1usize, l.p0, l.p1),
            PathSeg::Cubic(c) => {
                let len = c.arclen(0.25);
                let n = ((len / 6.0).ceil() as usize).clamp(8, 64);
                (n, c.p0, c.p3)
            }
            PathSeg::Quad(q) => {
                let len = q.arclen(0.25);
                let n = ((len / 6.0).ceil() as usize).clamp(6, 48);
                (n, q.p0, q.p2)
            }
        };
        if (p_end - p_start).hypot() <= 1e-6 && matches!(seg, PathSeg::Line(_)) {
            continue;
        }
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let pt = seg.eval(t);
            let tan = match seg {
                PathSeg::Line(l) => normalize_or_zero(l.p1 - l.p0),
                PathSeg::Quad(q) => normalize_or_zero(q.deriv().eval(t).to_vec2()),
                PathSeg::Cubic(c) => {
                    let d = c.deriv().eval(t).to_vec2();
                    if d.hypot() > EPS_LEN {
                        normalize_or_zero(d)
                    } else {
                        let t_fallback = if t < 0.5 { 0.05 } else { 0.95 };
                        let df = c.deriv().eval(t_fallback).to_vec2();
                        if df.hypot() > EPS_LEN {
                            normalize_or_zero(df)
                        } else {
                            normalize_or_zero(c.p3 - c.p0)
                        }
                    }
                }
            };
            if tan.hypot() <= 0.5 {
                continue;
            }
            let duplicate = samples.last().is_some_and(|(prev_p, prev_t)| {
                (*prev_p - pt).hypot() < 1e-4 && prev_t.dot(tan) > 0.999
            });
            if !duplicate {
                samples.push((pt, tan));
            }
        }
    }

    samples
}

fn append_cap_arc(
    pts: &mut Vec<Point>,
    center: Point,
    from_dir: Vec2,
    to_dir: Vec2,
    nib: &NibParams,
) {
    let a0 = from_dir.y.atan2(from_dir.x);
    let mut a1 = to_dir.y.atan2(to_dir.x);
    while a1 - a0 > PI {
        a1 -= 2.0 * PI;
    }
    while a1 - a0 < -PI {
        a1 += 2.0 * PI;
    }
    let steps = 6usize;
    for i in 1..steps {
        let ang = a0 + (a1 - a0) * (i as f64 / steps as f64);
        let dir = Vec2::new(ang.cos(), ang.sin());
        let sup = elliptical_nib_support(dir, nib);
        pts.push(center + sup);
    }
}

/// Sweeps an analytical elliptical calligraphic nib along centerline curves.
pub fn calligraphic_sweep(
    centerlines: &[Contour],
    size: f64,
    angle_deg: f64,
    roundness: f64,
) -> Vec<Contour> {
    if size <= 1e-4 || centerlines.is_empty() {
        return Vec::new();
    }

    let a = (size * 0.5).max(0.5);
    let b = (a * roundness.clamp(0.05, 1.0)).max(0.25);
    let theta = angle_deg.to_radians();
    let nib = NibParams {
        a,
        b,
        u: Vec2::new(theta.cos(), theta.sin()),
        v: Vec2::new(-theta.sin(), theta.cos()),
    };

    let mut raw_contours = Vec::new();

    for cl in centerlines {
        let samples = sample_contour_centerline(cl);
        if samples.len() < 2 {
            continue;
        }

        let mut left_pts = Vec::with_capacity(samples.len());
        let mut right_pts = Vec::with_capacity(samples.len());
        for &(pt, tan) in &samples {
            let left_n = Vec2::new(-tan.y, tan.x);
            let sup = elliptical_nib_support(left_n, &nib);
            left_pts.push(pt + sup);
            right_pts.push(pt - sup);
        }

        if cl.closed {
            let left_anchors: Vec<Anchor> = left_pts
                .into_iter()
                .map(|p| Anchor::on_curve(p.x, p.y))
                .collect();
            let right_anchors: Vec<Anchor> = right_pts
                .into_iter()
                .rev()
                .map(|p| Anchor::on_curve(p.x, p.y))
                .collect();
            raw_contours.push(Contour {
                anchors: left_anchors,
                closed: true,
            });
            raw_contours.push(Contour {
                anchors: right_anchors,
                closed: true,
            });
        } else {
            let mut loop_pts = Vec::with_capacity(left_pts.len() + right_pts.len() + 12);
            loop_pts.extend_from_slice(&left_pts);

            if let Some(&(end_pt, end_tan)) = samples.last() {
                let d_left = Vec2::new(-end_tan.y, end_tan.x);
                let d_right = -d_left;
                append_cap_arc(&mut loop_pts, end_pt, d_left, d_right, &nib);
            }

            for p in right_pts.into_iter().rev() {
                loop_pts.push(p);
            }

            if let Some(&(start_pt, start_tan)) = samples.first() {
                let d_left = Vec2::new(-start_tan.y, start_tan.x);
                let d_right = -d_left;
                append_cap_arc(&mut loop_pts, start_pt, d_right, d_left, &nib);
            }

            let anchors: Vec<Anchor> = loop_pts
                .into_iter()
                .map(|p| Anchor::on_curve(p.x, p.y))
                .collect();
            raw_contours.push(Contour {
                anchors,
                closed: true,
            });
        }
    }

    let simplified = simplify_contours(&raw_contours, 0.25);
    remove_overlap(&simplified)
}

/// Splits cubic Bézier segments at interior horizontal and vertical extrema.
pub fn add_extrema(contours: &[Contour]) -> Vec<Contour> {
    let mut out = Vec::with_capacity(contours.len());
    for contour in contours {
        let bp = contour.to_bezpath();
        let mut new_bp = BezPath::new();
        for el in bp.elements() {
            match *el {
                PathEl::MoveTo(p) => new_bp.move_to(p),
                PathEl::LineTo(p) => new_bp.line_to(p),
                PathEl::QuadTo(q1, p) => {
                    let cur = new_bp.current_position().unwrap_or(Point::ORIGIN);
                    let cubic = kurbo::QuadBez::new(cur, q1, p).raise();
                    split_cubic_at_extrema(&mut new_bp, cubic);
                }
                PathEl::CurveTo(c1, c2, p) => {
                    let cur = new_bp.current_position().unwrap_or(Point::ORIGIN);
                    let cubic = CubicBez::new(cur, c1, c2, p);
                    split_cubic_at_extrema(&mut new_bp, cubic);
                }
                PathEl::ClosePath => new_bp.close_path(),
            }
        }
        let mut converted = Contour::from_bezpath(&new_bp);
        for c in &mut converted {
            c.closed = contour.closed;
        }
        out.extend(converted);
    }
    out
}

fn split_cubic_at_extrema(bp: &mut BezPath, cubic: CubicBez) {
    let mut ts: Vec<f64> = cubic
        .extrema()
        .into_iter()
        .filter(|&t| t > 1e-4 && t < 1.0 - 1e-4)
        .collect();
    ts.sort_by(|a, b| a.total_cmp(b));
    ts.dedup_by(|a, b| (*a - *b).abs() < 1e-4);

    if ts.is_empty() {
        bp.curve_to(cubic.p1, cubic.p2, cubic.p3);
        return;
    }

    let mut prev_t = 0.0;
    for t in ts {
        let sub = cubic.subsegment(prev_t..t);
        bp.curve_to(sub.p1, sub.p2, sub.p3);
        prev_t = t;
    }
    let tail = cubic.subsegment(prev_t..1.0);
    bp.curve_to(tail.p1, tail.p2, tail.p3);
}

fn reverse_contour(contour: &Contour) -> Contour {
    let mut anchors: Vec<Anchor> = contour
        .anchors
        .iter()
        .rev()
        .map(|a| Anchor {
            x: a.x,
            y: a.y,
            h_in: a.h_out,
            h_out: a.h_in,
            smooth: a.smooth,
        })
        .collect();
    // Preserve starting anchor identity for closed contours if helpful.
    if contour.closed && anchors.len() > 1 {
        anchors.rotate_right(1);
    }
    Contour {
        anchors,
        closed: contour.closed,
    }
}

fn pick_interior_test_point(contour: &Contour, bp: &BezPath) -> Option<Point> {
    for seg in bp.segments() {
        let mid = seg.eval(0.5);
        let d = match seg {
            PathSeg::Line(l) => l.p1 - l.p0,
            PathSeg::Quad(q) => q.deriv().eval(0.5).to_vec2(),
            PathSeg::Cubic(c) => c.deriv().eval(0.5).to_vec2(),
        };
        let len = d.hypot();
        if len > 1e-4 {
            let n = Vec2::new(-d.y / len, d.x / len);
            let bb = bp.bounding_box();
            let step = (bb.width().min(bb.height()) * 0.01).clamp(1e-3, 1.0);
            for sign in [1.0, -1.0] {
                let candidate = mid + n * (sign * step);
                if bp.winding(candidate) != 0 {
                    return Some(candidate);
                }
            }
        }
    }
    contour.anchors.first().map(Anchor::point)
}

/// Normalizes winding direction by containment depth:
/// when `truetype_cw_outer` is true, outer contours (even depth) are CW in Y-up
/// (negative signed area) and holes (odd depth) are CCW (positive signed area).
pub fn normalize_winding(contours: &[Contour], truetype_cw_outer: bool) -> Vec<Contour> {
    if contours.is_empty() {
        return Vec::new();
    }

    let paths: Vec<Option<(BezPath, f64)>> = contours
        .iter()
        .map(|c| {
            if c.closed && c.anchors.len() >= 2 {
                let bp = c.to_bezpath();
                let area = bp.area();
                Some((bp, area))
            } else {
                None
            }
        })
        .collect();

    let mut out = Vec::with_capacity(contours.len());
    for (i, contour) in contours.iter().enumerate() {
        let Some((ref bp_i, area_i)) = paths[i] else {
            out.push(contour.clone());
            continue;
        };
        if area_i.abs() <= 1e-9 {
            out.push(contour.clone());
            continue;
        }

        let test_pt = pick_interior_test_point(contour, bp_i);
        let mut depth = 0usize;
        if let Some(pt) = test_pt {
            for (j, other) in paths.iter().enumerate() {
                if i == j {
                    continue;
                }
                let Some((ref bp_j, area_j)) = *other else {
                    continue;
                };
                if area_j.abs() > area_i.abs() + 1e-4 && bp_j.winding(pt) != 0 {
                    depth += 1;
                }
            }
        }

        let is_outer = depth.is_multiple_of(2);
        let want_negative = if truetype_cw_outer {
            is_outer
        } else {
            !is_outer
        };
        let needs_flip = if want_negative {
            area_i > 0.0
        } else {
            area_i < 0.0
        };

        if needs_flip {
            out.push(reverse_contour(contour));
        } else {
            out.push(contour.clone());
        }
    }
    out
}

fn point_line_dist_if_between(p: Point, a: Point, b: Point, tol: f64) -> bool {
    let ab = b - a;
    let ab_len2 = ab.hypot2();
    if ab_len2 <= 1e-12 {
        return (p - a).hypot() <= tol;
    }
    let ab_len = ab_len2.sqrt();
    let ap = p - a;
    let proj = ap.dot(ab) / ab_len2;
    if proj < -1e-4 || proj > 1.0 + 1e-4 {
        return false;
    }
    let perp = ap.cross(ab).abs() / ab_len;
    perp <= tol
}

/// Drops coincident duplicate anchors and collinear straight vertices within `tolerance`.
pub fn simplify_contours(contours: &[Contour], tolerance: f64) -> Vec<Contour> {
    let tol = tolerance.max(0.0);
    let mut out = Vec::with_capacity(contours.len());

    for contour in contours {
        if contour.anchors.is_empty() {
            continue;
        }
        let mut anchors = contour.anchors.clone();

        // 1. Remove consecutive coincident anchors.
        let mut deduped: Vec<Anchor> = Vec::with_capacity(anchors.len());
        for a in anchors.drain(..) {
            if let Some(prev) = deduped.last_mut() {
                let dist = (a.point() - prev.point()).hypot();
                if dist <= 1e-4 && !prev.has_h_out() && !a.has_h_in() {
                    prev.h_out = a.h_out;
                    prev.smooth = a.smooth;
                    continue;
                }
            }
            deduped.push(a);
        }
        if contour.closed && deduped.len() > 2 {
            let same_ends = {
                let first = &deduped[0];
                let last = &deduped[deduped.len() - 1];
                (first.point() - last.point()).hypot() <= 1e-4
                    && !last.has_h_out()
                    && !first.has_h_in()
            };
            if same_ends {
                if let Some(last) = deduped.pop() {
                    deduped[0].h_in = last.h_in;
                }
            }
        }

        // 2. Remove collinear straight-line vertices within tolerance.
        if tol > 0.0 && deduped.len() >= 3 {
            let mut changed = true;
            while changed && deduped.len() >= 3 {
                changed = false;
                let len = deduped.len();
                let mut remove_idx = None;
                for i in 0..len {
                    if !contour.closed && (i == 0 || i == len - 1) {
                        continue;
                    }
                    let prev = &deduped[(i + len - 1) % len];
                    let curr = &deduped[i];
                    let next = &deduped[(i + 1) % len];
                    if !prev.has_h_out()
                        && !curr.has_h_in()
                        && !curr.has_h_out()
                        && !next.has_h_in()
                        && point_line_dist_if_between(curr.point(), prev.point(), next.point(), tol)
                    {
                        remove_idx = Some(i);
                        break;
                    }
                }
                if let Some(idx) = remove_idx {
                    deduped.remove(idx);
                    changed = true;
                }
            }
        }

        if !deduped.is_empty() {
            out.push(Contour {
                anchors: deduped,
                closed: contour.closed,
            });
        }
    }

    out
}

/// Applies a single `GlyphTransformOp` to a glyph's contours.
pub fn apply_transform(contours: &[Contour], op: &GlyphTransformOp) -> Vec<Contour> {
    match *op {
        GlyphTransformOp::RemoveOverlap => remove_overlap(contours),
        GlyphTransformOp::Offset { delta, round_joins } => {
            offset_outline(contours, delta, round_joins)
        }
        GlyphTransformOp::RoundCorners { radius, chamfer } => {
            apply_corners(contours, radius, chamfer)
        }
        GlyphTransformOp::AddExtrema => add_extrema(contours),
        GlyphTransformOp::Simplify { tolerance } => simplify_contours(contours, tolerance),
        GlyphTransformOp::NormalizeWinding => normalize_winding(contours, true),
        GlyphTransformOp::CalligraphicSweep {
            size,
            angle_deg,
            roundness,
        } => calligraphic_sweep(contours, size, angle_deg, roundness),
    }
}
