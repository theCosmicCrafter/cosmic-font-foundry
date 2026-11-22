use std::collections::{BTreeMap, BTreeSet};

use image::GrayImage;
use serde::{Deserialize, Serialize};

use crate::model::{compute_bbox, Anchor, Contour};
use crate::pathops::{normalize_winding, simplify_contours};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TraceOptions {
    pub mode: String,
    pub threshold: u8,
    pub smooth_corners: bool,
    pub simplify_tol: f64,
    pub stroke_width: f64,
    pub invert: bool,
    pub upm: u16,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            mode: "outline".to_string(),
            threshold: 140,
            smooth_corners: true,
            simplify_tol: 2.0,
            stroke_width: 24.0,
            invert: false,
            upm: 1000,
        }
    }
}

fn build_foreground_mask(gray: &GrayImage, opts: &TraceOptions) -> Vec<bool> {
    let w = gray.width() as usize;
    let h = gray.height() as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let mut raw = vec![false; w * h];
    for (x, y, px) in gray.enumerate_pixels() {
        let luma = px.0[0];
        let fg = if opts.invert {
            luma >= opts.threshold
        } else {
            luma < opts.threshold
        };
        raw[y as usize * w + x as usize] = fg;
    }

    // Despeckle pass: remove isolated 1-pixel islands whose 4-neighbors are all false.
    let mut clean = raw.clone();
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !raw[idx] {
                continue;
            }
            let has_neighbor = (x > 0 && raw[idx - 1])
                || (x + 1 < w && raw[idx + 1])
                || (y > 0 && raw[idx - w])
                || (y + 1 < h && raw[idx + w]);
            if !has_neighbor {
                clean[idx] = false;
            }
        }
    }
    clean
}

#[inline]
fn mask_at(mask: &[bool], w: usize, h: usize, x: isize, y: isize) -> bool {
    if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
        return false;
    }
    mask[y as usize * w + x as usize]
}

/// Extracts exact pixel-crack boundary loops separating foreground and background pixels.
/// Outer loops and inner counter holes are kept as separate closed polylines.
fn extract_pixel_crack_loops(mask: &[bool], w: usize, h: usize) -> Vec<Vec<[f64; 2]>> {
    let vw = w + 1;
    let vh = h + 1;
    // Direction encoding: 0 = East (+1,0), 1 = South (0,+1), 2 = West (-1,0), 3 = North (0,-1)
    let mut edges = vec![false; vw * vh * 4];

    for vy in 0..vh {
        for vx in 0..vw {
            let base = (vy * vw + vx) * 4;
            let ix = vx as isize;
            let iy = vy as isize;
            if vx < w && mask_at(mask, w, h, ix, iy) && !mask_at(mask, w, h, ix, iy - 1) {
                edges[base] = true;
            }
            if vy < h && mask_at(mask, w, h, ix - 1, iy) && !mask_at(mask, w, h, ix, iy) {
                edges[base + 1] = true;
            }
            if vx > 0 && mask_at(mask, w, h, ix - 1, iy - 1) && !mask_at(mask, w, h, ix - 1, iy) {
                edges[base + 2] = true;
            }
            if vy > 0 && mask_at(mask, w, h, ix, iy - 1) && !mask_at(mask, w, h, ix - 1, iy - 1) {
                edges[base + 3] = true;
            }
        }
    }

    const DX: [isize; 4] = [1, 0, -1, 0];
    const DY: [isize; 4] = [0, 1, 0, -1];

    let mut loops = Vec::new();
    let total_slots = vw * vh * 4;

    for start_slot in 0..total_slots {
        if !edges[start_slot] {
            continue;
        }
        let start_v = start_slot / 4;
        let mut vx = (start_v % vw) as isize;
        let mut vy = (start_v / vw) as isize;
        let mut dir = start_slot % 4;
        let start_vx = vx;
        let start_vy = vy;

        let mut raw_pts: Vec<[f64; 2]> = Vec::new();
        let mut dirs: Vec<usize> = Vec::new();

        loop {
            let slot = (vy as usize * vw + vx as usize) * 4 + dir;
            if !edges[slot] {
                break;
            }
            edges[slot] = false;
            raw_pts.push([vx as f64, vy as f64]);
            dirs.push(dir);

            vx += DX[dir];
            vy += DY[dir];

            if vx == start_vx && vy == start_vy {
                // Check if another unvisited edge continues around this vertex only if needed;
                // reaching start closes this connected boundary cycle.
                break;
            }

            // Priority: right turn (hug foreground), straight, left turn.
            let candidates = [(dir + 1) % 4, dir, (dir + 3) % 4];
            let next_base = (vy as usize * vw + vx as usize) * 4;
            let Some(&next_dir) = candidates.iter().find(|&&d| edges[next_base + d]) else {
                break;
            };
            dir = next_dir;
        }

        // Keep only corner vertices where direction changes.
        let n = raw_pts.len();
        if n < 3 {
            continue;
        }
        let mut simplified_corners = Vec::with_capacity(n);
        for i in 0..n {
            let prev_dir = dirs[(i + n - 1) % n];
            if dirs[i] != prev_dir {
                simplified_corners.push(raw_pts[i]);
            }
        }
        if simplified_corners.len() >= 3 {
            loops.push(simplified_corners);
        }
    }

    loops
}

