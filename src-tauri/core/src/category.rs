//! Font category classification: sans, serif, script, display, mono, or other (non-Latin scripts).
//! Reads OS/2 table (PANOSE and sFamilyClass) first, then falls back to family name heuristics.

#[derive(Default, Clone, Debug)]
pub struct CategoryData {
    pub family: String,
    pub panose: [u8; 10],
    /// sFamilyClass: class in high byte, subclass in low byte.
    pub family_class: i16,
    pub unicode_ranges: [u32; 4],
    /// ulCodePageRange1 (OS/2 version 1+).
    pub codepages: Option<u32>,
}

fn contains_any(name: &str, words: &[&str]) -> bool {
    words.iter().any(|p| name.contains(p))
}

fn contains_word(name: &str, words: &[&str]) -> bool {
    name.split(|c: char| !c.is_alphanumeric())
        .any(|w| words.contains(&w))
}

const SCRIPT_NAMES: &[&str] = &[
    "arabic", "hebrew", "devanagari", "bengali", "tamil", "telugu", "kannada", "malayalam", "gujarati",
    "gurmukhi", "oriya", "sinhala", "thai", "lao", "khmer", "myanmar", "tibetan", "ethiopic", "georgian",
    "armenian", "mongolian", "naskh", "kufi", "nastaliq", "mincho", "hangul", "batang", "gulim", "dotum",
    "gungsuh", "malgun", "meiryo", "simsun", "simhei", "nsimsun", "mingliu", "pmingliu", "kaiti", "fangsong",
    "dengxian", "yahei", "jhenghei", "nirmala", "ebrima", "gadugi", "leelawadee", "javanese", "yi baiti",
    "himalaya", "estrangelo", "mangal", "kokila", "aparajita", "utsaah", "vani", "vijaya",
];

/// ulCodePageRange1 bits for Latin, Greek, and Cyrillic.
const CP_LATIN_LIKE: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 7) | (1 << 8);
/// ulCodePageRange1 bits for Thai, CJK, Arabic, Hebrew, etc.
const CP_OTHER_SCRIPTS: u32 = (1 << 5) | (1 << 6) | (1 << 16) | (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21);

