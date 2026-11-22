use std::collections::HashMap;

use kurbo::{BezPath, Shape};
use linesweeper::{binary_op, BinaryOp, FillRule};
use serde::{Deserialize, Serialize};

use crate::model::{compute_bbox, Anchor, Contour, FoundryProject, GlyphAnchor, GlyphOutline};
use crate::pathops::{normalize_winding, offset_outline, remove_overlap};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StyleVariantKind {
    Bold,
    Light,
    Condensed,
    Extended,
    SmallCaps,
    Oblique,
    Inline,
}

impl StyleVariantKind {
    fn style_label(&self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Light => "Light",
            Self::Condensed => "Condensed",
            Self::Extended => "Extended",
            Self::SmallCaps => "Small Caps",
            Self::Oblique => "Oblique",
            Self::Inline => "Inline",
        }
    }
}

fn transform_contours_linear<F>(contours: &[Contour], f: F) -> Vec<Contour>
where
    F: Fn(f64, f64) -> (f64, f64),
{
    let (ox, oy) = f(0.0, 0.0);
    contours
        .iter()
        .map(|c| {
            let anchors = c
                .anchors
                .iter()
                .map(|a| {
                    let (nx, ny) = f(a.x, a.y);
                    let (hix, hiy) = f(a.h_in[0], a.h_in[1]);
                    let (hox, hoy) = f(a.h_out[0], a.h_out[1]);
                    Anchor {
                        x: nx,
                        y: ny,
                        h_in: [hix - ox, hiy - oy],
                        h_out: [hox - ox, hoy - oy],
                        smooth: a.smooth,
                    }
                })
                .collect();
            Contour {
                anchors,
                closed: c.closed,
            }
        })
        .collect()
}

fn shift_contours_x(contours: &mut [Contour], dx: f64) {
    if dx.abs() <= 1e-6 {
        return;
    }
    for c in contours {
        for a in &mut c.anchors {
            a.x += dx;
        }
    }
}

fn update_glyph_lsb(glyph: &mut GlyphOutline) {
    if glyph.contours.is_empty() {
        glyph.lsb = 0;
    } else {
        glyph.lsb = compute_bbox(&glyph.contours)[0].round() as i32;
    }
}

fn combine_closed_bezpath(contours: &[Contour]) -> Option<BezPath> {
    let mut bp = BezPath::new();
    let mut has_any = false;
    for c in contours {
        if c.closed && c.anchors.len() >= 2 {
            let sub = c.to_bezpath();
            if sub.area().abs() > 1e-4 {
                bp.extend(sub.elements().iter().copied());
                has_any = true;
            }
        }
    }
    has_any.then_some(bp)
}

fn reverse_contour(c: &Contour) -> Contour {
    let mut anchors: Vec<Anchor> = c
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
    if c.closed && anchors.len() > 1 {
        anchors.rotate_right(1);
    }
    Contour {
        anchors,
        closed: c.closed,
    }
}

fn generate_bold(project: &mut FoundryProject, amount: f64) {
    let delta = if amount <= 0.0 || !amount.is_finite() {
        24.0
    } else {
        amount.clamp(1.0, 200.0)
    };

    for glyph in project.glyphs.values_mut() {
        if glyph.contours.is_empty() {
            if glyph.codepoint == 0x20 || glyph.advance_width > 0 {
                glyph.advance_width =
                    (glyph.advance_width as f64 + delta * 0.5).round().max(1.0) as i32;
            }
            continue;
        }

        let orig_bb = compute_bbox(&glyph.contours);
        let expanded = offset_outline(&glyph.contours, delta, true);
        let mut norm = normalize_winding(&expanded, true);
        if norm.is_empty() {
            norm = normalize_winding(&glyph.contours, true);
        }
        let new_bb = compute_bbox(&norm);
        let target_lsb = (orig_bb[0] - delta * 0.15).max(orig_bb[0] * 0.75);
        let dx = target_lsb - new_bb[0];
        shift_contours_x(&mut norm, dx);

        let right_growth = (new_bb[2] + dx) - orig_bb[2];
        let orig_rsb = glyph.advance_width as f64 - orig_bb[2];
        let target_adv =
            ((new_bb[2] + dx) + orig_rsb).max(glyph.advance_width as f64 + right_growth.max(delta));
        glyph.contours = norm;
        glyph.advance_width = target_adv.round().max(1.0) as i32;
        update_glyph_lsb(glyph);
    }
}