fn perp_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len <= 1e-12 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
}

fn douglas_peucker_open(pts: &[[f64; 2]], tol: f64) -> Vec<[f64; 2]> {
    if pts.len() <= 2 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;

    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let a = pts[start];
        let b = pts[end];
        let mut max_d = -1.0;
        let mut max_idx = start;
        for (i, &pt) in pts.iter().enumerate().take(end).skip(start + 1) {
            let d = perp_dist(pt, a, b);
            if d > max_d {
                max_d = d;
                max_idx = i;
            }
        }
        if max_d > tol && max_idx > start && max_idx < end {
            keep[max_idx] = true;
            stack.push((start, max_idx));
            stack.push((max_idx, end));
        }
    }

    pts.iter()
        .enumerate()
        .filter_map(|(i, &p)| keep[i].then_some(p))
        .collect()
}

fn douglas_peucker_closed(pts: &[[f64; 2]], tol: f64) -> Vec<[f64; 2]> {
    let n = pts.len();
    if n <= 3 || tol <= 0.0 {
        return pts.to_vec();
    }

    // Anchor at extreme corner (min x, then min y) and its farthest vertex.
    let mut a = 0usize;
    for i in 1..n {
        if pts[i][0] < pts[a][0] || (pts[i][0] == pts[a][0] && pts[i][1] < pts[a][1]) {
            a = i;
        }
    }
    let mut b = (a + 1) % n;
    let mut best_d2 = -1.0;
    for (i, &p) in pts.iter().enumerate() {
        let d2 = (p[0] - pts[a][0]).powi(2) + (p[1] - pts[a][1]).powi(2);
        if d2 > best_d2 {
            best_d2 = d2;
            b = i;
        }
    }
    if a == b {
        return pts.to_vec();
    }

    let mut chain1 = Vec::new();
    let mut idx = a;
    loop {
        chain1.push(pts[idx]);
        if idx == b {
            break;
        }
        idx = (idx + 1) % n;
    }

    let mut chain2 = Vec::new();
    let mut idx = b;
    loop {
        chain2.push(pts[idx]);
        if idx == a {
            break;
        }
        idx = (idx + 1) % n;
    }

    let s1 = douglas_peucker_open(&chain1, tol);
    let s2 = douglas_peucker_open(&chain2, tol);

    let mut combined = Vec::with_capacity(s1.len() + s2.len());
    if s1.len() > 1 {
        combined.extend_from_slice(&s1[..s1.len() - 1]);
    }
    if s2.len() > 1 {
        combined.extend_from_slice(&s2[..s2.len() - 1]);
    }

    if combined.len() >= 3 {
        combined
    } else {
        pts.to_vec()
    }
}

