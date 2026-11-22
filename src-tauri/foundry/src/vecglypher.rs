use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageFormat};
use kurbo::{flatten, Affine, Arc, BezPath, PathEl, Point, SvgArc, Vec2};
use serde::{Deserialize, Serialize};
use serde_json::json;
use svgtypes::{PathParser, PathSegment};

use crate::import::import_svg_glyph;
use crate::model::{compute_bbox, Contour, GlyphOutline};
use crate::pathops::{add_extrema, normalize_winding, remove_overlap};

pub const VECGLYPHER_SYSTEM_PROMPT: &str = "You are a specialized vector glyph designer creating SVG path elements.\n\nCRITICAL REQUIREMENTS:\n- Each glyph must be a complete, self-contained <path> element, in reading order of the given text.\n- Terminate each <path> element with a newline character\n- Output ONLY valid SVG <path> elements";

pub const VECGLYPHER_RECOMMENDED_GGUF: &str = "hf.co/wahidmounir/VecGlypher-27b-it-Q4_K_M-GGUF";

const DEFAULT_STYLE_PROMPT: &str = "clean modern sans-serif, 400 weight, normal style";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VecGlypherPromptConfig {
    pub style_prompt: String,
    pub target_text: String,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub repetition_penalty: f32,
    pub max_tokens: u32,
}

impl Default for VecGlypherPromptConfig {
    fn default() -> Self {
        Self {
            style_prompt: "humanist sans-serif, 600 weight, calm, clean, normal style".into(),
            target_text: "A".into(),
            temperature: 0.0,
            top_p: 0.8,
            top_k: 20,
            repetition_penalty: 1.05,
            max_tokens: 1024,
        }
    }
}

/// Formats the VecGlypher user prompt, falling back to a default style description when empty.
pub fn format_user_prompt(style_prompt: &str, target_text: &str) -> String {
    let trimmed = style_prompt.trim();
    let style = if trimmed.is_empty() {
        DEFAULT_STYLE_PROMPT
    } else {
        trimmed
    };
    format!("Font design requirements: {style}\nText content: {target_text}")
}

/// Builds an Ollama `/api/chat` request payload with optional base64 PNG exemplar images.
pub fn build_ollama_chat_payload(
    model: &str,
    cfg: &VecGlypherPromptConfig,
    images_b64: &[String],
) -> serde_json::Value {
    let prompt_text = format_user_prompt(&cfg.style_prompt, &cfg.target_text);
    let user_msg = if images_b64.is_empty() {
        json!({
            "role": "user",
            "content": prompt_text,
        })
    } else {
        json!({
            "role": "user",
            "content": prompt_text,
            "images": images_b64,
        })
    };

    json!({
        "model": model,
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": VECGLYPHER_SYSTEM_PROMPT,
            },
            user_msg,
        ],
        "options": {
            "temperature": cfg.temperature,
            "top_p": cfg.top_p,
            "top_k": cfg.top_k,
            "repeat_penalty": cfg.repetition_penalty,
            "num_predict": cfg.max_tokens,
        }
    })
}

/// Builds an OpenAI-compatible `/v1/chat/completions` payload for vLLM / llama.cpp.
pub fn build_openai_chat_payload(
    model: &str,
    cfg: &VecGlypherPromptConfig,
    images_b64: &[String],
) -> serde_json::Value {
    let prompt_text = format_user_prompt(&cfg.style_prompt, &cfg.target_text);
    let user_content = if images_b64.is_empty() {
        serde_json::Value::String(prompt_text)
    } else {
        let mut parts = Vec::with_capacity(images_b64.len() + 1);
        for b64 in images_b64 {
            parts.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/png;base64,{b64}")
                }
            }));
        }
        parts.push(json!({
            "type": "text",
            "text": prompt_text,
        }));
        serde_json::Value::Array(parts)
    };

    json!({
        "model": model,
        "stream": false,
        "temperature": cfg.temperature,
        "top_p": cfg.top_p,
        "max_tokens": cfg.max_tokens,
        "repetition_penalty": cfg.repetition_penalty,
        "chat_template_kwargs": {
            "enable_thinking": false,
        },
        "messages": [
            {
                "role": "system",
                "content": VECGLYPHER_SYSTEM_PROMPT,
            },
            {
                "role": "user",
                "content": user_content,
            }
        ]
    })
}

