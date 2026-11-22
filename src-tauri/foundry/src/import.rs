use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use kurbo::{Arc, BezPath, Point, SvgArc, Vec2};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use svgtypes::{PathParser, PathSegment};

use crate::model::{
    compute_bbox, contours_to_svg_d, Contour, FontMetrics, FoundryProject, GlyphInspectInfo,
    GlyphOutline,
};
use crate::pathops::normalize_winding;

struct BezPathPen {
    path: BezPath,
}

impl BezPathPen {
    fn new() -> Self {
        Self {
            path: BezPath::new(),
        }
    }
}

impl OutlinePen for BezPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path
            .quad_to((cx0 as f64, cy0 as f64), (x as f64, y as f64));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            (cx0 as f64, cy0 as f64),
            (cx1 as f64, cy1 as f64),
            (x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

fn default_glyph_name(cp: u32) -> String {
    match cp {
        0x20 => "space".to_string(),
        _ => format!("uni{cp:04X}"),
    }
}

fn localized_string(font: &FontRef<'_>, id: StringId) -> Option<String> {
    font.localized_strings(id)
        .english_or_first()
        .map(|s| s.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_font_metrics(font: &FontRef<'_>) -> FontMetrics {
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    let upm = if m.units_per_em > 0 {
        m.units_per_em
    } else {
        1000
    };
    let ascender = if m.ascent.abs() > 0.5 {
        m.ascent.round() as i16
    } else {
        (upm as f64 * 0.8) as i16
    };
    let descender = if m.descent.abs() > 0.5 {
        -(m.descent.abs().round() as i16)
    } else {
        -((upm as f64 * 0.2) as i16)
    };
    let x_height = m
        .x_height
        .map(|v| v.round() as i16)
        .unwrap_or_else(|| (upm as f64 * 0.52).round() as i16);
    let cap_height = m
        .cap_height
        .map(|v| v.round() as i16)
        .unwrap_or_else(|| (upm as f64 * 0.72).round() as i16);
    let line_gap = m.leading.round() as i16;

    FontMetrics {
        upm,
        ascender,
        descender,
        x_height,
        cap_height,
        line_gap,
    }
}

fn extract_glyph_contours(font: &FontRef<'_>, gid: GlyphId) -> Result<Vec<Contour>, String> {
    let outlines = font.outline_glyphs();
    let Some(outline) = outlines.get(gid) else {
        return Ok(Vec::new());
    };
    let mut pen = BezPathPen::new();
    let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
    outline
        .draw(settings, &mut pen)
        .map_err(|e| format!("failed to draw glyph {gid:?}: {e}"))?;
    Ok(Contour::from_bezpath(&pen.path))
}

fn check_embedding_permissions(font: &FontRef<'_>) -> Result<(), String> {
    if let Ok(os2) = font.os2() {
        let fs_type = os2.fs_type();
        // Bit 1 (0x0002) is Restricted License embedding; reject unless Preview (0x0004) or Editable (0x0008) is set.
        if (fs_type & 0x0002) != 0 && (fs_type & (0x0004 | 0x0008)) == 0 {
            return Err(
                "Font OS/2 fsType restricts modification and embedding (0x0002)".to_string(),
            );
        }
    }
    Ok(())
}

/// Imports an existing `.ttf`/`.otf` font file into a `FoundryProject`.
pub fn fork_font_file(
    path: &Path,
    new_family: Option<&str>,
    max_glyphs: usize,
) -> Result<FoundryProject, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let font = FontRef::from_index(&bytes, 0)
        .map_err(|e| format!("failed to parse font {}: {e}", path.display()))?;

    check_embedding_permissions(&font)?;

    let metrics = extract_font_metrics(&font);
    let base_family = localized_string(&font, StringId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| localized_string(&font, StringId::FAMILY_NAME))
        .unwrap_or_else(|| "Forked Font".to_string());
    let family_name = match new_family.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => name.to_string(),
        None => format!("{base_family} Remix"),
    };
    let style_name = localized_string(&font, StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| localized_string(&font, StringId::SUBFAMILY_NAME))
        .unwrap_or_else(|| "Regular".to_string());
    let designer = localized_string(&font, StringId::DESIGNER).unwrap_or_default();
    let license = localized_string(&font, StringId::LICENSE_DESCRIPTION).unwrap_or_default();

    let limit = if max_glyphs == 0 { 256 } else { max_glyphs };
    let all_mappings: Vec<(u32, GlyphId)> = font.charmap().mappings().collect();

    let mut ordered: Vec<(u32, GlyphId)> = Vec::with_capacity(limit.min(all_mappings.len()));
    let mut seen = HashSet::new();

    // Priority 1: Printable ASCII 0x20..=0x7E
    let mut ascii: Vec<(u32, GlyphId)> = all_mappings
        .iter()
        .copied()
        .filter(|(cp, _)| (0x20..=0x7E).contains(cp))
        .collect();
    ascii.sort_by_key(|(cp, _)| *cp);
    for (cp, gid) in ascii {
        if ordered.len() >= limit {
            break;
        }
        if seen.insert(cp) {
            ordered.push((cp, gid));
        }
    }

    // Priority 2: Latin-1 Supplement 0xA0..=0xFF
    if ordered.len() < limit {
        let mut latin1: Vec<(u32, GlyphId)> = all_mappings
            .iter()
            .copied()
            .filter(|(cp, _)| (0xA0..=0xFF).contains(cp) && !seen.contains(cp))
            .collect();
        latin1.sort_by_key(|(cp, _)| *cp);
        for (cp, gid) in latin1 {
            if ordered.len() >= limit {
                break;
            }
            if seen.insert(cp) {
                ordered.push((cp, gid));
            }
        }
    }

    // Priority 3: Remaining BMP glyphs
    if ordered.len() < limit {
        let mut rest: Vec<(u32, GlyphId)> = all_mappings
            .iter()
            .copied()
            .filter(|(cp, _)| *cp <= 0xFFFF && !seen.contains(cp))
            .collect();
        rest.sort_by_key(|(cp, _)| *cp);
        for (cp, gid) in rest {
            if ordered.len() >= limit {
                break;
            }
            if seen.insert(cp) {
                ordered.push((cp, gid));
            }
        }
    }

    let glyph_names = font.glyph_names();
    let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut glyphs = BTreeMap::new();

    for (cp, gid) in ordered {
        let Some(ch) = char::from_u32(cp) else {
            continue;
        };
        let key = ch.to_string();
        let name = glyph_names
            .get(gid)
            .map(|n| n.as_str().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_glyph_name(cp));

        let contours = extract_glyph_contours(&font, gid)?;
        let advance_width = glyph_metrics
            .advance_width(gid)
            .map(|w| w.round() as i32)
            .unwrap_or((metrics.upm as i32) / 2);
        let bbox = compute_bbox(&contours);
        let lsb = glyph_metrics
            .left_side_bearing(gid)
            .map(|v| v.round() as i32)
            .unwrap_or_else(|| {
                if contours.is_empty() {
                    0
                } else {
                    bbox[0].round() as i32
                }
            });

        glyphs.insert(
            key,
            GlyphOutline {
                codepoint: cp,
                name,
                advance_width,
                lsb,
                contours,
            },
        );
    }

    Ok(FoundryProject {
        version: 1,
        family_name,
        style_name,
        designer,
        license,
        metrics,
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    })
}

/// Inspects a single glyph from a font file and returns SVG preview + anchor data.
pub fn inspect_glyph_outline(path: &Path, codepoint: u32) -> Result<GlyphInspectInfo, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let font = FontRef::from_index(&bytes, 0)
        .map_err(|e| format!("failed to parse font {}: {e}", path.display()))?;

    let gid = font
        .charmap()
        .map(codepoint)
        .ok_or_else(|| format!("codepoint U+{codepoint:04X} not found in font"))?;

    let metrics = extract_font_metrics(&font);
    let glyph_names = font.glyph_names();
    let glyph_name = glyph_names
        .get(gid)
        .map(|n| n.as_str().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_glyph_name(codepoint));

    let contours = extract_glyph_contours(&font, gid)?;
    let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let advance_width = glyph_metrics
        .advance_width(gid)
        .map(|w| w.round() as i32)
        .unwrap_or((metrics.upm as i32) / 2);
    let bbox = compute_bbox(&contours);
    let lsb = glyph_metrics
        .left_side_bearing(gid)
        .map(|v| v.round() as i32)
        .unwrap_or_else(|| {
            if contours.is_empty() {
                0
            } else {
                bbox[0].round() as i32
            }
        });

    let svg_path = contours_to_svg_d(&contours, metrics.ascender as f64);
    let contour_count = contours.len();
    let point_count = contours.iter().map(|c| c.anchors.len()).sum();

    let vx0 = bbox[0].min(0.0);
    let vx1 = bbox[2]
        .max(advance_width as f64)
        .max(vx0 + metrics.upm as f64 * 0.25);
    let vy0 = 0.0_f64.min(metrics.ascender as f64 - bbox[3]);
    let vy1 =
        (metrics.ascender as f64 - metrics.descender as f64).max(metrics.ascender as f64 - bbox[1]);
    let view_box = [vx0, vy0, (vx1 - vx0).max(1.0), (vy1 - vy0).max(1.0)];

    Ok(GlyphInspectInfo {
        codepoint,
        glyph_name,
        svg_path,
        view_box,
        upm: metrics.upm,
        ascender: metrics.ascender,
        descender: metrics.descender,
        advance_width,
        lsb,
        bbox,
        contour_count,
        point_count,
        contours,
    })
}