fn generate_light(project: &mut FoundryProject, amount: f64) {
    let mag = if amount == 0.0 || !amount.is_finite() {
        16.0
    } else {
        amount.abs().clamp(1.0, 150.0)
    };

    for glyph in project.glyphs.values_mut() {
        if glyph.contours.is_empty() {
            if glyph.codepoint == 0x20 || glyph.advance_width > 0 {
                glyph.advance_width =
                    (glyph.advance_width as f64 - mag * 0.4).round().max(1.0) as i32;
            }
            continue;
        }

        let orig_bb = compute_bbox(&glyph.contours);
        let mut thinned = Vec::new();
        for factor in [1.0, 0.5, 0.25] {
            let candidate = offset_outline(&glyph.contours, -mag * factor, true);
            if !candidate.is_empty() {
                thinned = candidate;
                break;
            }
        }
        if thinned.is_empty() {
            let cx = 0.5 * (orig_bb[0] + orig_bb[2]);
            let cy = 0.5 * (orig_bb[1] + orig_bb[3]);
            thinned = transform_contours_linear(&glyph.contours, |x, y| {
                (cx + (x - cx) * 0.92, cy + (y - cy) * 0.96)
            });
        }

        let mut norm = normalize_winding(&thinned, true);
        let new_bb = compute_bbox(&norm);
        let target_lsb = orig_bb[0] + mag * 0.15;
        let dx = target_lsb - new_bb[0];
        shift_contours_x(&mut norm, dx);

        let shrunk_adv = (glyph.advance_width as f64 - mag * 0.7)
            .max(new_bb[2] + dx + 8.0)
            .max(1.0);
        glyph.contours = norm;
        glyph.advance_width = shrunk_adv.round() as i32;
        update_glyph_lsb(glyph);
    }
}

fn generate_horizontal_width(project: &mut FoundryProject, factor: f64) {
    for glyph in project.glyphs.values_mut() {
        if glyph.contours.is_empty() {
            if glyph.codepoint == 0x20 || glyph.advance_width > 0 {
                glyph.advance_width =
                    ((glyph.advance_width as f64) * factor).round().max(1.0) as i32;
            }
            continue;
        }

        let orig_bb = compute_bbox(&glyph.contours);
        let cx = 0.5 * (orig_bb[0] + orig_bb[2]);
        let scaled = transform_contours_linear(&glyph.contours, |x, y| (cx + (x - cx) * factor, y));

        let comp_delta = if factor < 1.0 {
            ((1.0 - factor) * 28.0).clamp(1.5, 12.0)
        } else if factor > 1.0 {
            ((factor - 1.0) * 14.0).clamp(1.0, 8.0)
        } else {
            0.0
        };

        let compensated = if comp_delta > 0.0 {
            let off = offset_outline(&scaled, comp_delta, true);
            if off.is_empty() {
                scaled
            } else {
                off
            }
        } else {
            scaled
        };

        let mut norm = normalize_winding(&compensated, true);
        let new_bb = compute_bbox(&norm);
        let target_lsb = (orig_bb[0] * factor).max(8.0);
        let dx = target_lsb - new_bb[0];
        shift_contours_x(&mut norm, dx);

        let scaled_adv = ((glyph.advance_width as f64) * factor).max(1.0);
        glyph.contours = norm;
        glyph.advance_width = scaled_adv.round() as i32;
        update_glyph_lsb(glyph);
    }

    for kp in &mut project.kerning {
        kp.value = ((kp.value as f64) * factor).round() as i16;
    }
    for anchors in project.anchors.values_mut() {
        for a in anchors {
            a.x *= factor;
        }
    }
}