/// Converts a simplified polyline into a `Contour`, optionally fitting smooth cubic
/// Bézier handles at shallow vertices while keeping sharp corners unsmoothed.
fn polyline_to_contour(pts: &[[f64; 2]], closed: bool, smooth_corners: bool) -> Contour {
    let n = pts.len();
    if n < 2 || !smooth_corners {
        return Contour {
            anchors: pts.iter().map(|p| Anchor::on_curve(p[0], p[1])).collect(),
            closed,
        };
    }

    let mut anchors = Vec::with_capacity(n);
    for i in 0..n {
        let p = pts[i];
        if !closed && (i == 0 || i == n - 1) {
            anchors.push(Anchor::on_curve(p[0], p[1]));
            continue;
        }
        if n < 3 {
            anchors.push(Anchor::on_curve(p[0], p[1]));
            continue;
        }

        let prev = pts[(i + n - 1) % n];
        let next = pts[(i + 1) % n];
        let vin_x = p[0] - prev[0];
        let vin_y = p[1] - prev[1];
        let vout_x = next[0] - p[0];
        let vout_y = next[1] - p[1];
        let len_in = vin_x.hypot(vin_y);
        let len_out = vout_x.hypot(vout_y);

        if len_in <= 1e-6 || len_out <= 1e-6 {
            anchors.push(Anchor::on_curve(p[0], p[1]));
            continue;
        }

        let tin_x = vin_x / len_in;
        let tin_y = vin_y / len_in;
        let tout_x = vout_x / len_out;
        let tout_y = vout_y / len_out;
        let cos_turn = tin_x * tout_x + tin_y * tout_y;

        // Preserve sharp corners (turn angle >= ~56 deg, e.g. 90-deg corners).
        if cos_turn <= 0.55 {
            anchors.push(Anchor::on_curve(p[0], p[1]));
            continue;
        }

        let bis_x = tin_x + tout_x;
        let bis_y = tin_y + tout_y;
        let bis_len = bis_x.hypot(bis_y);
        if bis_len <= 1e-6 {
            anchors.push(Anchor::on_curve(p[0], p[1]));
            continue;
        }
        let tx = bis_x / bis_len;
        let ty = bis_y / bis_len;

        let l_in = (len_in / 3.0).min(len_out);
        let l_out = (len_out / 3.0).min(len_in);

        anchors.push(Anchor {
            x: p[0],
            y: p[1],
            h_in: [-tx * l_in, -ty * l_in],
            h_out: [tx * l_out, ty * l_out],
            smooth: true,
        });
    }

    Contour { anchors, closed }
}

/// Guo-Hall (1989) two-subiteration morphological thinning to a 1-pixel skeleton.
fn thin_guo_hall(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut cur = mask.to_vec();
    let max_iters = w + h + 4;
    let mut to_delete = Vec::new();

    for _ in 0..max_iters {
        let mut any_changed = false;
        for subiter in 0..2 {
            to_delete.clear();
            for y in 0..h {
                for x in 0..w {
                    let idx = y * w + x;
                    if !cur[idx] {
                        continue;
                    }
                    let ix = x as isize;
                    let iy = y as isize;
                    let p2 = mask_at(&cur, w, h, ix, iy - 1) as u8;
                    let p3 = mask_at(&cur, w, h, ix + 1, iy - 1) as u8;
                    let p4 = mask_at(&cur, w, h, ix + 1, iy) as u8;
                    let p5 = mask_at(&cur, w, h, ix + 1, iy + 1) as u8;
                    let p6 = mask_at(&cur, w, h, ix, iy + 1) as u8;
                    let p7 = mask_at(&cur, w, h, ix - 1, iy + 1) as u8;
                    let p8 = mask_at(&cur, w, h, ix - 1, iy) as u8;
                    let p9 = mask_at(&cur, w, h, ix - 1, iy - 1) as u8;

                    let c = ((1 - p2) & (p3 | p4))
                        + ((1 - p4) & (p5 | p6))
                        + ((1 - p6) & (p7 | p8))
                        + ((1 - p8) & (p9 | p2));
                    if c != 1 {
                        continue;
                    }

                    let n1 = (p9 | p2) + (p3 | p4) + (p5 | p6) + (p7 | p8);
                    let n2 = (p2 | p3) + (p4 | p5) + (p6 | p7) + (p8 | p9);
                    let n = n1.min(n2);
                    if !(2..=3).contains(&n) {
                        continue;
                    }

                    let cond = if subiter == 0 {
                        (p2 | p3 | (1 - p5)) & p4
                    } else {
                        (p6 | p7 | (1 - p9)) & p8
                    };
                    if cond == 0 {
                        to_delete.push(idx);
                    }
                }
            }

            if !to_delete.is_empty() {
                any_changed = true;
                for &idx in &to_delete {
                    cur[idx] = false;
                }
            }
        }
        if !any_changed {
            break;
        }
    }
    cur
}

