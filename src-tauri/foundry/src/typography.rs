use std::collections::{BTreeMap, HashMap, HashSet};

use kurbo::{flatten, PathEl, Point};

use crate::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphAnchor, GlyphOutline,
    KerningPair,
};
use crate::pathops::{normalize_winding, remove_overlap};

const BANDS: usize = 24;
const EPS_LEN_SQ: f64 = 1e-12;

/// Critical Latin kerning pairs evaluated by optical envelope separation.
const CRITICAL_KERN_PAIRS: &[(char, char)] = &[
    ('A', 'V'),
    ('A', 'W'),
    ('A', 'T'),
    ('A', 'Y'),
    ('A', 'C'),
    ('A', 'G'),
    ('A', 'O'),
    ('A', 'Q'),
    ('A', 'v'),
    ('A', 'w'),
    ('A', 'y'),
    ('A', 'u'),
    ('F', 'A'),
    ('F', '.'),
    ('F', ','),
    ('F', 'a'),
    ('F', 'e'),
    ('F', 'o'),
    ('K', 'o'),
    ('K', 'y'),
    ('K', 'u'),
    ('K', 'e'),
    ('L', 'T'),
    ('L', 'V'),
    ('L', 'W'),
    ('L', 'Y'),
    ('L', 'y'),
    ('O', 'V'),
    ('O', 'W'),
    ('O', 'X'),
    ('O', 'Y'),
    ('P', 'A'),
    ('P', '.'),
    ('P', ','),
    ('T', 'A'),
    ('T', 'O'),
    ('T', 'a'),
    ('T', 'e'),
    ('T', 'i'),
    ('T', 'o'),
    ('T', 'r'),
    ('T', 's'),
    ('T', 'u'),
    ('T', 'w'),
    ('T', 'y'),
    ('T', '.'),
    ('T', ','),
    ('V', 'A'),
    ('V', 'a'),
    ('V', 'e'),
    ('V', 'i'),
    ('V', 'o'),
    ('V', 'u'),
    ('V', '.'),
    ('V', ','),
    ('W', 'A'),
    ('W', 'a'),
    ('W', 'e'),
    ('W', 'i'),
    ('W', 'o'),
    ('W', '.'),
    ('W', ','),
    ('Y', 'A'),
    ('Y', 'a'),
    ('Y', 'e'),
    ('Y', 'o'),
    ('Y', 'u'),
    ('Y', '.'),
    ('Y', ','),
    ('f', '"'),
    ('f', '\''),
    ('f', ')'),
    ('r', '.'),
    ('r', ','),
    ('r', '-'),
    ('v', '.'),
    ('v', ','),
    ('w', '.'),
    ('w', ','),
    ('y', '.'),
    ('y', ','),
];

#[derive(Clone, Copy, Debug)]
enum DiacriticKind {
    Acute,
    Grave,
    Circumflex,
    Dieresis,
    Tilde,
    Ring,
    Cedilla,
}

impl DiacriticKind {
    fn is_bottom(self) -> bool {
        matches!(self, Self::Cedilla)
    }

    fn candidate_codepoints(self) -> &'static [u32] {
        match self {
            Self::Acute => &[0x0301, 0x00B4],
            Self::Grave => &[0x0300, 0x0060],
            Self::Circumflex => &[0x0302, 0x02C6, 0x005E],
            Self::Dieresis => &[0x0308, 0x00A8],
            Self::Tilde => &[0x0303, 0x02DC, 0x007E],
            Self::Ring => &[0x030A, 0x02DA, 0x00B0],
            Self::Cedilla => &[0x0327, 0x00B8],
        }
    }
}

