use std::collections::BTreeMap;

use crate::model::{
    compute_bbox, Anchor, Contour, FontMetrics, FoundryProject, GlyphOutline, ModularForgeRequest,
};
use crate::pathops::{apply_corners, normalize_winding, offset_outline, remove_overlap};

const DEFAULT_MATRIX_COLS: f64 = 7.0;

fn skew_anchor(cx: f64, cy: f64, dx: f64, dy: f64, slant_factor: f64) -> Anchor {
    let y = (cy + dy).round();
    let x = (cx + y * slant_factor).round() + dx;
    Anchor::on_curve(x, y)
}

fn make_box_cw(cx: f64, cy: f64, rx: f64, ry: f64, slant_factor: f64) -> Contour {
    Contour {
        anchors: vec![
            skew_anchor(cx, cy, -rx, -ry, slant_factor),
            skew_anchor(cx, cy, -rx, ry, slant_factor),
            skew_anchor(cx, cy, rx, ry, slant_factor),
            skew_anchor(cx, cy, rx, -ry, slant_factor),
        ],
        closed: true,
    }
}

fn make_box_ccw(cx: f64, cy: f64, rx: f64, ry: f64, slant_factor: f64) -> Contour {
    Contour {
        anchors: vec![
            skew_anchor(cx, cy, -rx, -ry, slant_factor),
            skew_anchor(cx, cy, rx, -ry, slant_factor),
            skew_anchor(cx, cy, rx, ry, slant_factor),
            skew_anchor(cx, cy, -rx, ry, slant_factor),
        ],
        closed: true,
    }
}

struct ModuleStyle<'a> {
    w: f64,
    h: f64,
    shape: &'a str,
    corner_pct: f64,
    slant_factor: f64,
    bezier_round: bool,
}

fn build_module_contours(cx: f64, cy: f64, style: &ModuleStyle<'_>) -> Vec<Contour> {
    let hw = (style.w / 2.0).round().max(8.0);
    let hh = (style.h / 2.0).round().max(8.0);
    let slant_factor = style.slant_factor;

    match style.shape {
        "diamond" => vec![Contour {
            anchors: vec![
                skew_anchor(cx, cy, 0.0, hh, slant_factor),
                skew_anchor(cx, cy, hw, 0.0, slant_factor),
                skew_anchor(cx, cy, 0.0, -hh, slant_factor),
                skew_anchor(cx, cy, -hw, 0.0, slant_factor),
            ],
            closed: true,
        }],
        "slit" => {
            let sh = (hh * 0.68).round().max(6.0);
            vec![make_box_cw(cx, cy, hw, sh, slant_factor)]
        }
        "hollow" => {
            let irx = (hw * 0.42).round().max(4.0);
            let iry = (hh * 0.42).round().max(4.0);
            vec![
                make_box_cw(cx, cy, hw, hh, slant_factor),
                make_box_ccw(cx, cy, irx, iry, slant_factor),
            ]
        }
        "rounded" if style.bezier_round => vec![make_box_cw(cx, cy, hw, hh, slant_factor)],
        "chamfer" | "rounded" => {
            let cut_frac = if style.shape == "rounded" {
                (style.corner_pct / 100.0).max(0.28)
            } else {
                (style.corner_pct / 100.0).max(0.12)
            };
            let c = (hw.min(hh) * cut_frac.min(0.48)).round().max(4.0);
            vec![Contour {
                anchors: vec![
                    skew_anchor(cx, cy, -hw + c, -hh, slant_factor),
                    skew_anchor(cx, cy, -hw, -hh + c, slant_factor),
                    skew_anchor(cx, cy, -hw, hh - c, slant_factor),
                    skew_anchor(cx, cy, -hw + c, hh, slant_factor),
                    skew_anchor(cx, cy, hw - c, hh, slant_factor),
                    skew_anchor(cx, cy, hw, hh - c, slant_factor),
                    skew_anchor(cx, cy, hw, -hh + c, slant_factor),
                    skew_anchor(cx, cy, hw - c, -hh, slant_factor),
                ],
                closed: true,
            }]
        }
        _ => vec![make_box_cw(cx, cy, hw, hh, slant_factor)],
    }
}

fn clean_name(s: &str, fallback: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(max_len).collect()
    }
}

