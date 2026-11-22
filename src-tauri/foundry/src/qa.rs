use std::collections::{BTreeMap, HashSet};

use kurbo::{CubicBez, ParamCurve, ParamCurveExtrema, Point, Shape};
use serde::{Deserialize, Serialize};

use crate::model::{compute_bbox, Anchor, Contour, FoundryProject, GlyphOutline};
use crate::pathops::{add_extrema, normalize_winding, remove_overlap};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QaSeverity {
    Error,
    Warn,
    Info,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QaIssue {
    pub code: String,
    pub severity: QaSeverity,
    pub glyph_key: Option<String>,
    pub message: String,
    pub fixable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QaReport {
    pub score: u8,
    pub errors: usize,
    pub warnings: usize,
    pub issues: Vec<QaIssue>,
}

const MAX_COORD_BOUND: f64 = 16_000.0;

fn contour_has_missing_extrema(contours: &[Contour]) -> bool {
    for c in contours {
        let n = c.anchors.len();
        if n < 2 {
            continue;
        }
        let seg_count = if c.closed { n } else { n - 1 };
        for i in 0..seg_count {
            let a = &c.anchors[i];
            let b = &c.anchors[(i + 1) % n];
            if !a.has_h_out() && !b.has_h_in() {
                continue;
            }
            let cubic = CubicBez::new(
                a.point(),
                Point::new(a.x + a.h_out[0], a.y + a.h_out[1]),
                Point::new(b.x + b.h_in[0], b.y + b.h_in[1]),
                b.point(),
            );
            let min_x = a.x.min(b.x);
            let max_x = a.x.max(b.x);
            let min_y = a.y.min(b.y);
            let max_y = a.y.max(b.y);

            for t in cubic.extrema() {
                if t > 1e-2 && t < 1.0 - 1e-2 {
                    let pt = cubic.eval(t);
                    if pt.x < min_x - 0.75
                        || pt.x > max_x + 0.75
                        || pt.y < min_y - 0.75
                        || pt.y > max_y + 0.75
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn contour_has_wrong_winding(contours: &[Contour]) -> bool {
    let closed_valid: Vec<&Contour> = contours
        .iter()
        .filter(|c| c.closed && c.anchors.len() >= 2)
        .collect();
    if closed_valid.is_empty() {
        return false;
    }
    let normalized = normalize_winding(contours, true);
    if normalized.len() != contours.len() {
        return false;
    }
    for (orig, norm) in contours.iter().zip(normalized.iter()) {
        if !orig.closed || orig.anchors.len() < 2 {
            continue;
        }
        let orig_area = orig.to_bezpath().area();
        let norm_area = norm.to_bezpath().area();
        if orig_area.abs() > 1e-4 && norm_area.abs() > 1e-4 && orig_area * norm_area < 0.0 {
            return true;
        }
    }
    false
}

fn check_contour_coords(contours: &[Contour]) -> (bool, bool) {
    let mut has_extreme = false;
    let mut has_non_rounded = false;

    for c in contours {
        for a in &c.anchors {
            let coords = [
                a.x,
                a.y,
                a.x + a.h_in[0],
                a.y + a.h_in[1],
                a.x + a.h_out[0],
                a.y + a.h_out[1],
            ];
            for v in coords {
                if !v.is_finite() || v.abs() > MAX_COORD_BOUND {
                    has_extreme = true;
                }
            }
            for v in [a.x, a.y, a.h_in[0], a.h_in[1], a.h_out[0], a.h_out[1]] {
                if v.is_finite() && (v - v.round()).abs() > 1e-3 {
                    has_non_rounded = true;
                }
            }
        }
    }
    (has_extreme, has_non_rounded)
}

fn ligature_is_broken(
    rule: &crate::model::LigatureRule,
    project: &FoundryProject,
    cp_set: &HashSet<u32>,
    name_set: &HashSet<&str>,
) -> bool {
    if rule.components.len() < 2 {
        return true;
    }
    if rule.components.iter().any(|cp| !cp_set.contains(cp)) {
        return true;
    }
    let target_exists = project.glyphs.contains_key(&rule.glyph_key)
        || name_set.contains(rule.glyph_key.as_str())
        || (!rule.name.is_empty() && name_set.contains(rule.name.as_str()));
    !target_exists
}

/// Audits a `FoundryProject` for metric, encoding, outline, winding, extremum,
/// rounding, and OpenType feature issues, returning a scored `QaReport`.
pub fn audit_project(project: &FoundryProject) -> QaReport {
    let mut issues = Vec::new();

    // 1. Metrics & metadata sanity
    if project.metrics.upm < 16 || project.metrics.upm > 16384 {
        issues.push(QaIssue {
            code: "METRICS_UPM_RANGE".to_string(),
            severity: QaSeverity::Error,
            glyph_key: None,
            message: format!(
                "UPM {} is outside valid OpenType bounds [16, 16384]",
                project.metrics.upm
            ),
            fixable: false,
        });
    }
    if project.metrics.descender > 0 {
        issues.push(QaIssue {
            code: "METRICS_INVERTED_DESCENDER".to_string(),
            severity: QaSeverity::Error,
            glyph_key: None,
            message: format!(
                "Descender {} should be negative (below baseline)",
                project.metrics.descender
            ),
            fixable: true,
        });
    }
    if project.metrics.ascender <= project.metrics.descender {
        issues.push(QaIssue {
            code: "METRICS_ASCENDER_LE_DESCENDER".to_string(),
            severity: QaSeverity::Error,
            glyph_key: None,
            message: format!(
                "Ascender ({}) must be greater than descender ({})",
                project.metrics.ascender, project.metrics.descender
            ),
            fixable: false,
        });
    }
    if project.family_name.trim().is_empty() {
        issues.push(QaIssue {
            code: "META_EMPTY_FAMILY_NAME".to_string(),
            severity: QaSeverity::Warn,
            glyph_key: None,
            message: "Family name is empty".to_string(),
            fixable: false,
        });
    }

    // 2. Missing space (0x20)
    if !project.glyphs.values().any(|g| g.codepoint == 0x20) {
        issues.push(QaIssue {
            code: "MISSING_SPACE_GLYPH".to_string(),
            severity: QaSeverity::Warn,
            glyph_key: None,
            message: "Missing space (U+0020) glyph".to_string(),
            fixable: true,
        });
    }

    // 3. Basic Latin coverage
    let cp_set: HashSet<u32> = project
        .glyphs
        .values()
        .filter(|g| g.codepoint > 0)
        .map(|g| g.codepoint)
        .collect();
    let missing_upper: Vec<char> = ('A'..='Z')
        .filter(|c| !cp_set.contains(&(*c as u32)))
        .collect();
    if !missing_upper.is_empty() {
        let sample: String = missing_upper.iter().take(6).collect();
        issues.push(QaIssue {
            code: "COVERAGE_UPPERCASE".to_string(),
            severity: QaSeverity::Info,
            glyph_key: None,
            message: format!(
                "Missing {} uppercase Latin glyph(s), e.g. {sample}",
                missing_upper.len()
            ),
            fixable: false,
        });
    }
    let missing_lower: Vec<char> = ('a'..='z')
        .filter(|c| !cp_set.contains(&(*c as u32)))
        .collect();
    if !missing_lower.is_empty() {
        let sample: String = missing_lower.iter().take(6).collect();
        issues.push(QaIssue {
            code: "COVERAGE_LOWERCASE".to_string(),
            severity: QaSeverity::Info,
            glyph_key: None,
            message: format!(
                "Missing {} lowercase Latin glyph(s), e.g. {sample}",
                missing_lower.len()
            ),
            fixable: false,
        });
    }

    // 4-7, 9. Per-glyph checks + duplicate codepoints
    let mut cp_keys: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut name_set: HashSet<&str> = HashSet::new();

    for (key, glyph) in &project.glyphs {
        if !glyph.name.is_empty() {
            name_set.insert(glyph.name.as_str());
        }
        if glyph.codepoint > 0 {
            cp_keys
                .entry(glyph.codepoint)
                .or_default()
                .push(key.clone());
        }

        if glyph.advance_width < 0 {
            issues.push(QaIssue {
                code: "GLYPH_NEGATIVE_ADVANCE".to_string(),
                severity: QaSeverity::Error,
                glyph_key: Some(key.clone()),
                message: format!(
                    "Glyph '{key}' has negative advance width ({})",
                    glyph.advance_width
                ),
                fixable: true,
            });
        } else if glyph.advance_width == 0 && !glyph.contours.is_empty() {
            issues.push(QaIssue {
                code: "GLYPH_ZERO_ADVANCE".to_string(),
                severity: QaSeverity::Warn,
                glyph_key: Some(key.clone()),
                message: format!("Non-empty glyph '{key}' has zero advance width"),
                fixable: true,
            });
        }

        if !glyph.contours.is_empty() {
            if contour_has_wrong_winding(&glyph.contours) {
                issues.push(QaIssue {
                    code: "GLYPH_WRONG_WINDING".to_string(),
                    severity: QaSeverity::Warn,
                    glyph_key: Some(key.clone()),
                    message: format!(
                        "Glyph '{key}' has inverted contour winding (TrueType expects CW outer / CCW holes)"
                    ),
                    fixable: true,
                });
            }

            if contour_has_missing_extrema(&glyph.contours) {
                issues.push(QaIssue {
                    code: "GLYPH_MISSING_EXTREMA".to_string(),
                    severity: QaSeverity::Warn,
                    glyph_key: Some(key.clone()),
                    message: format!("Glyph '{key}' has Bézier curves missing extrema points"),
                    fixable: true,
                });
            }

            let (has_extreme, has_non_rounded) = check_contour_coords(&glyph.contours);
            if has_extreme {
                issues.push(QaIssue {
                    code: "GLYPH_EXTREME_COORDS".to_string(),
                    severity: QaSeverity::Warn,
                    glyph_key: Some(key.clone()),
                    message: format!(
                        "Glyph '{key}' has non-finite or out-of-bounds coordinates (> 16000)"
                    ),
                    fixable: true,
                });
            }
            if has_non_rounded {
                issues.push(QaIssue {
                    code: "GLYPH_NON_ROUNDED_COORDS".to_string(),
                    severity: QaSeverity::Warn,
                    glyph_key: Some(key.clone()),
                    message: format!("Glyph '{key}' has fractional anchor/handle coordinates"),
                    fixable: true,
                });
            }
        }
    }

    // 8. Broken ligature rules
    for rule in &project.ligatures {
        if ligature_is_broken(rule, project, &cp_set, &name_set) {
            issues.push(QaIssue {
                code: "LIGATURE_BROKEN_RULE".to_string(),
                severity: QaSeverity::Warn,
                glyph_key: Some(rule.glyph_key.clone()),
                message: format!(
                    "Ligature '{}' points to missing glyph or incomplete components",
                    rule.name
                ),
                fixable: true,
            });
        }
    }

    // 9. Duplicate unicode codepoints
    for (cp, keys) in cp_keys {
        if keys.len() > 1 {
            issues.push(QaIssue {
                code: "GLYPH_DUPLICATE_CODEPOINT".to_string(),
                severity: QaSeverity::Error,
                glyph_key: Some(keys.join(",")),
                message: format!(
                    "Duplicate Unicode codepoint U+{cp:04X} assigned to keys: {}",
                    keys.join(", ")
                ),
                fixable: false,
            });
        }
    }

    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut score = 100_i32;

    for issue in &issues {
        match issue.severity {
            QaSeverity::Error => {
                errors += 1;
                score -= 15;
            }
            QaSeverity::Warn => {
                warnings += 1;
                score -= 5;
            }
            QaSeverity::Info => {
                score -= 1;
            }
        }
    }

    QaReport {
        score: score.clamp(0, 100) as u8,
        errors,
        warnings,
        issues,
    }
}

fn sanitize_and_round_contours(contours: &mut [Contour]) {
    let clamp_round = |v: f64| -> f64 {
        if v.is_finite() {
            v.clamp(-MAX_COORD_BOUND, MAX_COORD_BOUND).round()
        } else {
            0.0
        }
    };
    for c in contours {
        for a in &mut c.anchors {
            a.x = clamp_round(a.x);
            a.y = clamp_round(a.y);
            a.h_in[0] = clamp_round(a.h_in[0]);
            a.h_in[1] = clamp_round(a.h_in[1]);
            a.h_out[0] = clamp_round(a.h_out[0]);
            a.h_out[1] = clamp_round(a.h_out[1]);
        }
    }
}

fn pre_sanitize_contours(contours: &[Contour]) -> Vec<Contour> {
    let fix = |v: f64| -> f64 {
        if v.is_finite() {
            v.clamp(-MAX_COORD_BOUND, MAX_COORD_BOUND)
        } else {
            0.0
        }
    };
    contours
        .iter()
        .map(|c| Contour {
            anchors: c
                .anchors
                .iter()
                .map(|a| Anchor {
                    x: fix(a.x),
                    y: fix(a.y),
                    h_in: [fix(a.h_in[0]), fix(a.h_in[1])],
                    h_out: [fix(a.h_out[0]), fix(a.h_out[1])],
                    smooth: a.smooth,
                })
                .collect(),
            closed: c.closed,
        })
        .collect()
}

/// Automatically repairs fixable issues in `project` (metrics sign, missing space,
/// negative/zero advance widths, overlaps, extrema, winding, coordinate rounding,
/// and broken ligatures) and returns `(fixed_count, updated_report)`.
pub fn autofix_project(project: &mut FoundryProject) -> (usize, QaReport) {
    let mut fixed_count = 0usize;

    if project.metrics.descender > 0 {
        project.metrics.descender = -project.metrics.descender;
        fixed_count += 1;
    }

    if !project.glyphs.values().any(|g| g.codepoint == 0x20) {
        let space_key = if project.glyphs.contains_key("space") {
            " ".to_string()
        } else {
            "space".to_string()
        };
        let space_adv = ((project.metrics.upm as i32) / 3).max(200);
        project.glyphs.insert(
            space_key,
            GlyphOutline {
                codepoint: 0x20,
                name: "space".to_string(),
                advance_width: space_adv,
                lsb: 0,
                contours: Vec::new(),
            },
        );
        fixed_count += 1;
    }

    let default_adv = ((project.metrics.upm as i32) / 2).max(250);
    for glyph in project.glyphs.values_mut() {
        if !glyph.contours.is_empty() {
            let needs_winding = contour_has_wrong_winding(&glyph.contours);
            let needs_extrema = contour_has_missing_extrema(&glyph.contours);
            let (needs_extreme, needs_round) = check_contour_coords(&glyph.contours);

            if needs_extreme {
                glyph.contours = pre_sanitize_contours(&glyph.contours);
            }
            let deduped = remove_overlap(&glyph.contours);
            let with_extrema = add_extrema(&deduped);
            let mut normalized = normalize_winding(&with_extrema, true);
            sanitize_and_round_contours(&mut normalized);

            let changed = needs_winding
                || needs_extrema
                || needs_extreme
                || needs_round
                || normalized != glyph.contours;

            if !normalized.is_empty() {
                glyph.contours = normalized;
            } else {
                sanitize_and_round_contours(&mut glyph.contours);
            }
            glyph.lsb = compute_bbox(&glyph.contours)[0].round() as i32;
            if changed {
                fixed_count += 1;
            }
        }

        if glyph.advance_width < 0 || (glyph.advance_width == 0 && !glyph.contours.is_empty()) {
            let new_adv = if glyph.contours.is_empty() {
                default_adv
            } else {
                let bb = compute_bbox(&glyph.contours);
                (bb[2] + 60.0).round().max(200.0) as i32
            };
            glyph.advance_width = new_adv;
            fixed_count += 1;
        }
    }

    if !project.ligatures.is_empty() {
        let cp_set: HashSet<u32> = project
            .glyphs
            .values()
            .filter(|g| g.codepoint > 0)
            .map(|g| g.codepoint)
            .collect();
        let name_set: HashSet<String> = project
            .glyphs
            .values()
            .filter(|g| !g.name.is_empty())
            .map(|g| g.name.clone())
            .collect();
        let key_set: HashSet<String> = project.glyphs.keys().cloned().collect();

        let before = project.ligatures.len();
        project.ligatures.retain(|rule| {
            rule.components.len() >= 2
                && rule.components.iter().all(|cp| cp_set.contains(cp))
                && (key_set.contains(&rule.glyph_key)
                    || name_set.contains(&rule.glyph_key)
                    || (!rule.name.is_empty() && name_set.contains(&rule.name)))
        });
        fixed_count += before.saturating_sub(project.ligatures.len());
    }

    let report = audit_project(project);
    (fixed_count, report)
}
