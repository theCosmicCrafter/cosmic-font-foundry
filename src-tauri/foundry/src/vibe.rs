use std::collections::{BTreeSet, HashSet};

use kurbo::{flatten, BezPath, PathEl, Point};
use serde::{Deserialize, Serialize};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

const UPPER_PROBES: [char; 9] = ['H', 'O', 'A', 'E', 'I', 'M', 'W', 'C', 'S'];
const LOWER_PROBES: [char; 7] = ['n', 'o', 'e', 'c', 'a', 'l', 'g'];
const MONO_PROBES: [char; 6] = ['i', 'l', 'I', 'm', 'M', 'W'];
const SERIF_PROBES: [char; 5] = ['I', 'H', 'E', 'T', 'L'];
const OPEN_PROBES: [char; 7] = ['c', 'e', 'C', 'o', 'O', 'S', 'a'];
const SLANT_PROBES: [char; 5] = ['H', 'I', 'E', 'l', 'A'];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TypographicDna {
    pub weight: f32,
    pub width: f32,
    pub contrast: f32,
    pub x_height: f32,
    pub slant: f32,
    pub roundness: f32,
    pub roughness: f32,
    pub serifness: f32,
    pub monospace: f32,
    pub openness: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VibeMatchHit {
    pub family: String,
    pub path: String,
    pub score: f32,
    pub matched_traits: Vec<String>,
    pub dna: TypographicDna,
}

struct ScaledBezPen {
    scale: f64,
    path: BezPath,
    line_segs: usize,
    curve_segs: usize,
    short_segs: usize,
    last_pt: Point,
}

impl ScaledBezPen {
    fn new(scale: f64) -> Self {
        Self {
            scale,
            path: BezPath::new(),
            line_segs: 0,
            curve_segs: 0,
            short_segs: 0,
            last_pt: Point::ZERO,
        }
    }

    fn pt(&self, x: f32, y: f32) -> Point {
        Point::new((x as f64) * self.scale, (y as f64) * self.scale)
    }
}

impl OutlinePen for ScaledBezPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.path.move_to(p);
        self.last_pt = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        if (p - self.last_pt).hypot() < 15.0 {
            self.short_segs += 1;
        }
        self.path.line_to(p);
        self.last_pt = p;
        self.line_segs += 1;
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let c = self.pt(cx0, cy0);
        let p = self.pt(x, y);
        if (p - self.last_pt).hypot() < 15.0 {
            self.short_segs += 1;
        }
        self.path.quad_to(c, p);
        self.last_pt = p;
        self.curve_segs += 1;
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let c0 = self.pt(cx0, cy0);
        let c1 = self.pt(cx1, cy1);
        let p = self.pt(x, y);
        if (p - self.last_pt).hypot() < 15.0 {
            self.short_segs += 1;
        }
        self.path.curve_to(c0, c1, p);
        self.last_pt = p;
        self.curve_segs += 1;
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

struct DrawnGlyph {
    ch: Option<char>,
    is_upper: bool,
    is_lower: bool,
    subpaths: Vec<Vec<Point>>,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
    ink_area: f64,
    perimeter: f64,
    line_segs: usize,
    curve_segs: usize,
    short_segs: usize,
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

fn signed_area(pts: &[Point]) -> f64 {
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
    0.5 * sum
}

fn poly_perimeter(pts: &[Point]) -> f64 {
    let n = pts.len();
    if n < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for i in 0..n {
        total += (pts[(i + 1) % n] - pts[i]).hypot();
    }
    total
}

fn draw_probe_glyph(
    font: &FontRef<'_>,
    gid: GlyphId,
    ch: Option<char>,
    scale: f64,
) -> Option<DrawnGlyph> {
    let glyph = font.outline_glyphs().get(gid)?;
    let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
    let mut pen = ScaledBezPen::new(scale);
    if glyph.draw(settings, &mut pen).is_err() {
        return None;
    }
    let subpaths = extract_subpaths(&pen.path);
    if subpaths.is_empty() {
        return None;
    }

    let mut xmin = f64::INFINITY;
    let mut ymin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    let mut signed_sum = 0.0;
    let mut abs_fallback = 0.0;
    let mut perimeter = 0.0;

    for sp in &subpaths {
        for p in sp {
            if p.x.is_finite() && p.y.is_finite() {
                xmin = xmin.min(p.x);
                ymin = ymin.min(p.y);
                xmax = xmax.max(p.x);
                ymax = ymax.max(p.y);
            }
        }
        let sa = signed_area(sp);
        signed_sum += sa;
        abs_fallback += sa.abs();
        perimeter += poly_perimeter(sp);
    }

    if !xmin.is_finite() || !xmax.is_finite() || xmax <= xmin || ymax <= ymin {
        return None;
    }

    let ink_area = if signed_sum.abs() > 1.0 {
        signed_sum.abs()
    } else {
        abs_fallback
    };

    let is_upper = ch.is_some_and(|c| c.is_ascii_uppercase());
    let is_lower = ch.is_some_and(|c| c.is_ascii_lowercase());

    Some(DrawnGlyph {
        ch,
        is_upper,
        is_lower,
        subpaths,
        xmin,
        ymin,
        xmax,
        ymax,
        ink_area,
        perimeter,
        line_segs: pen.line_segs,
        curve_segs: pen.curve_segs,
        short_segs: pen.short_segs,
    })
}

fn horizontal_scan_runs(subpaths: &[Vec<Point>], y: f64) -> Vec<(f64, f64)> {
    let mut xs = Vec::new();
    for sp in subpaths {
        let n = sp.len();
        for i in 0..n {
            let p0 = sp[i];
            let p1 = sp[(i + 1) % n];
            if (p0.y <= y && p1.y > y) || (p1.y <= y && p0.y > y) {
                let t = (y - p0.y) / (p1.y - p0.y);
                xs.push(p0.x + t * (p1.x - p0.x));
            }
        }
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let mut runs = Vec::new();
    for &[x0, x1] in xs.as_chunks::<2>().0 {
        let len = x1 - x0;
        if len > 0.5 && len.is_finite() {
            runs.push((x0, x1));
        }
    }
    runs
}

fn vertical_scan_runs(subpaths: &[Vec<Point>], x: f64) -> Vec<(f64, f64)> {
    let mut ys = Vec::new();
    for sp in subpaths {
        let n = sp.len();
        for i in 0..n {
            let p0 = sp[i];
            let p1 = sp[(i + 1) % n];
            if (p0.x <= x && p1.x > x) || (p1.x <= x && p0.x > x) {
                let t = (x - p0.x) / (p1.x - p0.x);
                ys.push(p0.y + t * (p1.y - p0.y));
            }
        }
    }
    ys.sort_by(|a, b| a.total_cmp(b));
    let mut runs = Vec::new();
    for &[y0, y1] in ys.as_chunks::<2>().0 {
        let len = y1 - y0;
        if len > 0.5 && len.is_finite() {
            runs.push((y0, y1));
        }
    }
    runs
}

fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
    sorted.get(idx).copied()
}

fn clamp01(val: f64) -> f32 {
    if val.is_finite() {
        val.clamp(0.0, 1.0) as f32
    } else {
        0.5
    }
}

/// Extracts a normalized 10-axis `TypographicDna` vector from raw SFNT font bytes
/// using `OS/2`/`post`/`hmtx` metadata combined with pure-vector scanline & contour analysis.
pub fn extract_dna_from_bytes(bytes: &[u8]) -> Result<TypographicDna, String> {
    let font = FontRef::from_index(bytes, 0)
        .map_err(|e| format!("failed to parse font for DNA extraction: {e}"))?;

    let upm = font
        .head()
        .map(|h| h.units_per_em())
        .unwrap_or_else(|_| {
            font.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
        })
        .max(1) as f64;
    let scale = 1000.0 / upm;

    let charmap = font.charmap();
    let mut seen_gids = HashSet::new();
    let mut glyphs: Vec<DrawnGlyph> = Vec::new();

    for ch in UPPER_PROBES.iter().chain(LOWER_PROBES.iter()) {
        if let Some(gid) = charmap.map(*ch) {
            if seen_gids.insert(gid) {
                if let Some(g) = draw_probe_glyph(&font, gid, Some(*ch), scale) {
                    glyphs.push(g);
                }
            }
        }
    }

    if glyphs.len() < 2 {
        let mut all_mappings: Vec<(u32, GlyphId)> = charmap.mappings().collect();
        all_mappings.sort_by_key(|(cp, _)| *cp);
        for (cp, gid) in all_mappings {
            if !seen_gids.insert(gid) {
                continue;
            }
            let ch = char::from_u32(cp);
            if let Some(g) = draw_probe_glyph(&font, gid, ch, scale) {
                glyphs.push(g);
                if glyphs.len() >= 8 {
                    break;
                }
            }
        }
    }

    if glyphs.is_empty() {
        return Err("font has no drawable glyph outlines for DNA analysis".to_string());
    }

    // Collect scanline thickness samples, aspect ratios, segment statistics, etc.
    let mut h_runs_all: Vec<f64> = Vec::new();
    let mut v_runs_all: Vec<f64> = Vec::new();
    let mut fill_ratios: Vec<f64> = Vec::new();
    let mut aspects: Vec<f64> = Vec::new();
    let mut uc_heights: Vec<f64> = Vec::new();
    let mut lc_heights: Vec<f64> = Vec::new();
    let mut total_lines = 0usize;
    let mut total_curves = 0usize;
    let mut total_short = 0usize;
    let mut circularities: Vec<f64> = Vec::new();
    let mut slant_angles: Vec<f64> = Vec::new();
    let mut serif_scores: Vec<f64> = Vec::new();
    let mut open_scores: Vec<f64> = Vec::new();

    for g in &glyphs {
        let w = (g.xmax - g.xmin).max(1.0);
        let h = (g.ymax - g.ymin).max(1.0);
        let bbox_area = w * h;
        fill_ratios.push((g.ink_area / bbox_area).clamp(0.0, 1.0));

        if g.is_upper || glyphs.len() <= 3 {
            aspects.push(w / h);
            uc_heights.push(h);
        }
        if g.is_lower {
            lc_heights.push(h);
        }

        total_lines += g.line_segs;
        total_curves += g.curve_segs;
        total_short += g.short_segs;

        if g.ch
            .is_some_and(|c| matches!(c, 'O' | 'o' | 'C' | 'c' | 'e'))
            && g.perimeter > 1.0
        {
            let circ = (4.0 * std::f64::consts::PI * g.ink_area) / (g.perimeter * g.perimeter);
            circularities.push(circ.clamp(0.0, 1.0));
        }

        // Horizontal scanlines across 5 vertical bands -> vertical stem widths
        for frac in [0.20, 0.35, 0.50, 0.65, 0.80] {
            let y = g.ymin + h * frac;
            for (x0, x1) in horizontal_scan_runs(&g.subpaths, y) {
                h_runs_all.push(x1 - x0);
            }
        }
        // Vertical scanlines across 5 horizontal bands -> horizontal bar thicknesses
        for frac in [0.25, 0.40, 0.50, 0.60, 0.75] {
            let x = g.xmin + w * frac;
            for (y0, y1) in vertical_scan_runs(&g.subpaths, x) {
                v_runs_all.push(y1 - y0);
            }
        }

        // Geometric slant via top vs bottom scanline X-midpoint
        if g.ch.is_none_or(|c| SLANT_PROBES.contains(&c)) {
            let top_runs = horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.82);
            let bot_runs = horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.18);
            if !top_runs.is_empty() && !bot_runs.is_empty() {
                let top_cx: f64 = top_runs.iter().map(|(a, b)| 0.5 * (a + b)).sum::<f64>()
                    / top_runs.len() as f64;
                let bot_cx: f64 = bot_runs.iter().map(|(a, b)| 0.5 * (a + b)).sum::<f64>()
                    / bot_runs.len() as f64;
                let dy = h * 0.64;
                if dy > 10.0 {
                    let deg = ((top_cx - bot_cx) / dy).atan().to_degrees().abs();
                    slant_angles.push(deg);
                }
            }
        }

        // Serifness: flare at cap/baseline vs mid-stem on I/H/E/T/L
        if g.ch.is_some_and(|c| SERIF_PROBES.contains(&c)) {
            let span_of = |runs: &[(f64, f64)]| -> Option<f64> {
                let min_x = runs.first()?.0;
                let max_x = runs.last()?.1;
                Some((max_x - min_x).max(0.0))
            };
            let bot = span_of(&horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.04));
            let mid = span_of(&horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.50));
            let top = span_of(&horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.96));
            if let (Some(b), Some(m), Some(t)) = (bot, mid, top) {
                if g.ch == Some('I') && m > 1.0 {
                    let flare = ((b.max(t) - m) / m).clamp(0.0, 1.5) / 1.5;
                    serif_scores.push(flare);
                } else if g.ch == Some('H') && m > 1.0 {
                    // On 'H' mid has crossbar, check 0.35 vs 0.04
                    if let Some(stem_w) =
                        span_of(&horizontal_scan_runs(&g.subpaths, g.ymin + h * 0.32))
                    {
                        if stem_w > 1.0 {
                            let flare = ((b.max(t) - stem_w) / stem_w).clamp(0.0, 0.5) / 0.5;
                            serif_scores.push(flare);
                        }
                    }
                }
            }
        }

        // Openness: counter/aperture spaciousness on open probes
        if g.ch.is_some_and(|c| OPEN_PROBES.contains(&c)) {
            let void_ratio = (1.0 - (g.ink_area / bbox_area)).clamp(0.0, 1.0);
            open_scores.push(void_ratio);
        }
    }

    h_runs_all.sort_by(|a, b| a.total_cmp(b));
    v_runs_all.sort_by(|a, b| a.total_cmp(b));

    let os2 = font.os2().ok();
    let post = font.post().ok();

    // 1. Weight
    let stem_median = quantile(&h_runs_all, 0.35).unwrap_or(80.0);
    let geom_stem_weight = ((stem_median - 20.0) / 160.0).clamp(0.0, 1.0);
    let mean_fill = fill_ratios.iter().sum::<f64>() / fill_ratios.len().max(1) as f64;
    let fill_weight = ((mean_fill - 0.15) / 0.55).clamp(0.0, 1.0);
    let os2_weight = os2
        .as_ref()
        .map(|o| ((o.us_weight_class() as f64 - 100.0) / 800.0).clamp(0.0, 1.0))
        .unwrap_or(0.375);
    let weight = 0.50 * geom_stem_weight + 0.30 * fill_weight + 0.20 * os2_weight;

    // 2. Width
    let mean_aspect = if aspects.is_empty() {
        0.75
    } else {
        aspects.iter().sum::<f64>() / aspects.len() as f64
    };
    let geom_width = ((mean_aspect - 0.35) / 0.85).clamp(0.0, 1.0);
    let os2_width = os2
        .as_ref()
        .map(|o| ((o.us_width_class().clamp(1, 9) as f64 - 1.0) / 8.0).clamp(0.0, 1.0))
        .unwrap_or(0.5);
    let width = 0.70 * geom_width + 0.30 * os2_width;

    // 3. Contrast
    let mut combined_runs = h_runs_all.clone();
    combined_runs.extend_from_slice(&v_runs_all);
    combined_runs.sort_by(|a, b| a.total_cmp(b));
    let thin_stroke = quantile(&combined_runs, 0.20).unwrap_or(60.0);
    let thick_stroke = quantile(&combined_runs, 0.75).unwrap_or(80.0);
    let contrast = if thick_stroke > 1.0 {
        ((thick_stroke - thin_stroke) / thick_stroke).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // 4. X-Height
    let x_height = if !uc_heights.is_empty() && !lc_heights.is_empty() {
        let uc = uc_heights.iter().sum::<f64>() / uc_heights.len() as f64;
        let lc = lc_heights.iter().sum::<f64>() / lc_heights.len() as f64;
        if uc > 1.0 {
            (lc / uc).clamp(0.0, 1.0)
        } else {
            0.72
        }
    } else if let Some(ref o) = os2 {
        match (o.sx_height(), o.s_cap_height()) {
            (Some(xh), Some(ch)) if ch > 0 => (xh as f64 / ch as f64).clamp(0.0, 1.0),
            _ => 0.72,
        }
    } else {
        0.72
    };

    // 5. Slant
    let post_slant = post
        .as_ref()
        .map(|p| ((p.italic_angle().to_f32() as f64).abs() / 20.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let geom_slant = if slant_angles.is_empty() {
        0.0
    } else {
        let mean_deg = slant_angles.iter().sum::<f64>() / slant_angles.len() as f64;
        ((mean_deg - 1.5) / 18.0).clamp(0.0, 1.0)
    };
    let slant = post_slant.max(geom_slant);

    // 6. Roundness
    let total_segs = (total_lines + total_curves).max(1) as f64;
    let curve_ratio = total_curves as f64 / total_segs;
    let mean_circ = if circularities.is_empty() {
        curve_ratio * 0.65
    } else {
        circularities.iter().sum::<f64>() / circularities.len() as f64
    };
    let roundness = (0.65 * curve_ratio + 0.35 * mean_circ).clamp(0.0, 1.0);

    // 7. Roughness
    let segs_per_glyph = total_segs / glyphs.len().max(1) as f64;
    let density_roughness = ((segs_per_glyph - 22.0) / 90.0).clamp(0.0, 1.0);
    let short_ratio = (total_short as f64 / total_segs).clamp(0.0, 1.0);
    let roughness = (0.65 * density_roughness + 0.35 * short_ratio).clamp(0.0, 1.0);

    // 8. Serifness
    let geom_serif = if serif_scores.is_empty() {
        0.2
    } else {
        serif_scores.iter().sum::<f64>() / serif_scores.len() as f64
    };
    let panose_serif = os2.as_ref().and_then(|o| {
        let p = o.panose_10();
        if p.len() >= 2 && p[0] == 2 && p[1] != 0 {
            match p[1] {
                2..=10 => Some(0.90),
                11..=13 => Some(0.08),
                _ => None,
            }
        } else {
            None
        }
    });
    let serifness = match panose_serif {
        Some(ps) => (0.55 * geom_serif + 0.45 * ps).clamp(0.0, 1.0),
        None => geom_serif.clamp(0.0, 1.0),
    };

    // 9. Monospace
    let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut mono_advs = Vec::new();
    for ch in MONO_PROBES {
        if let Some(gid) = charmap.map(ch) {
            if let Some(adv) = glyph_metrics.advance_width(gid) {
                if adv > 0.0 {
                    mono_advs.push(adv as f64);
                }
            }
        }
    }
    let geom_mono = if mono_advs.len() >= 3 {
        let min_a = mono_advs.iter().copied().fold(f64::INFINITY, f64::min);
        let max_a = mono_advs.iter().copied().fold(0.0_f64, f64::max);
        if max_a > 1.0 {
            let ratio = (min_a / max_a).clamp(0.0, 1.0);
            if ratio > 0.96 {
                1.0
            } else {
                (ratio * ratio * 0.5).clamp(0.0, 1.0)
            }
        } else {
            0.0
        }
    } else {
        0.0
    };
    let post_fixed = post.as_ref().is_some_and(|p| p.is_fixed_pitch() != 0);
    let panose_mono = os2
        .as_ref()
        .is_some_and(|o| o.panose_10().get(3).copied() == Some(9));
    let monospace = if post_fixed || panose_mono {
        geom_mono.max(0.92)
    } else {
        geom_mono
    };

    // 10. Openness
    let openness = if open_scores.is_empty() {
        (1.0 - mean_fill).clamp(0.0, 1.0)
    } else {
        (open_scores.iter().sum::<f64>() / open_scores.len() as f64).clamp(0.0, 1.0)
    };

    Ok(TypographicDna {
        weight: clamp01(weight),
        width: clamp01(width),
        contrast: clamp01(contrast),
        x_height: clamp01(x_height),
        slant: clamp01(slant),
        roundness: clamp01(roundness),
        roughness: clamp01(roughness),
        serifness: clamp01(serifness),
        monospace: clamp01(monospace),
        openness: clamp01(openness),
    })
}

const AXIS_COUNT: usize = 10;

struct VibeQueryProfile {
    targets: [Option<f32>; AXIS_COUNT],
    weights: [f32; AXIS_COUNT],
    labels: [Option<&'static str>; AXIS_COUNT],
    tokens: Vec<String>,
}

fn axis_value(dna: &TypographicDna, idx: usize) -> f32 {
    match idx {
        0 => dna.weight,
        1 => dna.width,
        2 => dna.contrast,
        3 => dna.x_height,
        4 => dna.slant,
        5 => dna.roundness,
        6 => dna.roughness,
        7 => dna.serifness,
        8 => dna.monospace,
        9 => dna.openness,
        _ => 0.5,
    }
}

fn build_vibe_query_profile(query: &str) -> VibeQueryProfile {
    let tokens: Vec<String> = query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let mut target_sums = [0.0_f32; AXIS_COUNT];
    let mut weight_sums = [0.0_f32; AXIS_COUNT];
    let mut labels: [Option<&'static str>; AXIS_COUNT] = [None; AXIS_COUNT];

    let mut push_axis = |idx: usize, val: f32, w: f32, label: &'static str| {
        target_sums[idx] += val * w;
        weight_sums[idx] += w;
        if labels[idx].is_none() {
            labels[idx] = Some(label);
        }
    };

    for tok in &tokens {
        match tok.as_str() {
            "bold" | "heavy" | "chunky" | "black" | "thick" | "fat" | "ultra" | "extra" => {
                push_axis(0, 0.90, 2.0, "heavy weight");
            }
            "light" | "thin" | "hairline" | "delicate" | "fine" | "airy" => {
                push_axis(0, 0.15, 2.0, "light weight");
            }
            "medium" | "regular" | "book" => {
                push_axis(0, 0.45, 1.0, "balanced weight");
            }
            "condensed" | "narrow" | "tall" | "compressed" | "slim" | "tight" => {
                push_axis(1, 0.20, 1.6, "condensed width");
            }
            "wide" | "extended" | "expanded" | "broad" => {
                push_axis(1, 0.90, 1.6, "wide proportions");
            }
            "serif" | "editorial" | "classic" | "academic" | "traditional" | "literary"
            | "roman" | "didone" => {
                push_axis(7, 0.85, 1.6, "serif terminals");
                push_axis(2, 0.70, 1.2, "high contrast");
            }
            "sans" | "grotesque" | "grotesk" | "clean" | "modern" | "geometric" | "minimal"
            | "swiss" | "neo" => {
                push_axis(7, 0.10, 1.5, "sans geometry");
                push_axis(6, 0.10, 1.0, "clean contours");
            }
            "mono" | "monospace" | "monospaced" | "code" | "coding" | "terminal" | "typewriter"
            | "fixed" | "console" => {
                push_axis(8, 0.95, 2.0, "monospace metrics");
            }
            "italic" | "oblique" | "slanted" | "script" | "cursive" | "handwritten"
            | "calligraphic" => {
                push_axis(4, 0.80, 1.8, "slanted posture");
            }
            "distressed" | "rough" | "grunge" | "horror" | "woodcut" | "eroded" | "weathered"
            | "textured" | "vintage" => {
                push_axis(6, 0.85, 1.8, "rough texture");
            }
            "round" | "rounded" | "friendly" | "soft" | "bubble" | "playful" | "warm" => {
                push_axis(5, 0.90, 1.6, "rounded curves");
                push_axis(6, 0.15, 0.8, "smooth contours");
            }
            "stencil" | "industrial" | "brutalist" | "sharp" | "angular" | "boxy" | "square"
            | "tech" | "cyber" | "scifi" => {
                push_axis(5, 0.20, 1.3, "angular geometry");
                push_axis(2, 0.30, 0.9, "low contrast");
                push_axis(0, 0.75, 0.8, "sturdy weight");
            }
            "open" | "legible" | "readable" | "humanist" => {
                push_axis(9, 0.80, 1.2, "open counters");
                push_axis(3, 0.75, 1.0, "tall x-height");
            }
            "display" | "poster" | "headline" | "titling" | "impactful" => {
                push_axis(0, 0.78, 1.0, "display presence");
                push_axis(2, 0.60, 0.6, "expressive contrast");
            }
            _ => {}
        }
    }

    let mut targets = [None; AXIS_COUNT];
    for i in 0..AXIS_COUNT {
        if weight_sums[i] > 0.0 {
            targets[i] = Some((target_sums[i] / weight_sums[i]).clamp(0.0, 1.0));
        }
    }

    VibeQueryProfile {
        targets,
        weights: weight_sums,
        labels,
        tokens,
    }
}

/// Ranks candidate font files `(family, path, category_hint)` against a natural-language
/// typographic vibe query using geometric `TypographicDna` similarity plus name/category hints.
pub fn search_fonts_by_vibe(
    query: &str,
    candidates: &[(String, String, String)],
    top_k: usize,
) -> Vec<VibeMatchHit> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let profile = build_vibe_query_profile(query);
    let total_axis_weight: f32 = profile.weights.iter().sum();

    let mut hits = Vec::with_capacity(candidates.len());

    for (family, path, category_hint) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(dna) = extract_dna_from_bytes(&bytes) else {
            continue;
        };

        let mut matched_traits: BTreeSet<String> = BTreeSet::new();

        let dna_score = if total_axis_weight > 0.0 {
            let mut weighted_sim = 0.0_f32;
            for i in 0..AXIS_COUNT {
                if let Some(target) = profile.targets[i] {
                    let w = profile.weights[i];
                    let val = axis_value(&dna, i);
                    let sim = (1.0 - (val - target).abs()).clamp(0.0, 1.0);
                    weighted_sim += w * sim;
                    if sim >= 0.50 {
                        if let Some(lbl) = profile.labels[i] {
                            matched_traits.insert(format!("{lbl} ({val:.2})"));
                        }
                    }
                }
            }
            (weighted_sim / total_axis_weight).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let fam_lower = family.to_ascii_lowercase();
        let cat_lower = category_hint.to_ascii_lowercase();
        let mut kw_bonus = 0.0_f32;
        for tok in &profile.tokens {
            if tok.len() >= 2 {
                if fam_lower.contains(tok) {
                    kw_bonus += 10.0;
                    matched_traits.insert(format!("family:{tok}"));
                }
                if cat_lower.contains(tok) {
                    kw_bonus += 6.0;
                    matched_traits.insert(format!("category:{tok}"));
                }
            }
        }

        let score = (dna_score * 88.0 + kw_bonus.min(12.0)).clamp(0.0, 100.0);

        hits.push(VibeMatchHit {
            family: family.clone(),
            path: path.clone(),
            score,
            matched_traits: matched_traits.into_iter().collect(),
            dna,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.family.cmp(&b.family))
            .then_with(|| a.path.cmp(&b.path))
    });
    hits.truncate(top_k.max(1));
    hits
}