fn strip_think_and_fences(raw: &str) -> String {
    // Strip any <think>...</think> blocks (case-insensitive on ASCII tags).
    let mut no_think = String::with_capacity(raw.len());
    let mut rest = raw;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            no_think.push_str(rest);
            break;
        };
        no_think.push_str(&rest[..start]);
        let after_open = start + "<think>".len();
        if let Some(end_rel) = lower[after_open..].find("</think>") {
            rest = &rest[after_open + end_rel + "</think>".len()..];
        } else {
            // Unclosed <think> block (e.g., truncated inside thinking) — drop trailing thoughts.
            break;
        }
    }

    // Strip markdown code fence lines (```xml, ```svg, ```).
    let mut cleaned = String::with_capacity(no_think.len());
    for line in no_think.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            continue;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    cleaned
}

#[derive(Debug)]
struct ExtractedPathTag {
    d: String,
    transform: Option<String>,
}

fn find_attr_tolerant(tag_body: &str, attr_name: &str) -> Option<String> {
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

fn extract_path_tags_tolerant(text: &str) -> Vec<ExtractedPathTag> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut pos = 0usize;

    while let Some(rel) = lower[pos..].find("<path") {
        let tag_start = pos + rel;
        let after_tag = tag_start + 5;
        let next_ch = lower.as_bytes().get(after_tag).copied();
        if !matches!(next_ch, Some(b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>')) {
            pos = after_tag;
            continue;
        }

        // Stop at '>' if present, or up to the next '<' (or end of string) for truncated tags.
        let end_pos = match text[after_tag..].find('>') {
            Some(gt_rel) => after_tag + gt_rel,
            None => match text[after_tag..].find('<') {
                Some(lt_rel) => after_tag + lt_rel,
                None => text.len(),
            },
        };
        let tag_body = &text[after_tag..end_pos];
        if let Some(d) = find_attr_tolerant(tag_body, "d") {
            let trimmed_d = d.trim();
            if !trimmed_d.is_empty() {
                let transform = find_attr_tolerant(tag_body, "transform");
                out.push(ExtractedPathTag {
                    d: trimmed_d.to_string(),
                    transform,
                });
            }
        }
        pos = if end_pos < text.len() && text.as_bytes()[end_pos] == b'>' {
            end_pos + 1
        } else {
            end_pos
        };
    }
    out
}

fn parse_svg_d_strict(d: &str) -> Result<BezPath, String> {
    let mut out = BezPath::new();
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

    if out.is_empty() {
        return Err("SVG path contained no segments".to_string());
    }
    Ok(out)
}

fn previous_token_boundary(s: &str, end: usize) -> usize {
    let prefix = s[..end].trim_end_matches(|c: char| c.is_whitespace() || c == ',');
    if prefix.is_empty() {
        return 0;
    }
    // Step backward across the trailing number/command token cleanly.
    let bytes = prefix.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let ch = bytes[i - 1];
        if ch.is_ascii_whitespace() || ch == b',' {
            return i;
        }
        if ch.is_ascii_alphabetic() && ch != b'e' && ch != b'E' {
            // Stop right before a command letter, or drop the command letter itself if it's at the end.
            return if i == bytes.len() { i - 1 } else { i };
        }
        i -= 1;
    }
    0
}