const LATIN_COMPOSITES: &[(char, char, DiacriticKind)] = &[
    ('Á', 'A', DiacriticKind::Acute),
    ('À', 'A', DiacriticKind::Grave),
    ('Â', 'A', DiacriticKind::Circumflex),
    ('Ä', 'A', DiacriticKind::Dieresis),
    ('Ã', 'A', DiacriticKind::Tilde),
    ('Å', 'A', DiacriticKind::Ring),
    ('É', 'E', DiacriticKind::Acute),
    ('È', 'E', DiacriticKind::Grave),
    ('Ê', 'E', DiacriticKind::Circumflex),
    ('Ë', 'E', DiacriticKind::Dieresis),
    ('Í', 'I', DiacriticKind::Acute),
    ('Ì', 'I', DiacriticKind::Grave),
    ('Î', 'I', DiacriticKind::Circumflex),
    ('Ï', 'I', DiacriticKind::Dieresis),
    ('Ñ', 'N', DiacriticKind::Tilde),
    ('Ó', 'O', DiacriticKind::Acute),
    ('Ò', 'O', DiacriticKind::Grave),
    ('Ô', 'O', DiacriticKind::Circumflex),
    ('Ö', 'O', DiacriticKind::Dieresis),
    ('Õ', 'O', DiacriticKind::Tilde),
    ('Ú', 'U', DiacriticKind::Acute),
    ('Ù', 'U', DiacriticKind::Grave),
    ('Û', 'U', DiacriticKind::Circumflex),
    ('Ü', 'U', DiacriticKind::Dieresis),
    ('Ý', 'Y', DiacriticKind::Acute),
    ('á', 'a', DiacriticKind::Acute),
    ('à', 'a', DiacriticKind::Grave),
    ('â', 'a', DiacriticKind::Circumflex),
    ('ä', 'a', DiacriticKind::Dieresis),
    ('ã', 'a', DiacriticKind::Tilde),
    ('å', 'a', DiacriticKind::Ring),
    ('é', 'e', DiacriticKind::Acute),
    ('è', 'e', DiacriticKind::Grave),
    ('ê', 'e', DiacriticKind::Circumflex),
    ('ë', 'e', DiacriticKind::Dieresis),
    ('í', 'i', DiacriticKind::Acute),
    ('ì', 'i', DiacriticKind::Grave),
    ('î', 'i', DiacriticKind::Circumflex),
    ('ï', 'i', DiacriticKind::Dieresis),
    ('ñ', 'n', DiacriticKind::Tilde),
    ('ó', 'o', DiacriticKind::Acute),
    ('ò', 'o', DiacriticKind::Grave),
    ('ô', 'o', DiacriticKind::Circumflex),
    ('ö', 'o', DiacriticKind::Dieresis),
    ('õ', 'o', DiacriticKind::Tilde),
    ('ú', 'u', DiacriticKind::Acute),
    ('ù', 'u', DiacriticKind::Grave),
    ('û', 'u', DiacriticKind::Circumflex),
    ('ü', 'u', DiacriticKind::Dieresis),
    ('ý', 'y', DiacriticKind::Acute),
    ('ÿ', 'y', DiacriticKind::Dieresis),
    ('Ç', 'C', DiacriticKind::Cedilla),
    ('ç', 'c', DiacriticKind::Cedilla),
];

fn has_drawable_contours(glyph: &GlyphOutline) -> bool {
    glyph.contours.iter().any(|c| c.anchors.len() >= 2)
}

fn flatten_contour_edges(contours: &[Contour]) -> Vec<(Point, Point)> {
    let mut edges = Vec::new();
    for contour in contours {
        if contour.anchors.len() < 2 {
            continue;
        }
        let bp = contour.to_bezpath();
        let mut subpath_start: Option<Point> = None;
        let mut cur: Option<Point> = None;
        flatten(bp.elements().iter().copied(), 0.5, |el| match el {
            PathEl::MoveTo(p) => {
                subpath_start = Some(p);
                cur = Some(p);
            }
            PathEl::LineTo(p) => {
                if let Some(c) = cur {
                    if (p - c).hypot2() > EPS_LEN_SQ {
                        edges.push((c, p));
                    }
                }
                cur = Some(p);
            }
            PathEl::ClosePath => {
                if let (Some(c), Some(s)) = (cur, subpath_start) {
                    if (s - c).hypot2() > EPS_LEN_SQ {
                        edges.push((c, s));
                    }
                    cur = Some(s);
                }
            }
            PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => {}
        });
    }
    edges
}

fn scanline_extents_at_y(edges: &[(Point, Point)], y: f64) -> Option<(f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut hit = false;

    for &(p0, p1) in edges {
        // Half-open vertical crossing rule prevents double-counting shared vertices.
        let crosses = (p0.y <= y && p1.y > y) || (p1.y <= y && p0.y > y);
        if !crosses {
            continue;
        }
        let dy = p1.y - p0.y;
        if dy.abs() <= 1e-12 {
            continue;
        }
        let t = (y - p0.y) / dy;
        let x = p0.x + t * (p1.x - p0.x);
        if x.is_finite() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            hit = true;
        }
    }

    if hit {
        Some((min_x, max_x))
    } else {
        None
    }
}

