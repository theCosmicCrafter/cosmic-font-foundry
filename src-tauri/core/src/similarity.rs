//! Offline PANOSE, category, weight, and script similarity & pairing engine.

use serde::Serialize;

use super::{
    inventory::{FontFamily, Origin},
    reader::LicenseTier,
};

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SimilarMatch {
    pub name: String,
    pub category: String,
    pub score: u8,
    pub reason: String,
    pub active: bool,
    pub origin: Origin,
    pub license_tier: LicenseTier,
}

fn panose_similarity(a: &[u8; 10], b: &[u8; 10]) -> Option<u32> {
    if *a == [0; 10] || *b == [0; 10] {
        return None;
    }
    if a[0] != b[0] {
        return Some(5);
    }
    // Weight the most perceptually salient PANOSE slots: family, serif, weight, proportion, contrast, x-height.
    const WEIGHTS: [u32; 10] = [6, 7, 5, 6, 5, 3, 2, 2, 2, 4];
    let mut max_score = 0u32;
    let mut earned = 0u32;
    for i in 0..10 {
        let w = WEIGHTS[i];
        max_score += w * 4;
        if a[i] == 0 || b[i] == 0 {
            earned += w * 2;
        } else {
            let diff = a[i].abs_diff(b[i]) as u32;
            earned += w * 4u32.saturating_sub(diff.min(4));
        }
    }
    Some(((earned * 45) / max_score.max(1)).min(45))
}

fn script_overlap(a: &[String], b: &[String]) -> u32 {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let shared = a.iter().filter(|s| b.contains(s)).count();
    let total = a.len().max(b.len());
    ((shared * 15) / total.max(1)) as u32
}

/// Finds visually and structurally similar font families using PANOSE, category, scripts, and variable axes.
pub fn find_similar_families(
    target_name: &str,
    families: &[FontFamily],
    limit: usize,
) -> Vec<SimilarMatch> {
    let Some(target) = families
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(target_name))
    else {
        return Vec::new();
    };

    let mut matches: Vec<SimilarMatch> = families
        .iter()
        .filter(|f| !f.name.eq_ignore_ascii_case(&target.name) && f.error.is_none())
        .filter_map(|cand| {
            let mut score = 0u32;
            let mut reasons = Vec::new();

            if cand.category == target.category {
                score += 32;
                reasons.push("Same category");
            }
            if let Some(p) = panose_similarity(&target.panose, &cand.panose) {
                score += p;
                if p >= 28 {
                    reasons.push("PANOSE match");
                }
            }
            let sc = script_overlap(&target.scripts, &cand.scripts);
            score += sc;
            if sc >= 10 {
                reasons.push("Shared scripts");
            }
            if target.is_variable && cand.is_variable {
                score += 8;
                reasons.push("Variable");
            } else if (target.styles.len() > 2 && cand.styles.len() > 2)
                || (target.styles.len() <= 2 && cand.styles.len() <= 2)
            {
                score += 5;
            }

            if score < 35 {
                return None;
            }

            Some(SimilarMatch {
                name: cand.name.clone(),
                category: cand.category.clone(),
                score: score.min(100) as u8,
                reason: if reasons.is_empty() {
                    "Related proportions".into()
                } else {
                    reasons.join(" · ")
                },
                active: cand.active,
                origin: cand.origin,
                license_tier: cand.license_tier,
            })
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    matches.truncate(limit);
    matches
}

/// Suggests complementary heading/body font pairings for the given target family.
pub fn suggest_pairings(
    target_name: &str,
    families: &[FontFamily],
    limit: usize,
) -> Vec<SimilarMatch> {
    let Some(target) = families
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(target_name))
    else {
        return Vec::new();
    };

    let wanted_cats: &[&str] = match target.category.as_str() {
        "display" | "script" | "serif" => &["sans", "mono"],
        "sans" => &["serif", "display", "mono"],
        "mono" => &["sans", "display"],
        _ => &["sans", "serif"],
    };

    let mut matches: Vec<SimilarMatch> = families
        .iter()
        .filter(|f| {
            !f.name.eq_ignore_ascii_case(&target.name)
                && f.error.is_none()
                && !f.essential
                && wanted_cats.contains(&f.category.as_str())
        })
        .map(|cand| {
            let mut score = 40u32;
            let mut reasons = Vec::new();

            if wanted_cats.first() == Some(&cand.category.as_str()) {
                score += 20;
                reasons.push("Complementary role");
            } else {
                score += 10;
            }

            let sc = script_overlap(&target.scripts, &cand.scripts);
            score += sc;
            if sc >= 10 {
                reasons.push("Shared scripts");
            }

            if cand.styles.len() >= 4 || cand.is_variable {
                score += 12;
                reasons.push("Rich weights");
            }
            if cand.license_tier == LicenseTier::FreeCommercial {
                score += 8;
                reasons.push("Commercial-safe");
            }
            if cand.active {
                score += 5;
            }

            SimilarMatch {
                name: cand.name.clone(),
                category: cand.category.clone(),
                score: score.min(100) as u8,
                reason: if reasons.is_empty() {
                    "Balanced contrast".into()
                } else {
                    reasons.join(" · ")
                },
                active: cand.active,
                origin: cand.origin,
                license_tier: cand.license_tier,
            }
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    matches.truncate(limit);
    matches
}