fn recover_truncated_d(raw_d: &str) -> Result<BezPath, String> {
    let trimmed = raw_d.trim();
    if trimmed.is_empty() {
        return Err("empty SVG path d attribute".to_string());
    }
    if let Ok(bp) = parse_svg_d_strict(trimmed) {
        return Ok(bp);
    }

    let mut end = trimmed.len();
    while end > 0 {
        let next_end = previous_token_boundary(trimmed, end);
        let candidate = trimmed[..next_end].trim_end();
        if candidate.is_empty() {
            break;
        }
        if let Ok(bp) = parse_svg_d_strict(candidate) {
            return Ok(bp);
        }
        if next_end >= end {
            break;
        }
        end = next_end;
    }

    // Fallback: step character-by-character from the end in case tokens were packed tightly.
    for (idx, _) in trimmed.char_indices().rev() {
        let candidate = trimmed[..idx].trim_end();
        if candidate.is_empty() {
            break;
        }
        if let Ok(bp) = parse_svg_d_strict(candidate) {
            return Ok(bp);
        }
    }

    Err("failed to recover valid SVG path prefix".to_string())
}

fn parse_svg_transform(spec: &str) -> Affine {
    let mut acc = Affine::IDENTITY;
    let mut rest = spec.trim();

    while !rest.is_empty() {
        let Some(open_idx) = rest.find('(') else {
            break;
        };
        let fn_name = rest[..open_idx]
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after_open = &rest[open_idx + 1..];
        let Some(close_idx) = after_open.find(')') else {
            break;
        };
        let args_str = &after_open[..close_idx];
        let args: Vec<f64> = args_str
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .collect();

        let step = match fn_name.as_str() {
            "translate" if !args.is_empty() => {
                let tx = args[0];
                let ty = args.get(1).copied().unwrap_or(0.0);
                Affine::translate((tx, ty))
            }
            "scale" if !args.is_empty() => {
                let sx = args[0];
                let sy = args.get(1).copied().unwrap_or(sx);
                Affine::scale_non_uniform(sx, sy)
            }
            "rotate" if !args.is_empty() => {
                let rad = args[0].to_radians();
                if args.len() >= 3 {
                    let cx = args[1];
                    let cy = args[2];
                    Affine::translate((cx, cy))
                        * Affine::rotate(rad)
                        * Affine::translate((-cx, -cy))
                } else {
                    Affine::rotate(rad)
                }
            }
            "matrix" if args.len() >= 6 => {
                Affine::new([args[0], args[1], args[2], args[3], args[4], args[5]])
            }
            "skewx" if !args.is_empty() => {
                Affine::new([1.0, 0.0, args[0].to_radians().tan(), 1.0, 0.0, 0.0])
            }
            "skewy" if !args.is_empty() => {
                Affine::new([1.0, args[0].to_radians().tan(), 0.0, 1.0, 0.0, 0.0])
            }
            _ => Affine::IDENTITY,
        };
        acc *= step;
        rest = after_open[close_idx + 1..].trim();
    }

    acc
}

fn fmt_svg_coord(v: f64) -> String {
    let clean = if v.abs() < 1e-6 { 0.0 } else { v };
    if (clean - clean.round()).abs() < 1e-5 {
        format!("{}", clean.round() as i64)
    } else {
        let s = format!("{clean:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn bezpath_to_svg_d(bp: &BezPath) -> String {
    let mut parts = Vec::new();
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                parts.push(format!("M{} {}", fmt_svg_coord(p.x), fmt_svg_coord(p.y)));
            }
            PathEl::LineTo(p) => {
                parts.push(format!("L{} {}", fmt_svg_coord(p.x), fmt_svg_coord(p.y)));
            }
            PathEl::QuadTo(q1, p) => {
                parts.push(format!(
                    "Q{} {} {} {}",
                    fmt_svg_coord(q1.x),
                    fmt_svg_coord(q1.y),
                    fmt_svg_coord(p.x),
                    fmt_svg_coord(p.y)
                ));
            }
            PathEl::CurveTo(c1, c2, p) => {
                parts.push(format!(
                    "C{} {} {} {} {} {}",
                    fmt_svg_coord(c1.x),
                    fmt_svg_coord(c1.y),
                    fmt_svg_coord(c2.x),
                    fmt_svg_coord(c2.y),
                    fmt_svg_coord(p.x),
                    fmt_svg_coord(p.y)
                ));
            }
            PathEl::ClosePath => parts.push("Z".to_string()),
        }
    }
    parts.join(" ")
}