fn generate_small_caps(project: &mut FoundryProject, amount: f64) {
    let sy = if amount > 0.0 && amount.is_finite() {
        amount.clamp(0.5, 0.95)
    } else if project.metrics.cap_height > 0 && project.metrics.x_height > 0 {
        (project.metrics.x_height as f64 / project.metrics.cap_height as f64).clamp(0.68, 0.78)
    } else {
        0.74
    };
    let sx = sy * 1.05;
    let embolden = (6.0 * (project.metrics.upm as f64 / 1000.0)).clamp(4.0, 8.0);

    let mut upper_by_cp: HashMap<u32, GlyphOutline> = HashMap::new();
    let mut lower_key_by_cp: HashMap<u32, String> = HashMap::new();
    for (k, g) in &project.glyphs {
        if (b'A' as u32..=b'Z' as u32).contains(&g.codepoint) {
            upper_by_cp.insert(g.codepoint, g.clone());
        } else if (b'a' as u32..=b'z' as u32).contains(&g.codepoint) {
            lower_key_by_cp.insert(g.codepoint, k.clone());
        }
    }

    let mut upper_anchors: HashMap<String, Vec<GlyphAnchor>> = HashMap::new();
    for (k, v) in &project.anchors {
        upper_anchors.insert(k.clone(), v.clone());
    }

    for lc in 'a'..='z' {
        let uc_cp = lc.to_ascii_uppercase() as u32;
        let lc_cp = lc as u32;
        let Some(upper) = upper_by_cp.get(&uc_cp) else {
            continue;
        };
        if upper.contours.is_empty() {
            continue;
        }

        let scaled = transform_contours_linear(&upper.contours, |x, y| (x * sx, y * sy));
        let emboldened = offset_outline(&scaled, embolden, true);
        let base_contours = if emboldened.is_empty() {
            scaled
        } else {
            emboldened
        };
        let mut norm = normalize_winding(&base_contours, true);
        let new_bb = compute_bbox(&norm);
        let target_lsb = (upper.lsb as f64 * sx).max(8.0);
        shift_contours_x(&mut norm, target_lsb - new_bb[0]);

        let adv = ((upper.advance_width as f64) * (sx * 1.04) + embolden * 1.5)
            .round()
            .max(1.0) as i32;
        let lsb = compute_bbox(&norm)[0].round() as i32;

        let target_key = lower_key_by_cp
            .get(&lc_cp)
            .cloned()
            .unwrap_or_else(|| lc.to_string());
        let name = project
            .glyphs
            .get(&target_key)
            .map(|g| g.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| lc.to_string());

        project.glyphs.insert(
            target_key.clone(),
            GlyphOutline {
                codepoint: lc_cp,
                name,
                advance_width: adv,
                lsb,
                contours: norm,
            },
        );

        let uc_key = lc.to_ascii_uppercase().to_string();
        if let Some(anc) = upper_anchors.get(&uc_key) {
            let scaled_anc = anc
                .iter()
                .map(|a| GlyphAnchor {
                    name: a.name.clone(),
                    x: a.x * sx,
                    y: a.y * sy,
                })
                .collect();
            project.anchors.insert(target_key, scaled_anc);
        }
    }
}

fn generate_oblique(project: &mut FoundryProject, amount: f64) {
    let angle_deg = if amount == 0.0 || !amount.is_finite() {
        10.0
    } else {
        amount
    };
    let k = angle_deg.clamp(-25.0, 25.0).to_radians().tan();

    for glyph in project.glyphs.values_mut() {
        if glyph.contours.is_empty() {
            continue;
        }
        glyph.contours = transform_contours_linear(&glyph.contours, |x, y| (x + k * y, y));
        update_glyph_lsb(glyph);
    }
    for anchors in project.anchors.values_mut() {
        for a in anchors {
            a.x += k * a.y;
        }
    }
}