fn vertical_band_bounds(metrics: &FontMetrics) -> (f64, f64) {
    let y_min = metrics.descender as f64;
    let y_max = metrics.ascender as f64;
    if y_max - y_min >= 10.0 {
        (y_min, y_max)
    } else {
        (-200.0, 800.0)
    }
}

fn sample_band_profile(contours: &[Contour], metrics: &FontMetrics) -> [Option<(f64, f64)>; BANDS] {
    let mut profile = [None; BANDS];
    let edges = flatten_contour_edges(contours);
    if edges.is_empty() {
        return profile;
    }

    let (y_min, y_max) = vertical_band_bounds(metrics);
    let band_h = (y_max - y_min) / BANDS as f64;

    for (idx, slot) in profile.iter_mut().enumerate() {
        let y0 = y_min + idx as f64 * band_h;
        let y_center = y0 + 0.5 * band_h;
        if let Some(ext) = scanline_extents_at_y(&edges, y_center) {
            *slot = Some(ext);
        } else {
            // Sub-sample inside the band so thin horizontal bars are not skipped.
            let mut combined: Option<(f64, f64)> = None;
            for frac in [0.25, 0.75] {
                if let Some((lx, rx)) = scanline_extents_at_y(&edges, y0 + frac * band_h) {
                    combined = Some(match combined {
                        Some((al, ar)) => (al.min(lx), ar.max(rx)),
                        None => (lx, rx),
                    });
                }
            }
            *slot = combined;
        }
    }

    profile
}

fn band_weight(y: f64, y_min: f64, y_max: f64, metrics: &FontMetrics) -> f64 {
    let x_h = (metrics.x_height as f64).max(1.0).min(y_max);
    if (0.0..=x_h).contains(&y) {
        1.0
    } else if y > x_h {
        let span = (y_max - x_h).max(1.0);
        let t = ((y - x_h) / span).clamp(0.0, 1.0);
        1.0 - 0.8 * t
    } else {
        let span = (0.0 - y_min).max(1.0);
        let t = ((0.0 - y) / span).clamp(0.0, 1.0);
        1.0 - 0.8 * t
    }
}

fn compute_optical_edges(contours: &[Contour], metrics: &FontMetrics) -> (f64, f64) {
    let bbox = compute_bbox(contours);
    let profile = sample_band_profile(contours, metrics);
    let (y_min, y_max) = vertical_band_bounds(metrics);
    let band_h = (y_max - y_min) / BANDS as f64;

    let mut w_sum = 0.0;
    let mut left_sum = 0.0;
    let mut right_sum = 0.0;

    for (idx, ext) in profile.iter().enumerate() {
        let Some((lx, rx)) = *ext else {
            continue;
        };
        let y_c = y_min + (idx as f64 + 0.5) * band_h;
        let w = band_weight(y_c, y_min, y_max, metrics);
        w_sum += w;
        left_sum += lx * w;
        right_sum += rx * w;
    }

    if w_sum > 1e-6 {
        (left_sum / w_sum, right_sum / w_sum)
    } else {
        (bbox[0], bbox[2])
    }
}

fn is_narrow_or_curved_punctuation(codepoint: u32) -> bool {
    matches!(
        char::from_u32(codepoint),
        Some(
            '.' | ','
                | ':'
                | ';'
                | '\''
                | '"'
                | '`'
                | '!'
                | '?'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '-'
                | '–'
                | '—'
                | '·'
                | '•'
        )
    )
}

fn shift_contours_x(contours: &mut [Contour], dx: f64) {
    if dx.abs() <= 1e-9 {
        return;
    }
    for contour in contours {
        for anchor in &mut contour.anchors {
            anchor.x += dx;
        }
    }
}

fn shift_glyph_anchors_x(
    anchors: &mut BTreeMap<String, Vec<GlyphAnchor>>,
    glyph_key: &str,
    glyph_name: &str,
    dx: f64,
) {
    if dx.abs() <= 1e-9 {
        return;
    }
    if let Some(list) = anchors.get_mut(glyph_key) {
        for a in list {
            a.x += dx;
        }
    } else if glyph_name != glyph_key {
        if let Some(list) = anchors.get_mut(glyph_name) {
            for a in list {
                a.x += dx;
            }
        }
    }
}

