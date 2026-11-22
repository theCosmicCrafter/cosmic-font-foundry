use std::collections::HashSet;

use kurbo::{flatten, PathEl, Point};
use serde::{Deserialize, Serialize};
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::read::tables::glyf::Glyph as ReadGlyph;
use write_fonts::read::{FontRef as ReadFontRef, TableProvider};
use write_fonts::tables::fvar::{AxisInstanceArrays, Fvar, InstanceRecord, VariationAxisRecord};
use write_fonts::tables::glyf::SimpleGlyph;
use write_fonts::tables::gvar::{GlyphDelta, GlyphDeltas, GlyphVariations, Gvar, Tent};
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::types::{F2Dot14, Fixed, GlyphId, NameId, Tag};
use write_fonts::FontBuilder;

use crate::compiler::compile_ttf;
use crate::model::{Contour, FoundryProject, GlyphOutline};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VariableMaster {
    pub name: String,
    pub wght: f32,
    pub project: FoundryProject,
}

struct PolyContour {
    pts: Vec<Point>,
    cum_lens: Vec<f64>,
    total_len: f64,
    signed_area: f64,
    centroid: Point,
}

fn contour_to_polyline(contour: &Contour) -> Option<PolyContour> {
    if contour.anchors.len() < 2 {
        return None;
    }
    let bp = contour.to_bezpath();
    let mut pts: Vec<Point> = Vec::new();
    flatten(bp.elements().iter().copied(), 0.5, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => {
            if pts.last().is_none_or(|prev| (*prev - p).hypot() > 1e-4) {
                pts.push(p);
            }
        }
        PathEl::ClosePath => {}
        PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => {}
    });
    if pts.len() >= 2 {
        if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
            if (first - last).hypot() <= 1e-4 {
                pts.pop();
            }
        }
    }
    if pts.len() < 2 {
        return None;
    }
    Some(build_poly_contour(pts))
}

fn build_poly_contour(pts: Vec<Point>) -> PolyContour {
    let n = pts.len();
    let mut cum_lens = Vec::with_capacity(n + 1);
    cum_lens.push(0.0);
    let mut total_len = 0.0;
    let mut area2 = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;

    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        total_len += (b - a).hypot();
        cum_lens.push(total_len);
        area2 += a.x * b.y - b.x * a.y;
        sx += a.x;
        sy += a.y;
    }

    let centroid = Point::new(sx / n as f64, sy / n as f64);
    PolyContour {
        pts,
        cum_lens,
        total_len,
        signed_area: 0.5 * area2,
        centroid,
    }
}

fn reverse_poly_contour(poly: &PolyContour) -> PolyContour {
    let mut pts = poly.pts.clone();
    pts.reverse();
    build_poly_contour(pts)
}