fn edge_key(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn extract_skeleton_polylines(skel: &[bool], w: usize, h: usize) -> Vec<(Vec<[f64; 2]>, bool)> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); w * h];

    for y in 0..h {
        for x in 0..w {
            let u = y * w + x;
            if !skel[u] {
                continue;
            }
            // Cardinal links (East, South) added symmetrically.
            if x + 1 < w && skel[u + 1] {
                adj[u].push(u + 1);
                adj[u + 1].push(u);
            }
            if y + 1 < h && skel[u + w] {
                adj[u].push(u + w);
                adj[u + w].push(u);
            }
            // Diagonal links only if no shared cardinal neighbor bridges them (avoids 3-pixel triangles).
            if x + 1 < w && y + 1 < h && skel[u + w + 1] && !skel[u + 1] && !skel[u + w] {
                adj[u].push(u + w + 1);
                adj[u + w + 1].push(u);
            }
            if x > 0 && y + 1 < h && skel[u + w - 1] && !skel[u - 1] && !skel[u + w] {
                adj[u].push(u + w - 1);
                adj[u + w - 1].push(u);
            }
        }
    }

    let to_pt = |idx: usize| -> [f64; 2] { [(idx % w) as f64 + 0.5, (idx / w) as f64 + 0.5] };
    let mut visited_edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut polylines: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();

    // 1. Walk open chains starting at endpoints (deg == 1) and junctions (deg >= 3).
    for u in 0..(w * h) {
        if !skel[u] || adj[u].is_empty() || adj[u].len() == 2 {
            continue;
        }
        for i in 0..adj[u].len() {
            let v = adj[u][i];
            if !visited_edges.insert(edge_key(u, v)) {
                continue;
            }
            let mut chain = vec![to_pt(u), to_pt(v)];
            let mut prev = u;
            let mut curr = v;
            while adj[curr].len() == 2 {
                let next_opt = adj[curr]
                    .iter()
                    .copied()
                    .find(|&n| n != prev && !visited_edges.contains(&edge_key(curr, n)));
                let Some(next) = next_opt else {
                    break;
                };
                visited_edges.insert(edge_key(curr, next));
                chain.push(to_pt(next));
                prev = curr;
                curr = next;
            }
            polylines.push((chain, false));
        }
    }

    // 2. Walk any remaining unvisited edges, which form pure closed rings (all deg == 2).
    for u in 0..(w * h) {
        if !skel[u] || adj[u].is_empty() {
            continue;
        }
        for i in 0..adj[u].len() {
            let v = adj[u][i];
            if !visited_edges.insert(edge_key(u, v)) {
                continue;
            }
            let mut ring = vec![to_pt(u), to_pt(v)];
            let mut prev = u;
            let mut curr = v;
            let mut closed = false;
            loop {
                if curr == u {
                    closed = true;
                    break;
                }
                let next_opt = adj[curr]
                    .iter()
                    .copied()
                    .find(|&n| n != prev && !visited_edges.contains(&edge_key(curr, n)));
                let Some(next) = next_opt else {
                    // Check if closing back onto start `u` via its second neighbor.
                    if adj[curr].contains(&u) && !visited_edges.contains(&edge_key(curr, u)) {
                        visited_edges.insert(edge_key(curr, u));
                        closed = true;
                    }
                    break;
                };
                visited_edges.insert(edge_key(curr, next));
                if next == u {
                    closed = true;
                    break;
                }
                ring.push(to_pt(next));
                prev = curr;
                curr = next;
            }
            if closed && ring.len() >= 3 {
                polylines.push((ring, true));
            } else if ring.len() >= 2 {
                polylines.push((ring, false));
            }
        }
    }

    polylines
}