pub fn classify(d: &CategoryData) -> &'static str {
    let n = d.family.to_lowercase();
    let panose_family = d.panose[0];
    let class_id = (d.family_class >> 8) as u8;

    let non_latin_cp = d.codepages.is_some_and(|c| c & CP_LATIN_LIKE == 0 && c & CP_OTHER_SCRIPTS != 0);
    let unicode_latin = d.unicode_ranges[0] & 0b11 != 0;
    if contains_any(&n, SCRIPT_NAMES)
        || contains_word(&n, &["jp", "kr", "sc", "tc", "hk", "cjk", "yu"])
        || n.starts_with("ms gothic")
        || n.starts_with("ms mincho")
        || (non_latin_cp && !unicode_latin)
    {
        return "other";
    }

    let mono_name = n.split(|c: char| !c.is_alphanumeric()).any(|w| {
        (w.ends_with("mono") && w != "monotype")
            || ["code", "courier", "typewriter", "terminal", "fixedsys"].contains(&w)
            || w.starts_with("consol")
    });
    if (panose_family == 2 && d.panose[3] == 9) || mono_name {
        return "mono";
    }

    if panose_family == 5 || class_id == 12 || contains_any(&n, &["dings", "symbol", "icons", "mdl2", "marlett", "webdings", "emoji"]) {
        return "display";
    }

    if panose_family == 3
        || class_id == 10
        || contains_any(&n, &["script", "hand", "brush", "marker", "caveat", "pacifico", "signature", "cursive", "kalam", "calligraph", "chancery", "gochi"])
    {
        return "script";
    }
    if panose_family == 4 || class_id == 9 {
        return "display";
    }

    if panose_family == 2 {
        match d.panose[1] {
            2..=10 => return "serif",
            11..=15 => return "sans",
            _ => {}
        }
    }
    match class_id {
        1..=5 | 7 => return "serif",
        8 => return "sans",
        _ => {}
    }

    if contains_any(&n, &["sans", "grotesk", "grotesque", "gothic", "helvetica", "arial", "verdana", "segoe", "inter", "roboto", "calibri", "tahoma"]) {
        return "sans";
    }
    if contains_any(&n, &["serif", "roman", "garamond", "baskerville", "times", "georgia", "bodoni", "caslon", "didot", "slab", "antiqua", "book"]) {
        return "serif";
    }
    "display"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(family: &str) -> CategoryData {
        CategoryData { family: family.into(), codepages: Some(1), unicode_ranges: [3, 0, 0, 0], ..Default::default() }
    }

    #[test]
    fn classifies_by_panose() {
        let mut x = sample("Test");
        x.panose = [2, 11, 6, 4, 2, 2, 2, 2, 2, 4];
        assert_eq!(classify(&x), "sans");
        x.panose = [2, 2, 6, 3, 5, 4, 5, 2, 3, 4];
        assert_eq!(classify(&x), "serif");
        x.panose = [2, 11, 6, 9, 2, 2, 4, 3, 2, 4];
        assert_eq!(classify(&x), "mono");
        x.panose = [3, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        assert_eq!(classify(&x), "script");
        x.panose = [4, 2, 2, 2, 2, 2, 2, 2, 2, 2];
        assert_eq!(classify(&x), "display");
        x.panose = [5, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(classify(&x), "display");
    }

    #[test]
    fn classifies_by_family_class() {
        let mut x = sample("Test");
        x.family_class = 8 << 8;
        assert_eq!(classify(&x), "sans");
        x.family_class = (1 << 8) | 4;
        assert_eq!(classify(&x), "serif");
        x.family_class = 5 << 8;
        assert_eq!(classify(&x), "serif");
        x.family_class = 10 << 8;
        assert_eq!(classify(&x), "script");
    }

    #[test]
    fn classifies_by_name() {
        assert_eq!(classify(&sample("JetBrains Mono")), "mono");
        assert_eq!(classify(&sample("Courier New")), "mono");
        assert_eq!(classify(&sample("Open Sans")), "sans");
        assert_eq!(classify(&sample("Space Grotesk")), "sans");
        assert_eq!(classify(&sample("Libre Baskerville")), "serif");
        assert_eq!(classify(&sample("Instrument Serif")), "serif");
        assert_eq!(classify(&sample("Dancing Script")), "script");
        assert_eq!(classify(&sample("Gochi Hand")), "script");
        assert_eq!(classify(&sample("Wingdings")), "display");
        assert_eq!(classify(&sample("Segoe MDL2 Assets")), "display");
    }

    #[test]
    fn name_overrides_panose_for_mono_and_scripts() {
        let mut x = sample("Fira Code");
        x.panose = [2, 11, 5, 9, 5, 0, 0, 2, 0, 4];
        assert_eq!(classify(&x), "mono");
        let mut y = sample("Noto Sans JP");
        y.panose = [2, 11, 5, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(classify(&y), "other");
    }

    #[test]
    fn classifies_non_latin_scripts() {
        assert_eq!(classify(&sample("Noto Naskh Arabic")), "other");
        assert_eq!(classify(&sample("Yu Gothic")), "other");
        assert_eq!(classify(&sample("Malgun Gothic")), "other");
        assert_eq!(classify(&sample("MS Gothic")), "other");
        assert_eq!(classify(&sample("Microsoft YaHei")), "other");
        let x = CategoryData { family: "Mystery".into(), codepages: Some(1 << 17), ..Default::default() };
        assert_eq!(classify(&x), "other");
    }

    #[test]
    fn panose_precedes_family_class_and_avoids_false_positives() {
        let mut x = sample("Century Gothic");
        x.family_class = (2 << 8) | 4;
        x.panose = [2, 11, 5, 2, 2, 2, 2, 2, 2, 4];
        assert_eq!(classify(&x), "sans");

        let mut y = sample("Lucida Sans Unicode");
        y.panose = [2, 11, 6, 2, 3, 5, 4, 2, 2, 4];
        assert_eq!(classify(&y), "sans");
        assert_ne!(classify(&sample("Monotype Corsiva")), "mono");
        assert_eq!(classify(&sample("JetBrainsMono Nerd Font")), "mono");
        assert_ne!(classify(&sample("Scope One")), "other");
    }
}