/// Fault-tolerantly decodes VecGlypher LLM output (stripping `<think>` and markdown fences,
/// recovering truncated `<path>` tags, and baking `transform` attributes) into font `Contour`s.
pub fn decode_vecglypher_svg(raw_llm_output: &str, upm: u16) -> Result<Vec<Contour>, String> {
    let stripped = strip_think_and_fences(raw_llm_output);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err("empty VecGlypher output".to_string());
    }

    let tags = extract_path_tags_tolerant(trimmed);
    let mut baked_ds = Vec::new();

    if !tags.is_empty() {
        for tag in tags {
            let Ok(mut bp) = recover_truncated_d(&tag.d) else {
                continue;
            };
            if let Some(ref tf) = tag.transform {
                let affine = parse_svg_transform(tf);
                if affine != Affine::IDENTITY {
                    bp.apply_affine(affine);
                }
            }
            let d_str = bezpath_to_svg_d(&bp);
            if !d_str.is_empty() {
                baked_ds.push(d_str);
            }
        }
    } else {
        // If no <path> tag is present, look for raw SVG path commands starting with M/m.
        let candidate = match trimmed.find(['M', 'm']) {
            Some(idx) if idx == 0 || trimmed[..idx].trim().is_empty() => &trimmed[idx..],
            _ => {
                return Err("no <path> elements or raw SVG path data found".to_string());
            }
        };
        let bp = recover_truncated_d(candidate)?;
        let d_str = bezpath_to_svg_d(&bp);
        if !d_str.is_empty() {
            baked_ds.push(d_str);
        }
    }

    if baked_ds.is_empty() {
        return Err("no recoverable SVG <path> geometry in VecGlypher output".to_string());
    }

    let combined_d = baked_ds.join(" ");
    let combined_svg = format!("<svg><path d=\"{combined_d}\"/></svg>");
    let contours = import_svg_glyph(&combined_svg, upm)?;
    let no_overlap = remove_overlap(&contours);
    let with_extrema = add_extrema(&no_overlap);
    Ok(normalize_winding(&with_extrema, true))
}

fn flatten_glyph_to_edges(
    glyph: &GlyphOutline,
    transform: impl Fn(Point) -> Point,
) -> Vec<(Point, Point)> {
    let mut edges = Vec::new();
    for contour in &glyph.contours {
        if contour.anchors.len() < 2 {
            continue;
        }
        let bp = contour.to_bezpath();
        let mut subpath_start: Option<Point> = None;
        let mut cur: Option<Point> = None;
        flatten(bp.elements().iter().copied(), 0.5, |el| match el {
            PathEl::MoveTo(p) => {
                let tp = transform(p);
                subpath_start = Some(tp);
                cur = Some(tp);
            }
            PathEl::LineTo(p) => {
                let tp = transform(p);
                if let Some(c) = cur {
                    if (tp - c).hypot2() > 1e-12 {
                        edges.push((c, tp));
                    }
                }
                cur = Some(tp);
            }
            PathEl::ClosePath => {
                if let (Some(c), Some(s)) = (cur, subpath_start) {
                    if (s - c).hypot2() > 1e-12 {
                        edges.push((c, s));
                    }
                    cur = Some(s);
                }
            }
            PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => {}
        });
        // Ensure polyline loop is closed for scanline fill even if source contour lacked ClosePath.
        if let (Some(c), Some(s)) = (cur, subpath_start) {
            if (s - c).hypot2() > 1e-12 {
                edges.push((c, s));
            }
        }
    }
    edges
}