fn flip_contours_y(contours: &mut [Contour], img_h: f64) {
    for c in contours {
        for a in &mut c.anchors {
            a.y = img_h - a.y;
            a.h_in[1] = -a.h_in[1];
            a.h_out[1] = -a.h_out[1];
        }
    }
}

fn scale_and_anchor_contours(contours: &mut [Contour], upm: u16, fallback_span: f64) {
    if contours.is_empty() {
        return;
    }
    let bbox = compute_bbox(contours);
    let min_x = bbox[0];
    let min_y = bbox[1];
    let bw = bbox[2] - bbox[0];
    let bh = bbox[3] - bbox[1];

    let target_h = 0.72 * (upm.max(100) as f64);
    let ref_span = if bh > 1e-3 {
        bh
    } else if bw > 1e-3 {
        bw
    } else {
        fallback_span.max(1.0)
    };
    let scale = target_h / ref_span;

    for c in contours {
        for a in &mut c.anchors {
            a.x = (a.x - min_x) * scale;
            a.y = (a.y - min_y) * scale;
            a.h_in = [a.h_in[0] * scale, a.h_in[1] * scale];
            a.h_out = [a.h_out[0] * scale, a.h_out[1] * scale];
        }
    }
}

fn compute_px_tolerance(pts_bbox_h: f64, opts: &TraceOptions, min_tol: f64) -> f64 {
    let target_h = 0.72 * (opts.upm.max(100) as f64);
    let px_per_upm = if pts_bbox_h > 1e-3 {
        pts_bbox_h / target_h
    } else {
        1.0
    };
    let base = opts.simplify_tol.max(min_tol);
    let scaled = base * px_per_upm.max(1.0);
    if pts_bbox_h > 4.0 {
        scaled.min((pts_bbox_h * 0.08).max(min_tol))
    } else {
        base
    }
}

fn bbox_height_of_polylines<'a>(iter: impl Iterator<Item = &'a [[f64; 2]]>) -> f64 {
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pts in iter {
        for p in pts {
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
        }
    }
    if min_y.is_finite() && max_y.is_finite() {
        (max_y - min_y).max(0.0)
    } else {
        0.0
    }
}