/// Automatically balances optical sidebearings across all non-empty glyphs using
/// a 24-band weighted scanline ink profile, then reapplies linked metric rules.
pub fn auto_space_project(project: &mut FoundryProject, target_sidebearing: f64) -> usize {
    let metrics = project.metrics.clone();
    let mut spaced = 0_usize;

    for (key, glyph) in &mut project.glyphs {
        if glyph.codepoint == 0x20 || !has_drawable_contours(glyph) {
            continue;
        }

        let effective_sb = if is_narrow_or_curved_punctuation(glyph.codepoint) {
            target_sidebearing * 0.65
        } else {
            target_sidebearing
        };

        let (opt_left, opt_right) = compute_optical_edges(&glyph.contours, &metrics);
        let dx = effective_sb - opt_left;
        shift_contours_x(&mut glyph.contours, dx);
        shift_glyph_anchors_x(&mut project.anchors, key, &glyph.name, dx);

        let shifted_opt_right = opt_right + dx;
        glyph.advance_width = (shifted_opt_right + effective_sb).round().max(160.0) as i32;
        let bbox = compute_bbox(&glyph.contours);
        glyph.lsb = bbox[0].round() as i32;
        spaced += 1;
    }

    apply_linked_metrics(project);
    spaced
}

/// Computes optical kerning pairs across critical Latin glyph combinations using
/// 24-band horizontal separation, updates `project.kerning`, and returns emitted pairs.
pub fn auto_kern_project(
    project: &mut FoundryProject,
    target_gap: f64,
    min_abs_kern: i16,
) -> Vec<KerningPair> {
    let mut by_cp: HashMap<u32, &GlyphOutline> = HashMap::new();
    for glyph in project.glyphs.values() {
        if glyph.codepoint > 0 && has_drawable_contours(glyph) {
            by_cp.entry(glyph.codepoint).or_insert(glyph);
        }
    }

    let mut profiles: HashMap<u32, [Option<(f64, f64)>; BANDS]> = HashMap::new();
    let mut evaluated_pairs: HashSet<(u32, u32)> = HashSet::new();
    let mut emitted: Vec<KerningPair> = Vec::new();
    let threshold = min_abs_kern.max(0) as f64;

    for &(left_ch, right_ch) in CRITICAL_KERN_PAIRS {
        let l_cp = left_ch as u32;
        let r_cp = right_ch as u32;
        if !evaluated_pairs.insert((l_cp, r_cp)) {
            continue;
        }
        let (Some(&left), Some(&right)) = (by_cp.get(&l_cp), by_cp.get(&r_cp)) else {
            continue;
        };

        let left_prof = *profiles
            .entry(l_cp)
            .or_insert_with(|| sample_band_profile(&left.contours, &project.metrics));
        let right_prof = *profiles
            .entry(r_cp)
            .or_insert_with(|| sample_band_profile(&right.contours, &project.metrics));

        let mut min_sep: Option<f64> = None;
        for i in 0..BANDS {
            if let (Some((_, r_ink)), Some((l_ink, _))) = (left_prof[i], right_prof[i]) {
                let sep = (left.advance_width as f64 - r_ink) + l_ink;
                min_sep = Some(match min_sep {
                    Some(prev) => prev.min(sep),
                    None => sep,
                });
            }
        }

        let Some(sep) = min_sep else {
            continue;
        };
        if sep > target_gap + threshold || sep < target_gap - threshold {
            let value = (target_gap - sep).round().clamp(-180.0, 120.0) as i16;
            if value != 0 && value.abs() >= min_abs_kern {
                emitted.push(KerningPair {
                    left: l_cp,
                    right: r_cp,
                    value,
                });
            }
        }
    }

    project
        .kerning
        .retain(|pair| !evaluated_pairs.contains(&(pair.left, pair.right)));
    project.kerning.extend(emitted.iter().cloned());
    project.kerning.sort_by_key(|p| (p.left, p.right));

    emitted
}

fn poly_contour(points: &[(f64, f64)]) -> Contour {
    Contour {
        anchors: points
            .iter()
            .map(|&(x, y)| Anchor::on_curve(x, y))
            .collect(),
        closed: true,
    }
}