/// Renders a glyph outline into a grayscale PNG byte buffer (white background, black ink)
/// with 14% margin padding and 2x2 supersampled non-zero winding scanline rasterization.
pub fn render_glyph_exemplar_png(
    glyph: &GlyphOutline,
    upm: u16,
    size: u32,
) -> Result<Vec<u8>, String> {
    let size = size.clamp(32, 1024);
    let upm_f = f64::from(upm.max(100));
    let raw_bbox = compute_bbox(&glyph.contours);
    let has_geometry = (raw_bbox[2] - raw_bbox[0]) > 1e-3 || (raw_bbox[3] - raw_bbox[1]) > 1e-3;

    let bbox = if has_geometry {
        raw_bbox
    } else {
        let adv = f64::from(glyph.advance_width.max(100));
        [0.0, -0.2 * upm_f, adv, 0.8 * upm_f]
    };

    let min_x = bbox[0];
    let min_y = bbox[1];
    let max_x = bbox[2];
    let max_y = bbox[3];
    let gw = (max_x - min_x).max(1.0);
    let gh = (max_y - min_y).max(1.0);

    let size_f = f64::from(size);
    let margin = 0.14 * size_f;
    let avail = (size_f - 2.0 * margin).max(1.0);
    let scale = (avail / gw).min(avail / gh);
    let cx = 0.5 * (min_x + max_x);
    let cy = 0.5 * (min_y + max_y);
    let half_canvas = 0.5 * size_f;

    let edges = flatten_glyph_to_edges(glyph, |p| {
        Point::new(
            half_canvas + (p.x - cx) * scale,
            half_canvas - (p.y - cy) * scale,
        )
    });

    let sub_w = (size as usize) * 2;
    let sub_h = (size as usize) * 2;
    let pixel_count = (size as usize) * (size as usize);
    let mut coverage = vec![0u8; pixel_count];

    if !edges.is_empty() {
        let mut crossings: Vec<(f64, i32)> = Vec::with_capacity(16);
        for sy in 0..sub_h {
            let y = (sy as f64 + 0.5) * 0.5;
            crossings.clear();

            for &(p0, p1) in &edges {
                let dir = if p0.y <= y && p1.y > y {
                    1
                } else if p1.y <= y && p0.y > y {
                    -1
                } else {
                    continue;
                };
                let dy = p1.y - p0.y;
                if dy.abs() <= 1e-12 {
                    continue;
                }
                let t = (y - p0.y) / dy;
                let x = p0.x + t * (p1.x - p0.x);
                if x.is_finite() {
                    crossings.push((x, dir));
                }
            }

            if crossings.is_empty() {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

            let py = sy / 2;
            let row_offset = py * (size as usize);
            let mut winding = 0i32;
            let mut ci = 0usize;

            for sx in 0..sub_w {
                let x = (sx as f64 + 0.5) * 0.5;
                while ci < crossings.len() && crossings[ci].0 <= x {
                    winding += crossings[ci].1;
                    ci += 1;
                }
                if winding != 0 {
                    let px = sx / 2;
                    coverage[row_offset + px] = coverage[row_offset + px].saturating_add(1);
                }
            }
        }
    }

    let mut luma_buf = vec![255u8; pixel_count];
    for (dst, &cov) in luma_buf.iter_mut().zip(coverage.iter()) {
        let ink = (u16::from(cov.min(4)) * 255) / 4;
        *dst = (255u16.saturating_sub(ink)) as u8;
    }

    let img = GrayImage::from_raw(size, size, luma_buf)
        .ok_or_else(|| "failed to construct grayscale image buffer".to_string())?;
    let mut png_bytes = Vec::new();
    DynamicImage::ImageLuma8(img)
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|e| format!("failed to encode PNG exemplar: {e}"))?;
    Ok(png_bytes)
}

fn encode_base64_rfc4648(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Renders a glyph outline into a base64-encoded grayscale PNG string.
pub fn render_glyph_exemplar_base64(
    glyph: &GlyphOutline,
    upm: u16,
    size: u32,
) -> Result<String, String> {
    let png_bytes = render_glyph_exemplar_png(glyph, upm, size)?;
    Ok(encode_base64_rfc4648(&png_bytes))
}