/// Converts a `ModularForgeRequest` into a `FoundryProject`, matching Forge Studio's
/// 7x9 matrix proportions and applying overlap removal, true Bézier live corners,
/// and optional optical offset.
pub fn modular_to_project(req: &ModularForgeRequest) -> FoundryProject {
    let family_name = clean_name(&req.family_name, "Cosmic Forge", 48);
    let style_name = clean_name(&req.style_name, "Regular", 24);

    let cols = req.cols.max(1);
    let rows = req.rows.max(1);
    let cell_size = (76.0 * (DEFAULT_MATRIX_COLS / cols as f64))
        .round()
        .max(18.0);
    let origin_x = 48.0;
    let top_y = 740.0;
    let mod_w = (cell_size * (req.weight / 100.0)).round().max(10.0);
    let mod_h = (mod_w * (req.contrast / 100.0)).round().max(10.0);
    let hw = (mod_w / 2.0).round().max(8.0);
    let hh = (mod_h / 2.0).round().max(8.0);
    let slant_factor = req.slant.to_radians().tan();
    let side_pad = req.tracking.round().max(20.0);
    let space_adv = (cell_size * (cols as f64 * 0.54) + side_pad).round() as i32;

    let use_live_corners = req.smooth_corners || (req.shape == "rounded" && req.corner > 0.0);
    let corner_radius = if use_live_corners {
        let pct = if req.corner > 0.0 { req.corner } else { 24.0 };
        let frac = if req.shape == "rounded" {
            (pct / 100.0).clamp(0.28, 0.48)
        } else {
            (pct / 100.0).clamp(0.08, 0.48)
        };
        (hw.min(hh) * frac).max(3.0)
    } else {
        0.0
    };

    let mut glyphs = BTreeMap::new();
    glyphs.insert(
        " ".to_string(),
        GlyphOutline {
            codepoint: 0x20,
            name: "space".to_string(),
            advance_width: space_adv,
            lsb: 0,
            contours: Vec::new(),
        },
    );

    let mut max_y = 800_i16;
    let mut min_y = -200_i16;

    for (ch_str, grid) in &req.glyphs {
        let Some(ch) = ch_str.chars().next() else {
            continue;
        };
        let code = ch as u32;
        if code <= 0x20 || grid.len() != rows {
            continue;
        }

        let mut min_col = cols;
        let mut max_col = 0_usize;
        let mut has_cells = false;
        let mut contours = Vec::new();

        for (r, row_cells) in grid.iter().enumerate() {
            for (c, &cell) in row_cells.iter().take(cols).enumerate() {
                if cell == 0 {
                    continue;
                }
                has_cells = true;
                if c < min_col {
                    min_col = c;
                }
                if c > max_col {
                    max_col = c;
                }
                let cx = origin_x + (c as f64) * cell_size + cell_size * 0.5;
                let cy = top_y - (r as f64) * cell_size - cell_size * 0.5;
                let cell_contours = build_module_contours(
                    cx,
                    cy,
                    &ModuleStyle {
                        w: mod_w,
                        h: mod_h,
                        shape: &req.shape,
                        corner_pct: req.corner,
                        slant_factor,
                        bezier_round: use_live_corners,
                    },
                );
                contours.extend(cell_contours);
            }
        }

        if req.remove_overlap && !contours.is_empty() {
            contours = remove_overlap(&contours);
        }
        if use_live_corners && corner_radius > 0.0 && !contours.is_empty() {
            contours = apply_corners(&contours, corner_radius, false);
        }
        if req.optical_offset.abs() > 0.5 && !contours.is_empty() {
            contours = offset_outline(
                &contours,
                req.optical_offset,
                req.smooth_corners || req.shape == "rounded",
            );
        }
        if !contours.is_empty() {
            contours = normalize_winding(&contours, true);
        }

        let bbox = compute_bbox(&contours);
        let has_points = contours.iter().any(|c| !c.anchors.is_empty());
        let lsb = if has_points {
            bbox[0].round() as i32
        } else {
            0
        };
        let active_cols = if has_cells && max_col >= min_col {
            max_col - min_col + 1
        } else {
            3
        };
        let base_adv = if has_points {
            (bbox[2] + side_pad).max(origin_x + (active_cols as f64) * cell_size + side_pad)
        } else {
            space_adv as f64
        };
        let extra_w = req.glyph_widths.get(ch_str).copied().unwrap_or(0);
        let advance_width = (base_adv.round() as i32 + extra_w).max(180);

        if has_points {
            max_y = max_y.max((bbox[3].ceil() as i16).saturating_add(40));
            min_y = min_y.min((bbox[1].floor() as i16).saturating_sub(40));
        }

        let name = if ch.is_ascii_alphanumeric() {
            ch.to_string()
        } else {
            format!("uni{code:04X}")
        };

        glyphs.insert(
            ch_str.clone(),
            GlyphOutline {
                codepoint: code,
                name,
                advance_width,
                lsb,
                contours,
            },
        );
    }

    FoundryProject {
        version: 1,
        family_name,
        style_name,
        designer: "Cosmic Font Foundry".to_string(),
        license: "MIT".to_string(),
        metrics: FontMetrics {
            upm: 1000,
            ascender: max_y,
            descender: min_y,
            x_height: 520,
            cap_height: 740,
            line_gap: 0,
        },
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    }
}