fn circle_contour(cx: f64, cy: f64, r: f64, ccw: bool) -> Contour {
    let steps = 12_usize;
    let mut anchors = Vec::with_capacity(steps);
    for i in 0..steps {
        let angle = if ccw {
            2.0 * std::f64::consts::PI * (i as f64) / (steps as f64)
        } else {
            -2.0 * std::f64::consts::PI * (i as f64) / (steps as f64)
        };
        anchors.push(Anchor::on_curve(cx + r * angle.cos(), cy + r * angle.sin()));
    }
    Contour {
        anchors,
        closed: true,
    }
}

fn synthesize_diacritic(kind: DiacriticKind, metrics: &FontMetrics) -> Vec<Contour> {
    let u = (metrics.upm as f64 / 1000.0).max(0.2);
    let raw = match kind {
        DiacriticKind::Acute => vec![poly_contour(&[
            (-40.0 * u, 0.0),
            (-10.0 * u, 135.0 * u),
            (65.0 * u, 135.0 * u),
            (5.0 * u, 0.0),
        ])],
        DiacriticKind::Grave => vec![poly_contour(&[
            (-5.0 * u, 0.0),
            (-65.0 * u, 135.0 * u),
            (10.0 * u, 135.0 * u),
            (40.0 * u, 0.0),
        ])],
        DiacriticKind::Circumflex => vec![poly_contour(&[
            (-105.0 * u, 0.0),
            (-20.0 * u, 130.0 * u),
            (20.0 * u, 130.0 * u),
            (105.0 * u, 0.0),
            (55.0 * u, 0.0),
            (0.0, 80.0 * u),
            (-55.0 * u, 0.0),
        ])],
        DiacriticKind::Dieresis => {
            let r = 34.0 * u;
            let offset = 62.0 * u;
            vec![
                circle_contour(-offset, r, r, false),
                circle_contour(offset, r, r, false),
            ]
        }
        DiacriticKind::Tilde => vec![poly_contour(&[
            (-115.0 * u, 10.0 * u),
            (-75.0 * u, 85.0 * u),
            (-20.0 * u, 85.0 * u),
            (35.0 * u, 38.0 * u),
            (72.0 * u, 38.0 * u),
            (100.0 * u, 85.0 * u),
            (125.0 * u, 60.0 * u),
            (85.0 * u, -10.0 * u),
            (25.0 * u, -10.0 * u),
            (-30.0 * u, 38.0 * u),
            (-65.0 * u, 38.0 * u),
            (-92.0 * u, -10.0 * u),
        ])],
        DiacriticKind::Ring => {
            let r_out = 62.0 * u;
            let r_in = 30.0 * u;
            vec![
                circle_contour(0.0, r_out, r_out, false),
                circle_contour(0.0, r_out, r_in, true),
            ]
        }
        DiacriticKind::Cedilla => vec![poly_contour(&[
            (-16.0 * u, 0.0),
            (20.0 * u, 0.0),
            (20.0 * u, -42.0 * u),
            (52.0 * u, -42.0 * u),
            (52.0 * u, -138.0 * u),
            (-45.0 * u, -138.0 * u),
            (-45.0 * u, -98.0 * u),
            (14.0 * u, -98.0 * u),
            (14.0 * u, -72.0 * u),
            (-16.0 * u, -72.0 * u),
        ])],
    };
    normalize_winding(&raw, true)
}

fn find_diacritic_contours(project: &FoundryProject, kind: DiacriticKind) -> Vec<Contour> {
    for &cp in kind.candidate_codepoints() {
        if let Some(glyph) = project
            .glyphs
            .values()
            .find(|g| g.codepoint == cp && has_drawable_contours(g))
        {
            return glyph.contours.clone();
        }
    }
    synthesize_diacritic(kind, &project.metrics)
}

