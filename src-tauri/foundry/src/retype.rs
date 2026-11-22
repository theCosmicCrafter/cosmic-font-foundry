use std::collections::HashSet;

use kurbo::{flatten, BezPath, PathEl, Point};
use serde::{Deserialize, Serialize};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

use crate::model::Contour;

const PRIMARY_SAMPLES: usize = 64;
const SECONDARY_SAMPLES: usize = 32;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VisualMatchHit {
    pub family: String,
    pub path: String,
    pub matched_char: String,
    pub score: f32,
    pub distance: f32,
}

#[derive(Default)]
struct BezPathPen(BezPath);

impl OutlinePen for BezPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0
            .quad_to((cx0 as f64, cy0 as f64), (x as f64, y as f64));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (cx0 as f64, cy0 as f64),
            (cx1 as f64, cy1 as f64),
            (x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}

struct OutlineSignature {
    primary: Vec<[f64; 2]>,
    secondary: Option<Vec<[f64; 2]>>,
    aspect: f64,
}

fn poly_abs_area(pts: &[Point]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    0.5 * sum.abs()
}

fn resample_closed_poly(pts: &[Point], count: usize) -> Option<Vec<Point>> {
    if pts.len() < 2 || count == 0 {
        return None;
    }
    let n = pts.len();
    let mut cum = Vec::with_capacity(n + 1);
    cum.push(0.0);
    let mut total = 0.0;
    for i in 0..n {
        total += (pts[(i + 1) % n] - pts[i]).hypot();
        cum.push(total);
    }
    if total <= 1e-6 || !total.is_finite() {
        return None;
    }

    let mut out = Vec::with_capacity(count);
    let mut seg_idx = 0usize;
    for k in 0..count {
        let target = total * (k as f64 / count as f64);
        while seg_idx + 1 < n && cum[seg_idx + 1] < target {
            seg_idx += 1;
        }
        let s0 = cum[seg_idx];
        let s1 = cum[seg_idx + 1];
        let seg_len = s1 - s0;
        let t = if seg_len > 1e-9 {
            ((target - s0) / seg_len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push(pts[seg_idx].lerp(pts[(seg_idx + 1) % n], t));
    }
    Some(out)
}

fn extract_subpaths(bp: &BezPath) -> Vec<Vec<Point>> {
    let mut subpaths: Vec<Vec<Point>> = Vec::new();
    let mut current: Vec<Point> = Vec::new();

    flatten(bp.elements().iter().copied(), 0.75, |el| match el {
        PathEl::MoveTo(p) => {
            if current.len() >= 3 {
                subpaths.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            current.push(p);
        }
        PathEl::LineTo(p) => {
            if current.last().is_none_or(|prev| (*prev - p).hypot() > 1e-4) {
                current.push(p);
            }
        }
        PathEl::ClosePath => {
            if current.len() >= 2 {
                if let (Some(&first), Some(&last)) = (current.first(), current.last()) {
                    if (first - last).hypot() <= 1e-4 {
                        current.pop();
                    }
                }
            }
            if current.len() >= 3 {
                subpaths.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
        PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => {}
    });

    if current.len() >= 3 {
        subpaths.push(current);
    }
    subpaths
}

fn build_signature_from_bezpath(bp: &BezPath) -> Option<OutlineSignature> {
    let mut subpaths = extract_subpaths(bp);
    if subpaths.is_empty() {
        return None;
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for sp in &subpaths {
        for p in sp {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }
    let width = (max_x - min_x).max(1e-3);
    let height = (max_y - min_y).max(1e-3);
    let aspect = (width / height).clamp(0.05, 20.0);

    subpaths.sort_by(|a, b| poly_abs_area(b).total_cmp(&poly_abs_area(a)));
    let primary_area = poly_abs_area(&subpaths[0]);
    if primary_area <= 1e-4 {
        return None;
    }

    let resampled_primary = resample_closed_poly(&subpaths[0], PRIMARY_SAMPLES)?;
    let n = resampled_primary.len() as f64;
    let cx = resampled_primary.iter().map(|p| p.x).sum::<f64>() / n;
    let cy = resampled_primary.iter().map(|p| p.y).sum::<f64>() / n;

    let rms_sq = resampled_primary
        .iter()
        .map(|p| {
            let dx = p.x - cx;
            let dy = p.y - cy;
            dx * dx + dy * dy
        })
        .sum::<f64>()
        / n;
    let rms = rms_sq.sqrt();
    if rms <= 1e-6 || !rms.is_finite() {
        return None;
    }

    let primary: Vec<[f64; 2]> = resampled_primary
        .into_iter()
        .map(|p| [(p.x - cx) / rms, (p.y - cy) / rms])
        .collect();

    let secondary = if subpaths.len() >= 2 && poly_abs_area(&subpaths[1]) > primary_area * 0.03 {
        resample_closed_poly(&subpaths[1], SECONDARY_SAMPLES).map(|pts| {
            pts.into_iter()
                .map(|p| [(p.x - cx) / rms, (p.y - cy) / rms])
                .collect()
        })
    } else {
        None
    };

    Some(OutlineSignature {
        primary,
        secondary,
        aspect,
    })
}

fn circular_rmsd(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let n = a.len();
    if n == 0 || n != b.len() {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;

    for reversed in [false, true] {
        for shift in 0..n {
            let mut sum_sq = 0.0;
            for (i, pt_a) in a.iter().enumerate() {
                let bi = if reversed {
                    (n + shift - i) % n
                } else {
                    (i + shift) % n
                };
                let dx = pt_a[0] - b[bi][0];
                let dy = pt_a[1] - b[bi][1];
                sum_sq += dx * dx + dy * dy;
            }
            let rmsd = (sum_sq / n as f64).sqrt();
            if rmsd < best {
                best = rmsd;
            }
        }
    }
    best
}

fn signature_distance(query: &OutlineSignature, cand: &OutlineSignature) -> f64 {
    let aspect_penalty = (query.aspect / cand.aspect).ln().abs() * 0.35;
    let mut best = f64::INFINITY;

    for y_sign in [1.0_f64, -1.0_f64] {
        let q_prim: Vec<[f64; 2]> = query
            .primary
            .iter()
            .map(|&[x, y]| [x, y * y_sign])
            .collect();
        let prim_dist = circular_rmsd(&q_prim, &cand.primary);

        let sec_penalty = match (&query.secondary, &cand.secondary) {
            (Some(qs), Some(cs)) => {
                let q_sec: Vec<[f64; 2]> = qs.iter().map(|&[x, y]| [x, y * y_sign]).collect();
                circular_rmsd(&q_sec, cs) * 0.30
            }
            (None, None) => 0.0,
            _ => 0.25,
        };

        let total = prim_dist + sec_penalty + aspect_penalty;
        if total < best {
            best = total;
        }
    }

    best
}

fn contours_to_bezpath(contours: &[Contour]) -> BezPath {
    let mut bp = BezPath::new();
    for c in contours {
        if c.anchors.len() < 2 {
            continue;
        }
        bp.extend(c.to_bezpath().elements().iter().copied());
    }
    bp
}

/// Matches query contours against a list of candidate font files `(family, path)`
/// using centroid/RMS-normalized Procrustes polyline distance across `sample_chars`.
pub fn match_outline_against_fonts(
    query_contours: &[Contour],
    candidates: &[(String, String)],
    sample_chars: &str,
    top_k: usize,
) -> Vec<VisualMatchHit> {
    if query_contours.is_empty() || candidates.is_empty() {
        return Vec::new();
    }

    let query_bp = contours_to_bezpath(query_contours);
    let Some(query_sig) = build_signature_from_bezpath(&query_bp) else {
        return Vec::new();
    };

    let chars_raw = if sample_chars.trim().is_empty() {
        "AHMNOORSTaeon"
    } else {
        sample_chars
    };
    let mut seen_chars = HashSet::new();
    let probe_chars: Vec<char> = chars_raw
        .chars()
        .filter(|c| !c.is_whitespace() && seen_chars.insert(*c))
        .collect();
    if probe_chars.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::with_capacity(candidates.len());

    for (family, path) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(font) = FontRef::new(&bytes) else {
            continue;
        };
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();

        let mut best_for_font: Option<(char, f64)> = None;
        for &ch in &probe_chars {
            let Some(gid) = charmap.map(ch) else {
                continue;
            };
            let Some(glyph) = outlines.get(gid) else {
                continue;
            };
            let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
            let mut pen = BezPathPen::default();
            if glyph.draw(settings, &mut pen).is_err() {
                continue;
            }
            let Some(cand_sig) = build_signature_from_bezpath(&pen.0) else {
                continue;
            };
            let dist = signature_distance(&query_sig, &cand_sig);
            if dist.is_finite() {
                match best_for_font {
                    Some((_, best_d)) if dist >= best_d => {}
                    _ => best_for_font = Some((ch, dist)),
                }
            }
        }

        if let Some((matched_char, dist)) = best_for_font {
            let score = (100.0 * (-dist * 2.2).exp()).clamp(0.0, 100.0) as f32;
            hits.push(VisualMatchHit {
                family: family.clone(),
                path: path.clone(),
                matched_char: matched_char.to_string(),
                score,
                distance: dist as f32,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.distance.total_cmp(&b.distance))
            .then_with(|| a.family.cmp(&b.family))
    });
    hits.truncate(top_k.max(1));
    hits
}