fn eval_poly_at_arclen(poly: &PolyContour, s: f64) -> Point {
    if poly.pts.is_empty() {
        return Point::ORIGIN;
    }
    if poly.total_len <= 1e-9 || poly.pts.len() == 1 {
        return poly.pts[0];
    }
    let target = s.rem_euclid(poly.total_len);
    let n = poly.pts.len();
    for i in 0..n {
        let s0 = poly.cum_lens[i];
        let s1 = poly.cum_lens[i + 1];
        if target <= s1 + 1e-9 || i + 1 == n {
            let seg_len = s1 - s0;
            let t = if seg_len > 1e-9 {
                ((target - s0) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return poly.pts[i].lerp(poly.pts[(i + 1) % n], t);
        }
    }
    poly.pts[0]
}

fn closest_arclen_to_point(poly: &PolyContour, target: Point) -> f64 {
    if poly.pts.is_empty() {
        return 0.0;
    }
    let n = poly.pts.len();
    let mut best_dist2 = f64::INFINITY;
    let mut best_s = 0.0;
    for i in 0..n {
        let a = poly.pts[i];
        let b = poly.pts[(i + 1) % n];
        let ab = b - a;
        let ab2 = ab.hypot2();
        let t = if ab2 > 1e-12 {
            ((target - a).dot(ab) / ab2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let proj = a.lerp(b, t);
        let d2 = (proj - target).hypot2();
        if d2 < best_dist2 {
            best_dist2 = d2;
            let s0 = poly.cum_lens[i];
            let s1 = poly.cum_lens[i + 1];
            best_s = s0 + t * (s1 - s0);
        }
    }
    best_s
}

fn base_contour_fractions(pts: &[(i16, i16)]) -> (Vec<f64>, f64, Point) {
    let n = pts.len();
    if n == 0 {
        return (Vec::new(), 0.0, Point::ORIGIN);
    }
    let mut cum = Vec::with_capacity(n);
    let mut total = 0.0;
    let mut area2 = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;

    for i in 0..n {
        cum.push(total);
        let a = Point::new(pts[i].0 as f64, pts[i].1 as f64);
        let b = Point::new(pts[(i + 1) % n].0 as f64, pts[(i + 1) % n].1 as f64);
        total += (b - a).hypot();
        area2 += a.x * b.y - b.x * a.y;
        sx += a.x;
        sy += a.y;
    }
    let fractions = if total > 1e-9 {
        cum.into_iter().map(|d| d / total).collect()
    } else {
        vec![0.0; n]
    };
    (
        fractions,
        0.5 * area2,
        Point::new(sx / n as f64, sy / n as f64),
    )
}

fn find_master_glyph<'a>(
    master: &'a FoundryProject,
    key: &str,
    base_glyph: &GlyphOutline,
) -> Option<&'a GlyphOutline> {
    if let Some(g) = master.glyphs.get(key) {
        return Some(g);
    }
    if base_glyph.codepoint > 0 {
        if let Some((_, g)) = master
            .glyphs
            .iter()
            .find(|(_, g)| g.codepoint == base_glyph.codepoint)
        {
            return Some(g);
        }
    }
    if !base_glyph.name.is_empty() {
        if let Some((_, g)) = master
            .glyphs
            .iter()
            .find(|(_, g)| g.name == base_glyph.name)
        {
            return Some(g);
        }
    }
    None
}

fn compute_master_deltas_for_glyph(
    base_contours: &[Vec<(i16, i16)>],
    base_glyph: &GlyphOutline,
    base_project: &FoundryProject,
    master_glyph: &GlyphOutline,
    master_project: &FoundryProject,
) -> Option<Vec<GlyphDelta>> {
    let master_polys: Vec<PolyContour> = master_glyph
        .contours
        .iter()
        .filter_map(contour_to_polyline)
        .collect();
    if master_polys.is_empty() {
        return None;
    }

    let mut used = vec![false; master_polys.len()];
    let mut deltas = Vec::new();

    for base_pts in base_contours {
        if base_pts.is_empty() {
            continue;
        }
        let (fractions, base_area, base_centroid) = base_contour_fractions(base_pts);

        let mut best_idx = 0usize;
        let mut best_score = f64::INFINITY;
        for (idx, mp) in master_polys.iter().enumerate() {
            let used_penalty = if used[idx] { 1.0e6 } else { 0.0 };
            let sign_penalty = if (base_area < 0.0) == (mp.signed_area < 0.0) {
                0.0
            } else {
                1000.0
            };
            let dist = (mp.centroid - base_centroid).hypot();
            let score = used_penalty + sign_penalty + dist;
            if score < best_score {
                best_score = score;
                best_idx = idx;
            }
        }
        used[best_idx] = true;

        let chosen = &master_polys[best_idx];
        let aligned_poly = if (base_area < 0.0 && chosen.signed_area > 0.0)
            || (base_area > 0.0 && chosen.signed_area < 0.0)
        {
            reverse_poly_contour(chosen)
        } else {
            PolyContour {
                pts: chosen.pts.clone(),
                cum_lens: chosen.cum_lens.clone(),
                total_len: chosen.total_len,
                signed_area: chosen.signed_area,
                centroid: chosen.centroid,
            }
        };

        let start_pt = Point::new(base_pts[0].0 as f64, base_pts[0].1 as f64);
        let s0 = closest_arclen_to_point(&aligned_poly, start_pt);

        for (k, &frac) in fractions.iter().enumerate() {
            let sample = eval_poly_at_arclen(&aligned_poly, s0 + frac * aligned_poly.total_len);
            let dx = (sample.x.round() as i32 - base_pts[k].0 as i32)
                .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let dy = (sample.y.round() as i32 - base_pts[k].1 as i32)
                .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            deltas.push(GlyphDelta::required(dx, dy));
        }
    }

    let adv_delta = (master_glyph.advance_width - base_glyph.advance_width)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    let asc_delta = (master_project.metrics.ascender as i32 - base_project.metrics.ascender as i32)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    let desc_delta = (master_project.metrics.descender as i32
        - base_project.metrics.descender as i32)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16;

    // 4 TrueType phantom points: left-side origin, advance width, top origin, bottom advance.
    deltas.push(GlyphDelta::required(0, 0));
    deltas.push(GlyphDelta::required(adv_delta, 0));
    deltas.push(GlyphDelta::required(0, asc_delta));
    deltas.push(GlyphDelta::required(0, desc_delta));

    Some(deltas)
}

fn normalized_wght_coord(wght: f32, min_wght: f32, default_wght: f32, max_wght: f32) -> f32 {
    if (wght - default_wght).abs() < 1e-3 {
        0.0
    } else if wght < default_wght {
        let denom = default_wght - min_wght;
        if denom.abs() < 1e-6 {
            0.0
        } else {
            ((wght - default_wght) / denom).clamp(-1.0, 0.0)
        }
    } else {
        let denom = max_wght - default_wght;
        if denom.abs() < 1e-6 {
            0.0
        } else {
            ((wght - default_wght) / denom).clamp(0.0, 1.0)
        }
    }
}

/// Compiles a variable TrueType font (`.ttf`) with a `wght` axis (`fvar` + `gvar`)
/// from a base `FoundryProject` and a slice of `VariableMaster`s.
pub fn compile_variable_ttf(
    base: &FoundryProject,
    masters: &[VariableMaster],
) -> Result<Vec<u8>, String> {
    if masters.is_empty() {
        return Err("at least one variable master is required".to_string());
    }
    if masters.iter().any(|m| !m.wght.is_finite()) {
        return Err("variable master wght values must be finite".to_string());
    }

    let mut min_wght = masters.iter().map(|m| m.wght).fold(f32::INFINITY, f32::min);
    let mut max_wght = masters
        .iter()
        .map(|m| m.wght)
        .fold(f32::NEG_INFINITY, f32::max);
    if masters.len() == 1 {
        min_wght = min_wght.min(400.0);
        max_wght = max_wght.max(400.0);
    }
    if (max_wght - min_wght).abs() < 1e-3 {
        return Err("variable masters must span a non-zero weight range".to_string());
    }
    let default_wght = 400.0_f32.clamp(min_wght, max_wght);

    let base_compiled = compile_ttf(base, false)?;
    let base_font = ReadFontRef::new(&base_compiled.ttf_bytes)
        .map_err(|e| format!("failed to parse compiled base font: {e}"))?;

    let glyf = base_font
        .glyf()
        .map_err(|e| format!("missing glyf table in base font: {e}"))?;
    let loca = base_font
        .loca(None)
        .map_err(|e| format!("missing loca table in base font: {e}"))?;
    let maxp = base_font
        .maxp()
        .map_err(|e| format!("missing maxp table in base font: {e}"))?;
    let mut name_table: Name = base_font
        .name()
        .map_err(|e| format!("missing name table in base font: {e}"))?
        .to_owned_table();

    let num_glyphs = maxp.num_glyphs() as usize;

    // Match the exact glyph ordering in compiler::compile_ttf (GID 0 = .notdef, GID 1 = space).
    let mut sorted_glyphs: Vec<(&String, &GlyphOutline)> = base
        .glyphs
        .iter()
        .filter(|(_, g)| g.codepoint != 0x20 && g.name != ".notdef")
        .collect();
    sorted_glyphs.sort_by(|(ka, a), (kb, b)| {
        a.codepoint
            .cmp(&b.codepoint)
            .then(a.name.cmp(&b.name))
            .then(ka.cmp(kb))
    });

    let mut glyph_variations = Vec::with_capacity(num_glyphs);
    for gid in 0..num_glyphs {
        let gid_typed = GlyphId::new(gid as u32);
        if gid < 2 {
            glyph_variations.push(GlyphVariations::new(gid_typed, Vec::new()));
            continue;
        }
        let Some((glyph_key, base_glyph)) = sorted_glyphs.get(gid - 2).copied() else {
            glyph_variations.push(GlyphVariations::new(gid_typed, Vec::new()));
            continue;
        };

        let raw_glyf = loca
            .get_glyf(gid_typed, &glyf)
            .map_err(|e| format!("failed to read glyf entry for gid {gid}: {e}"))?;
        let Some(ReadGlyph::Simple(sg_ref)) = raw_glyf else {
            glyph_variations.push(GlyphVariations::new(gid_typed, Vec::new()));
            continue;
        };

        let base_sg: SimpleGlyph = sg_ref.to_owned_table();
        let base_contours: Vec<Vec<(i16, i16)>> = base_sg
            .contours
            .iter()
            .map(|c| c.iter().map(|pt| (pt.x, pt.y)).collect())
            .collect();
        let total_pts: usize = base_contours.iter().map(Vec::len).sum();
        if total_pts == 0 {
            glyph_variations.push(GlyphVariations::new(gid_typed, Vec::new()));
            continue;
        }

        let mut per_master_deltas = Vec::new();
        let mut seen_peaks: HashSet<i16> = HashSet::new();

        for master in masters {
            let norm = normalized_wght_coord(master.wght, min_wght, default_wght, max_wght);
            let peak = F2Dot14::from_f32(norm);
            if peak == F2Dot14::ZERO || !seen_peaks.insert(peak.to_bits()) {
                continue;
            }
            let Some(master_glyph) = find_master_glyph(&master.project, glyph_key, base_glyph)
            else {
                continue;
            };
            if let Some(deltas) = compute_master_deltas_for_glyph(
                &base_contours,
                base_glyph,
                base,
                master_glyph,
                &master.project,
            ) {
                if deltas.len() == total_pts + 4 {
                    per_master_deltas.push(GlyphDeltas::new(vec![Tent::new(peak, None)], deltas));
                }
            }
        }

        glyph_variations.push(GlyphVariations::new(gid_typed, per_master_deltas));
    }

    let gvar = Gvar::new(glyph_variations, 1)
        .map_err(|e| format!("failed to construct gvar table: {e:?}"))?;

    // Assign NameIds starting at 256 (avoiding collisions with existing name records).
    let max_existing = name_table
        .name_record
        .iter()
        .map(|r| r.name_id.to_u16())
        .max()
        .unwrap_or(255);
    let mut next_name_id = max_existing.saturating_add(1).max(256);

    let axis_name_id = NameId::new(next_name_id);
    next_name_id = next_name_id.saturating_add(1);
    name_table.name_record.push(NameRecord::new(
        3,
        1,
        0x0409,
        axis_name_id,
        "Weight".to_string().into(),
    ));

    let mut instances = Vec::with_capacity(masters.len());
    for master in masters {
        let inst_name_id = NameId::new(next_name_id);
        next_name_id = next_name_id.saturating_add(1);
        let label = if master.name.trim().is_empty() {
            format!("Weight {}", master.wght.round() as i32)
        } else {
            master.name.trim().to_string()
        };
        name_table
            .name_record
            .push(NameRecord::new(3, 1, 0x0409, inst_name_id, label.into()));
        instances.push(InstanceRecord {
            subfamily_name_id: inst_name_id,
            flags: 0,
            coordinates: vec![Fixed::from_f64(master.wght as f64)],
            post_script_name_id: None,
        });
    }

    name_table
        .name_record
        .sort_by_key(|r| (r.platform_id, r.encoding_id, r.language_id, r.name_id));

    let axis = VariationAxisRecord::new(
        Tag::new(b"wght"),
        Fixed::from_f64(min_wght as f64),
        Fixed::from_f64(default_wght as f64),
        Fixed::from_f64(max_wght as f64),
        0,
        axis_name_id,
    );
    let fvar = Fvar::new(AxisInstanceArrays::new(vec![axis], instances));

    let mut builder = FontBuilder::new();
    builder
        .add_table(&name_table)
        .map_err(|e| format!("failed to add name table: {e}"))?;
    builder
        .add_table(&fvar)
        .map_err(|e| format!("failed to add fvar table: {e}"))?;
    builder
        .add_table(&gvar)
        .map_err(|e| format!("failed to add gvar table: {e}"))?;
    builder.copy_missing_tables(base_font);

    Ok(builder.build())
}