fn find_base_glyph(project: &FoundryProject, base_char: char) -> Option<(String, GlyphOutline)> {
    // Prefer dotless 'ı' (U+0131) when composing accented 'i'.
    if base_char == 'i' {
        if let Some((k, g)) = project
            .glyphs
            .iter()
            .find(|(_, g)| g.codepoint == 0x0131 && has_drawable_contours(g))
        {
            return Some((k.clone(), g.clone()));
        }
    }

    let cp = base_char as u32;
    if let Some((k, g)) = project
        .glyphs
        .iter()
        .find(|(_, g)| g.codepoint == cp && has_drawable_contours(g))
    {
        return Some((k.clone(), g.clone()));
    }

    let key = base_char.to_string();
    project
        .glyphs
        .get(&key)
        .filter(|g| has_drawable_contours(g))
        .map(|g| (key, g.clone()))
}

fn strip_i_dot_if_present(contours: &[Contour], x_height: f64) -> Vec<Contour> {
    if contours.len() < 2 {
        return contours.to_vec();
    }
    let dot_idx = contours.iter().enumerate().find_map(|(idx, c)| {
        let bb = compute_bbox(std::slice::from_ref(c));
        let h = bb[3] - bb[1];
        if bb[1] > x_height * 0.9 && h < x_height * 0.45 {
            Some(idx)
        } else {
            None
        }
    });
    match dot_idx {
        Some(drop_idx) => contours
            .iter()
            .enumerate()
            .filter_map(|(i, c)| if i == drop_idx { None } else { Some(c.clone()) })
            .collect(),
        None => contours.to_vec(),
    }
}

fn find_glyph_anchor<'a>(
    anchors: &'a BTreeMap<String, Vec<GlyphAnchor>>,
    base_key: &str,
    base_name: &str,
    base_char: char,
    anchor_name: &str,
) -> Option<&'a GlyphAnchor> {
    let char_key = base_char.to_string();
    for candidate_key in [base_key, base_name, char_key.as_str()] {
        if let Some(list) = anchors.get(candidate_key) {
            if let Some(found) = list.iter().find(|a| a.name == anchor_name) {
                return Some(found);
            }
        }
    }
    None
}

fn translate_contours(contours: &mut [Contour], dx: f64, dy: f64) {
    for c in contours {
        for a in &mut c.anchors {
            a.x += dx;
            a.y += dy;
        }
    }
}

/// Generates accented Latin-Extended composite glyphs (`ÁÀÂÄÃÅÉÈÊËÍÌÎÏÑÓÒÔÖÕÚÙÛÜÝáàâäãåéèêëíìîïñóòôöõúùûüýÿÇç`)
/// from available base glyphs, fusing base and diacritic contours with `remove_overlap`.
pub fn generate_latin_composites(project: &mut FoundryProject) -> usize {
    let mut generated = 0_usize;
    let accent_gap = (project.metrics.upm as f64 * 0.055).max(30.0);

    for &(target_ch, base_ch, kind) in LATIN_COMPOSITES {
        let Some((base_key, base_glyph)) = find_base_glyph(project, base_ch) else {
            continue;
        };

        let base_contours = if base_ch == 'i' && base_glyph.codepoint == 'i' as u32 {
            strip_i_dot_if_present(&base_glyph.contours, project.metrics.x_height as f64)
        } else {
            base_glyph.contours.clone()
        };
        let base_bbox = compute_bbox(&base_contours);
        let base_cx = (base_bbox[0] + base_bbox[2]) * 0.5;

        let mut accent_contours = find_diacritic_contours(project, kind);
        if accent_contours.is_empty() {
            continue;
        }
        let accent_bbox = compute_bbox(&accent_contours);
        let accent_cx = (accent_bbox[0] + accent_bbox[2]) * 0.5;

        let (anchor_x, anchor_y) = if kind.is_bottom() {
            if let Some(a) = find_glyph_anchor(
                &project.anchors,
                &base_key,
                &base_glyph.name,
                base_ch,
                "bottom",
            ) {
                (a.x, a.y)
            } else {
                (base_cx, base_bbox[1])
            }
        } else if let Some(a) = find_glyph_anchor(
            &project.anchors,
            &base_key,
            &base_glyph.name,
            base_ch,
            "top",
        ) {
            (a.x, a.y)
        } else {
            let top_y = if base_bbox[3] > 0.0 {
                base_bbox[3]
            } else if base_ch.is_ascii_uppercase() {
                project.metrics.cap_height as f64
            } else {
                project.metrics.x_height as f64
            };
            (base_cx, top_y + accent_gap)
        };

        let dx = anchor_x - accent_cx;
        let dy = if kind.is_bottom() {
            anchor_y - accent_bbox[3]
        } else {
            anchor_y - accent_bbox[1]
        };
        translate_contours(&mut accent_contours, dx, dy);

        let mut combined = base_contours;
        combined.extend(accent_contours);
        let fused = remove_overlap(&combined);
        let normalized = normalize_winding(&fused, true);

        let cp = target_ch as u32;
        project.glyphs.insert(
            target_ch.to_string(),
            GlyphOutline {
                codepoint: cp,
                name: format!("uni{cp:04X}"),
                advance_width: base_glyph.advance_width,
                lsb: base_glyph.lsb,
                contours: normalized,
            },
        );
        generated += 1;
    }

    generated
}

