use std::collections::BTreeMap;

use kurbo::{BezPath, PathEl, Point, Shape};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub h_in: [f64; 2],
    pub h_out: [f64; 2],
    pub smooth: bool,
}

impl Anchor {
    pub fn on_curve(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            h_in: [0.0, 0.0],
            h_out: [0.0, 0.0],
            smooth: false,
        }
    }

    pub fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn has_h_in(&self) -> bool {
        self.h_in[0].hypot(self.h_in[1]) > 1e-6
    }

    pub fn has_h_out(&self) -> bool {
        self.h_out[0].hypot(self.h_out[1]) > 1e-6
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Contour {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GlyphOutline {
    pub codepoint: u32,
    pub name: String,
    pub advance_width: i32,
    pub lsb: i32,
    pub contours: Vec<Contour>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FontMetrics {
    pub upm: u16,
    pub ascender: i16,
    pub descender: i16,
    pub x_height: i16,
    pub cap_height: i16,
    pub line_gap: i16,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KerningPair {
    pub left: u32,
    pub right: u32,
    pub value: i16,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LigatureRule {
    pub name: String,
    pub components: Vec<u32>,
    pub glyph_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinkedMetricRule {
    pub target: String,
    pub lsb_ref: Option<String>,
    pub rsb_ref: Option<String>,
    pub mirror: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GlyphAnchor {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FoundryProject {
    pub version: u32,
    pub family_name: String,
    pub style_name: String,
    pub designer: String,
    pub license: String,
    pub metrics: FontMetrics,
    pub glyphs: BTreeMap<String, GlyphOutline>,
    pub kerning: Vec<KerningPair>,
    #[serde(default)]
    pub ligatures: Vec<LigatureRule>,
    #[serde(default)]
    pub linked_metrics: Vec<LinkedMetricRule>,
    #[serde(default)]
    pub anchors: BTreeMap<String, Vec<GlyphAnchor>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModularForgeRequest {
    pub family_name: String,
    pub style_name: String,
    pub weight: f64,
    pub contrast: f64,
    pub slant: f64,
    pub tracking: f64,
    pub corner: f64,
    pub shape: String,
    pub glyphs: BTreeMap<String, Vec<Vec<u8>>>,
    pub glyph_widths: BTreeMap<String, i32>,
    pub cols: usize,
    pub rows: usize,
    pub remove_overlap: bool,
    pub smooth_corners: bool,
    pub optical_offset: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CompiledFontResult {
    pub ttf_bytes: Vec<u8>,
    pub glyph_count: usize,
    pub total_contours: usize,
    pub total_points: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GlyphInspectInfo {
    pub codepoint: u32,
    pub glyph_name: String,
    pub svg_path: String,
    pub view_box: [f64; 4],
    pub upm: u16,
    pub ascender: i16,
    pub descender: i16,
    pub advance_width: i32,
    pub lsb: i32,
    pub bbox: [f64; 4],
    pub contour_count: usize,
    pub point_count: usize,
    pub contours: Vec<Contour>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GlyphTransformOp {
    RemoveOverlap,
    Offset {
        delta: f64,
        round_joins: bool,
    },
    RoundCorners {
        radius: f64,
        chamfer: bool,
    },
    AddExtrema,
    Simplify {
        tolerance: f64,
    },
    NormalizeWinding,
    CalligraphicSweep {
        size: f64,
        angle_deg: f64,
        roundness: f64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubFontRepo {
    pub full_name: String,
    pub name: String,
    pub description: String,
    pub stars: u64,
    pub license: String,
    pub html_url: String,
    pub releases_url: String,
    pub updated_at: String,
}

fn push_segment_to_bezpath(bp: &mut BezPath, from: &Anchor, to: &Anchor) {
    if !from.has_h_out() && !to.has_h_in() {
        bp.line_to((to.x, to.y));
    } else {
        bp.curve_to(
            (from.x + from.h_out[0], from.y + from.h_out[1]),
            (to.x + to.h_in[0], to.y + to.h_in[1]),
            (to.x, to.y),
        );
    }
}

fn are_opposite_collinear(a: [f64; 2], b: [f64; 2]) -> bool {
    let la = a[0].hypot(a[1]);
    let lb = b[0].hypot(b[1]);
    if la <= 1e-6 || lb <= 1e-6 {
        return false;
    }
    let dot = (a[0] * b[0] + a[1] * b[1]) / (la * lb);
    dot < -0.995
}

impl Contour {
    pub fn to_bezpath(&self) -> BezPath {
        let mut bp = BezPath::new();
        let Some(first) = self.anchors.first() else {
            return bp;
        };
        bp.move_to((first.x, first.y));
        for win in self.anchors.windows(2) {
            push_segment_to_bezpath(&mut bp, &win[0], &win[1]);
        }
        if self.closed && self.anchors.len() >= 2 {
            if let Some(last) = self.anchors.last() {
                if last.has_h_out() || first.has_h_in() {
                    bp.curve_to(
                        (last.x + last.h_out[0], last.y + last.h_out[1]),
                        (first.x + first.h_in[0], first.y + first.h_in[1]),
                        (first.x, first.y),
                    );
                } else if (last.x - first.x).hypot(last.y - first.y) > 1e-6 {
                    bp.line_to((first.x, first.y));
                }
            }
            bp.close_path();
        } else if self.closed {
            bp.close_path();
        }
        bp
    }

    pub fn from_bezpath(bp: &BezPath) -> Vec<Contour> {
        let mut contours = Vec::new();
        let mut current: Option<Vec<Anchor>> = None;
        let mut cur_pt = Point::ORIGIN;

        for el in bp.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    if let Some(anchors) = current.take() {
                        if !anchors.is_empty() {
                            contours.push(Contour {
                                anchors,
                                closed: false,
                            });
                        }
                    }
                    cur_pt = p;
                    current = Some(vec![Anchor::on_curve(p.x, p.y)]);
                }
                PathEl::LineTo(p) => {
                    let anchors =
                        current.get_or_insert_with(|| vec![Anchor::on_curve(cur_pt.x, cur_pt.y)]);
                    anchors.push(Anchor::on_curve(p.x, p.y));
                    cur_pt = p;
                }
                PathEl::QuadTo(q1, p) => {
                    let c1 = cur_pt + (q1 - cur_pt) * (2.0 / 3.0);
                    let c2 = p + (q1 - p) * (2.0 / 3.0);
                    let anchors =
                        current.get_or_insert_with(|| vec![Anchor::on_curve(cur_pt.x, cur_pt.y)]);
                    if let Some(prev) = anchors.last_mut() {
                        prev.h_out = [c1.x - cur_pt.x, c1.y - cur_pt.y];
                    }
                    let mut next = Anchor::on_curve(p.x, p.y);
                    next.h_in = [c2.x - p.x, c2.y - p.y];
                    anchors.push(next);
                    cur_pt = p;
                }
                PathEl::CurveTo(c1, c2, p) => {
                    let anchors =
                        current.get_or_insert_with(|| vec![Anchor::on_curve(cur_pt.x, cur_pt.y)]);
                    if let Some(prev) = anchors.last_mut() {
                        prev.h_out = [c1.x - cur_pt.x, c1.y - cur_pt.y];
                    }
                    let mut next = Anchor::on_curve(p.x, p.y);
                    next.h_in = [c2.x - p.x, c2.y - p.y];
                    anchors.push(next);
                    cur_pt = p;
                }
                PathEl::ClosePath => {
                    if let Some(mut anchors) = current.take() {
                        if anchors.len() > 1 {
                            let same_as_start = {
                                let first = &anchors[0];
                                let last = &anchors[anchors.len() - 1];
                                (first.x - last.x).hypot(first.y - last.y) < 1e-5
                            };
                            if same_as_start {
                                if let Some(last) = anchors.pop() {
                                    anchors[0].h_in = last.h_in;
                                }
                            }
                        }
                        for anchor in &mut anchors {
                            anchor.smooth = are_opposite_collinear(anchor.h_in, anchor.h_out);
                        }
                        if !anchors.is_empty() {
                            if let Some(first) = anchors.first() {
                                cur_pt = first.point();
                            }
                            contours.push(Contour {
                                anchors,
                                closed: true,
                            });
                        }
                    }
                }
            }
        }

        if let Some(mut anchors) = current.take() {
            for anchor in &mut anchors {
                anchor.smooth = are_opposite_collinear(anchor.h_in, anchor.h_out);
            }
            if !anchors.is_empty() {
                contours.push(Contour {
                    anchors,
                    closed: false,
                });
            }
        }

        contours
    }
}

fn fmt_num(v: f64) -> String {
    let rounded = (v * 1000.0).round() / 1000.0;
    let clean = if rounded == -0.0 { 0.0 } else { rounded };
    if (clean - clean.round()).abs() < 1e-6 {
        format!("{}", clean.round() as i64)
    } else {
        let s = format!("{clean:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn contours_to_svg_d(contours: &[Contour], ascender: f64) -> String {
    let mut parts = Vec::new();
    for contour in contours {
        let Some(first) = contour.anchors.first() else {
            continue;
        };
        parts.push(format!(
            "M{} {}",
            fmt_num(first.x),
            fmt_num(ascender - first.y)
        ));
        for win in contour.anchors.windows(2) {
            let from = &win[0];
            let to = &win[1];
            if !from.has_h_out() && !to.has_h_in() {
                parts.push(format!("L{} {}", fmt_num(to.x), fmt_num(ascender - to.y)));
            } else {
                parts.push(format!(
                    "C{} {} {} {} {} {}",
                    fmt_num(from.x + from.h_out[0]),
                    fmt_num(ascender - (from.y + from.h_out[1])),
                    fmt_num(to.x + to.h_in[0]),
                    fmt_num(ascender - (to.y + to.h_in[1])),
                    fmt_num(to.x),
                    fmt_num(ascender - to.y),
                ));
            }
        }
        if contour.closed {
            if contour.anchors.len() >= 2 {
                if let Some(last) = contour.anchors.last() {
                    if last.has_h_out() || first.has_h_in() {
                        parts.push(format!(
                            "C{} {} {} {} {} {}",
                            fmt_num(last.x + last.h_out[0]),
                            fmt_num(ascender - (last.y + last.h_out[1])),
                            fmt_num(first.x + first.h_in[0]),
                            fmt_num(ascender - (first.y + first.h_in[1])),
                            fmt_num(first.x),
                            fmt_num(ascender - first.y),
                        ));
                    }
                }
            }
            parts.push("Z".to_string());
        }
    }
    parts.join(" ")
}

pub fn compute_bbox(contours: &[Contour]) -> [f64; 4] {
    let mut combined: Option<kurbo::Rect> = None;
    for contour in contours {
        if contour.anchors.is_empty() {
            continue;
        }
        let rect = if contour.anchors.len() == 1 {
            let p = contour.anchors[0].point();
            kurbo::Rect::from_points(p, p)
        } else {
            contour.to_bezpath().bounding_box()
        };
        if rect.x0.is_finite() && rect.y0.is_finite() && rect.x1.is_finite() && rect.y1.is_finite()
        {
            combined = Some(match combined {
                Some(acc) => acc.union(rect),
                None => rect,
            });
        }
    }
    match combined {
        Some(r) => [r.x0, r.y0, r.x1, r.y1],
        None => [0.0, 0.0, 0.0, 0.0],
    }
}