fn extract_svg_path_attributes(svg: &str) -> Vec<String> {
    let mut results = Vec::new();
    let lower = svg.to_ascii_lowercase();
    let mut pos = 0usize;

    while let Some(rel) = lower[pos..].find("<path") {
        let tag_start = pos + rel;
        let after_tag = tag_start + 5;
        let next_ch = lower.as_bytes().get(after_tag).copied();
        if !matches!(next_ch, Some(b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>')) {
            pos = after_tag;
            continue;
        }
        let Some(end_rel) = svg[after_tag..].find('>') else {
            break;
        };
        let tag_body = &svg[after_tag..after_tag + end_rel];
        if let Some(d_val) = find_attr_value(tag_body, "d") {
            if !d_val.trim().is_empty() {
                results.push(d_val);
            }
        }
        pos = after_tag + end_rel + 1;
    }

    results
}

fn find_attr_value(tag_body: &str, attr_name: &str) -> Option<String> {
    let bytes = tag_body.as_bytes();
    let name_bytes = attr_name.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'/' {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let name_end = i;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let val = if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let v_end = i;
                if i < bytes.len() {
                    i += 1;
                }
                &tag_body[v_start..v_end]
            } else {
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                &tag_body[v_start..i]
            };

            if name_end - name_start == name_bytes.len()
                && tag_body[name_start..name_end].eq_ignore_ascii_case(attr_name)
            {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn parse_svg_d_to_bezpath(d: &str, out: &mut BezPath) -> Result<(), String> {
    let mut cur = Point::ORIGIN;
    let mut subpath_start = Point::ORIGIN;
    let mut last_cubic_ctrl: Option<Point> = None;
    let mut last_quad_ctrl: Option<Point> = None;

    for seg in PathParser::from(d) {
        let seg = seg.map_err(|e| format!("SVG path parse error: {e}"))?;
        match seg {
            PathSegment::MoveTo { abs, x, y } => {
                let pt = if abs {
                    Point::new(x, y)
                } else {
                    Point::new(cur.x + x, cur.y + y)
                };
                out.move_to(pt);
                cur = pt;
                subpath_start = pt;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            PathSegment::LineTo { abs, x, y } => {
                let pt = if abs {
                    Point::new(x, y)
                } else {
                    Point::new(cur.x + x, cur.y + y)
                };
                out.line_to(pt);
                cur = pt;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            PathSegment::HorizontalLineTo { abs, x } => {
                let nx = if abs { x } else { cur.x + x };
                let pt = Point::new(nx, cur.y);
                out.line_to(pt);
                cur = pt;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            PathSegment::VerticalLineTo { abs, y } => {
                let ny = if abs { y } else { cur.y + y };
                let pt = Point::new(cur.x, ny);
                out.line_to(pt);
                cur = pt;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            PathSegment::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, pt) = if abs {
                    (Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y))
                } else {
                    (
                        Point::new(cur.x + x1, cur.y + y1),
                        Point::new(cur.x + x2, cur.y + y2),
                        Point::new(cur.x + x, cur.y + y),
                    )
                };
                out.curve_to(c1, c2, pt);
                cur = pt;
                last_cubic_ctrl = Some(c2);
                last_quad_ctrl = None;
            }
            PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                let c1 = match last_cubic_ctrl {
                    Some(prev_c2) => Point::new(2.0 * cur.x - prev_c2.x, 2.0 * cur.y - prev_c2.y),
                    None => cur,
                };
                let (c2, pt) = if abs {
                    (Point::new(x2, y2), Point::new(x, y))
                } else {
                    (
                        Point::new(cur.x + x2, cur.y + y2),
                        Point::new(cur.x + x, cur.y + y),
                    )
                };
                out.curve_to(c1, c2, pt);
                cur = pt;
                last_cubic_ctrl = Some(c2);
                last_quad_ctrl = None;
            }
            PathSegment::Quadratic { abs, x1, y1, x, y } => {
                let (q1, pt) = if abs {
                    (Point::new(x1, y1), Point::new(x, y))
                } else {
                    (
                        Point::new(cur.x + x1, cur.y + y1),
                        Point::new(cur.x + x, cur.y + y),
                    )
                };
                out.quad_to(q1, pt);
                cur = pt;
                last_quad_ctrl = Some(q1);
                last_cubic_ctrl = None;
            }
            PathSegment::SmoothQuadratic { abs, x, y } => {
                let q1 = match last_quad_ctrl {
                    Some(prev_q1) => Point::new(2.0 * cur.x - prev_q1.x, 2.0 * cur.y - prev_q1.y),
                    None => cur,
                };
                let pt = if abs {
                    Point::new(x, y)
                } else {
                    Point::new(cur.x + x, cur.y + y)
                };
                out.quad_to(q1, pt);
                cur = pt;
                last_quad_ctrl = Some(q1);
                last_cubic_ctrl = None;
            }
            PathSegment::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let target = if abs {
                    Point::new(x, y)
                } else {
                    Point::new(cur.x + x, cur.y + y)
                };
                if (target - cur).hypot() > 1e-6 {
                    let svg_arc = SvgArc {
                        from: cur,
                        to: target,
                        radii: Vec2::new(rx.abs(), ry.abs()),
                        x_rotation: x_axis_rotation.to_radians(),
                        large_arc,
                        sweep,
                    };
                    if let Some(arc) = Arc::from_svg_arc(&svg_arc) {
                        arc.to_cubic_beziers(0.1, |p1, p2, p3| {
                            out.curve_to(p1, p2, p3);
                        });
                    } else {
                        out.line_to(target);
                    }
                }
                cur = target;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            PathSegment::ClosePath { .. } => {
                out.close_path();
                cur = subpath_start;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
        }
    }
    Ok(())
}

/// Parses raw SVG path `d` data or `<svg>...</svg>` XML into Y-up normalized `Contour`s
/// scaled to fit ~`0.72 * upm` height with baseline at `y = 0`.
pub fn import_svg_glyph(svg_or_d: &str, upm: u16) -> Result<Vec<Contour>, String> {
    let trimmed = svg_or_d.trim();
    if trimmed.is_empty() {
        return Err("empty SVG input".to_string());
    }

    let d_list = if trimmed.contains('<') {
        let extracted = extract_svg_path_attributes(trimmed);
        if extracted.is_empty() {
            return Err("no <path d=\"...\"> elements found in SVG".to_string());
        }
        extracted
    } else {
        vec![trimmed.to_string()]
    };

    let mut bp = BezPath::new();
    for d in d_list {
        parse_svg_d_to_bezpath(&d, &mut bp)?;
    }

    let mut contours = Contour::from_bezpath(&bp);
    if contours.is_empty() {
        return Err("SVG path produced no contours".to_string());
    }

    // Flip Y from SVG Y-down to font Y-up.
    for c in &mut contours {
        for a in &mut c.anchors {
            a.y = -a.y;
            a.h_in[1] = -a.h_in[1];
            a.h_out[1] = -a.h_out[1];
        }
    }

    let bbox = compute_bbox(&contours);
    let height = (bbox[3] - bbox[1]).max(1e-3);
    let target_h = 0.72 * (upm.max(100) as f64);
    let scale = target_h / height;
    let dx = -bbox[0];
    let dy = -bbox[1];

    for c in &mut contours {
        for a in &mut c.anchors {
            a.x = (a.x + dx) * scale;
            a.y = (a.y + dy) * scale;
            a.h_in[0] *= scale;
            a.h_in[1] *= scale;
            a.h_out[0] *= scale;
            a.h_out[1] *= scale;
        }
    }

    Ok(normalize_winding(&contours, true))
}