fn resolve_glyph_key(project: &FoundryProject, key_or_name: &str) -> Option<String> {
    if project.glyphs.contains_key(key_or_name) {
        return Some(key_or_name.to_string());
    }
    project
        .glyphs
        .iter()
        .find(|(_, g)| g.name == key_or_name)
        .map(|(k, _)| k.clone())
}

fn glyph_lsb_rsb(glyph: &GlyphOutline) -> (f64, f64) {
    if has_drawable_contours(glyph) {
        let bbox = compute_bbox(&glyph.contours);
        let lsb = bbox[0];
        let rsb = glyph.advance_width as f64 - bbox[2];
        (lsb, rsb)
    } else {
        let lsb = glyph.lsb as f64;
        let rsb = glyph.advance_width as f64 - lsb;
        (lsb, rsb)
    }
}

/// Applies linked sidebearing rules (`lsb_ref`, `rsb_ref`, optional `mirror`) across
/// target glyphs, returning the number of target glyphs updated.
pub fn apply_linked_metrics(project: &mut FoundryProject) -> usize {
    let rules = project.linked_metrics.clone();
    let mut updated = 0_usize;

    for rule in rules {
        let Some(target_key) = resolve_glyph_key(project, &rule.target) else {
            continue;
        };

        let lsb_source = if rule.mirror {
            rule.rsb_ref.as_deref().or(rule.lsb_ref.as_deref())
        } else {
            rule.lsb_ref.as_deref()
        };
        let rsb_source = if rule.mirror {
            rule.lsb_ref.as_deref().or(rule.rsb_ref.as_deref())
        } else {
            rule.rsb_ref.as_deref()
        };

        let desired_lsb = lsb_source
            .and_then(|k| resolve_glyph_key(project, k))
            .and_then(|k| project.glyphs.get(&k))
            .map(|g| {
                let (lsb, rsb) = glyph_lsb_rsb(g);
                if rule.mirror {
                    rsb
                } else {
                    lsb
                }
            });

        let desired_rsb = rsb_source
            .and_then(|k| resolve_glyph_key(project, k))
            .and_then(|k| project.glyphs.get(&k))
            .map(|g| {
                let (lsb, rsb) = glyph_lsb_rsb(g);
                if rule.mirror {
                    lsb
                } else {
                    rsb
                }
            });

        if desired_lsb.is_none() && desired_rsb.is_none() {
            continue;
        }

        let Some(target) = project.glyphs.get_mut(&target_key) else {
            continue;
        };
        let has_contours = has_drawable_contours(target);
        let bbox = compute_bbox(&target.contours);
        let cur_lsb = if has_contours {
            bbox[0]
        } else {
            target.lsb as f64
        };
        let cur_xmax = if has_contours { bbox[2] } else { cur_lsb };
        let cur_rsb = target.advance_width as f64 - cur_xmax;

        let dx = desired_lsb.map(|dl| dl - cur_lsb).unwrap_or(0.0);
        if dx.abs() > 1e-9 {
            shift_contours_x(&mut target.contours, dx);
            let target_name = target.name.clone();
            target.lsb = (cur_lsb + dx).round() as i32;
            let shifted_xmax = cur_xmax + dx;
            let final_rsb = desired_rsb.unwrap_or(cur_rsb);
            target.advance_width = ((shifted_xmax + final_rsb).round() as i32).max(0);
            shift_glyph_anchors_x(&mut project.anchors, &target_key, &target_name, dx);
        } else if let Some(dr) = desired_rsb {
            target.advance_width = ((cur_xmax + dr).round() as i32).max(0);
        }

        updated += 1;
    }

    updated
}