fn trace_luma_image(gray: &GrayImage, opts: &TraceOptions) -> Vec<Contour> {
    let w = gray.width() as usize;
    let h = gray.height() as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let mask = build_foreground_mask(gray, opts);
    if !mask.iter().any(|&v| v) {
        return Vec::new();
    }

    let is_centerline = opts.mode.trim().eq_ignore_ascii_case("centerline");
    if is_centerline {
        let skel = thin_guo_hall(&mask, w, h);
        let polylines = extract_skeleton_polylines(&skel, w, h);
        if polylines.is_empty() {
            return Vec::new();
        }
        let raw_h = bbox_height_of_polylines(polylines.iter().map(|(pts, _)| pts.as_slice()));
        let tol = compute_px_tolerance(raw_h.max(h as f64 * 0.25), opts, 0.25);

        let mut contours = Vec::with_capacity(polylines.len());
        for (pts, closed) in polylines {
            let simp = if closed {
                douglas_peucker_closed(&pts, tol)
            } else {
                douglas_peucker_open(&pts, tol)
            };
            if (closed && simp.len() >= 3) || (!closed && simp.len() >= 2) {
                contours.push(polyline_to_contour(&simp, closed, opts.smooth_corners));
            }
        }
        flip_contours_y(&mut contours, h as f64);
        scale_and_anchor_contours(&mut contours, opts.upm, h as f64);
        return simplify_contours(&contours, 0.05);
    }

    let loops = extract_pixel_crack_loops(&mask, w, h);
    if loops.is_empty() {
        return Vec::new();
    }
    let raw_h = bbox_height_of_polylines(loops.iter().map(|pts| pts.as_slice()));
    let tol = compute_px_tolerance(raw_h, opts, 0.5);

    let mut contours = Vec::with_capacity(loops.len());
    for pts in loops {
        let simp = douglas_peucker_closed(&pts, tol);
        if simp.len() >= 3 {
            contours.push(polyline_to_contour(&simp, true, opts.smooth_corners));
        }
    }
    flip_contours_y(&mut contours, h as f64);
    scale_and_anchor_contours(&mut contours, opts.upm, h as f64);
    let cleaned = simplify_contours(&contours, 0.05);
    normalize_winding(&cleaned, true)
}

/// Traces an encoded raster image (PNG, JPEG, BMP, WebP) into glyph contours.
pub fn trace_image_bytes(bytes: &[u8], opts: &TraceOptions) -> Result<Vec<Contour>, String> {
    let dyn_img =
        image::load_from_memory(bytes).map_err(|e| format!("failed to decode image: {e}"))?;
    let gray = dyn_img.to_luma8();
    Ok(trace_luma_image(&gray, opts))
}

/// Slices a grid sprite sheet into `cols * rows` cells in row-major order, skips empty
/// cells, traces each populated cell, and maps them to `chars` in order.
pub fn trace_glyph_sheet(
    bytes: &[u8],
    cols: usize,
    rows: usize,
    chars: &str,
    opts: &TraceOptions,
) -> Result<BTreeMap<String, Vec<Contour>>, String> {
    if cols == 0 || rows == 0 {
        return Err("cols and rows must be >= 1".to_string());
    }

    let dyn_img =
        image::load_from_memory(bytes).map_err(|e| format!("failed to decode image: {e}"))?;
    let gray = dyn_img.to_luma8();
    let img_w = gray.width() as usize;
    let img_h = gray.height() as usize;
    if img_w < cols || img_h < rows {
        return Err("image dimensions are smaller than grid cell count".to_string());
    }

    let mut result = BTreeMap::new();
    let mut char_iter = chars.chars();
    const MIN_FG_PIXELS: usize = 4;

    'outer: for r in 0..rows {
        let y0 = (r * img_h) / rows;
        let y1 = ((r + 1) * img_h) / rows;
        let ch = (y1 - y0) as u32;
        if ch == 0 {
            continue;
        }
        for c in 0..cols {
            let x0 = (c * img_w) / cols;
            let x1 = ((c + 1) * img_w) / cols;
            let cw = (x1 - x0) as u32;
            if cw == 0 {
                continue;
            }

            let cell_img =
                image::imageops::crop_imm(&gray, x0 as u32, y0 as u32, cw, ch).to_image();
            let fg_count = cell_img
                .pixels()
                .filter(|px| {
                    if opts.invert {
                        px.0[0] >= opts.threshold
                    } else {
                        px.0[0] < opts.threshold
                    }
                })
                .count();
            if fg_count < MIN_FG_PIXELS {
                continue;
            }

            let contours = trace_luma_image(&cell_img, opts);
            if contours.is_empty() {
                continue;
            }

            let Some(ch_char) = char_iter.next() else {
                break 'outer;
            };
            result.insert(ch_char.to_string(), contours);
        }
    }

    Ok(result)
}