fn generate_inline(project: &mut FoundryProject, amount: f64) {
    let inset = if amount <= 0.0 || !amount.is_finite() {
        18.0
    } else {
        amount.clamp(2.0, 120.0)
    };
    let outer_delta = (inset * 0.25).clamp(1.0, 20.0);

    for glyph in project.glyphs.values_mut() {
        if glyph.contours.is_empty() {
            continue;
        }
        let cleaned = remove_overlap(&glyph.contours);
        let outer = {
            let expanded = offset_outline(&cleaned, outer_delta, true);
            if expanded.is_empty() {
                cleaned.clone()
            } else {
                expanded
            }
        };

        let mut inner = Vec::new();
        for factor in [0.75, 0.5, 0.3, 0.15] {
            let eroded = offset_outline(&cleaned, -inset * factor, true);
            if !eroded.is_empty() {
                inner = eroded;
                break;
            }
        }

        if inner.is_empty() {
            glyph.contours = normalize_winding(&outer, true);
            update_glyph_lsb(glyph);
            continue;
        }

        let combined_inline = match (
            combine_closed_bezpath(&outer),
            combine_closed_bezpath(&inner),
        ) {
            (Some(outer_bp), Some(inner_bp)) => {
                match binary_op(
                    &outer_bp,
                    &inner_bp,
                    FillRule::NonZero,
                    BinaryOp::Difference,
                ) {
                    Ok(ls) => {
                        let mut out = Vec::new();
                        for c in ls.into_contours() {
                            for mut mc in Contour::from_bezpath(&c.path) {
                                mc.closed = true;
                                if mc.anchors.len() >= 2 && mc.to_bezpath().area().abs() > 1e-4 {
                                    out.push(mc);
                                }
                            }
                        }
                        out
                    }
                    Err(_) => Vec::new(),
                }
            }
            _ => Vec::new(),
        };

        let mut result = if combined_inline.is_empty() {
            let mut merged = normalize_winding(&outer, true);
            for c in normalize_winding(&inner, true) {
                merged.push(reverse_contour(&c));
            }
            normalize_winding(&merged, true)
        } else {
            normalize_winding(&combined_inline, true)
        };

        if result.is_empty() {
            result = normalize_winding(&outer, true);
        }

        glyph.contours = result;
        glyph.advance_width = (glyph.advance_width as f64 + outer_delta * 2.0)
            .round()
            .max(1.0) as i32;
        update_glyph_lsb(glyph);
    }
}

/// Generates a multi-style derivative (`Bold`, `Light`, `Condensed`, `Extended`,
/// `SmallCaps`, `Oblique`, or `Inline`) from an existing `FoundryProject`.
pub fn generate_style_variant(
    project: &FoundryProject,
    kind: StyleVariantKind,
    amount: f64,
) -> FoundryProject {
    let mut out = project.clone();
    out.style_name = kind.style_label().to_string();

    match kind {
        StyleVariantKind::Bold => generate_bold(&mut out, amount),
        StyleVariantKind::Light => generate_light(&mut out, amount),
        StyleVariantKind::Condensed => {
            let factor = if amount <= 0.0 || !amount.is_finite() {
                0.78
            } else {
                amount.clamp(0.35, 0.99)
            };
            generate_horizontal_width(&mut out, factor);
        }
        StyleVariantKind::Extended => {
            let factor = if amount <= 0.0 || !amount.is_finite() {
                1.24
            } else {
                amount.clamp(1.01, 2.5)
            };
            generate_horizontal_width(&mut out, factor);
        }
        StyleVariantKind::SmallCaps => generate_small_caps(&mut out, amount),
        StyleVariantKind::Oblique => generate_oblique(&mut out, amount),
        StyleVariantKind::Inline => generate_inline(&mut out, amount),
    }

    out
}
