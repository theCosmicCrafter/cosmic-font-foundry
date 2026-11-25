//! Multi-repository open-source font catalog parsers and on-demand search providers:
//! - Homebrew Open Font Registry (`formulae.brew.sh/api/cask.json`, 415+ non-Google HTTPS fonts)
//! - Scoop Open Fonts Bucket (`matthewjberger/scoop-nerd-fonts`, 367 manifests)
//! - FontGet-Sources (`font-squirrel.json` 608 commercial-vetted + `league-of-moveable-type.json` 18 OFL)
//! - Open Font Repository (`GustavoPeredo/open-font-repository/main/fonts.json`)
//! - Live Fontesk WordPress REST Search (`fontesk.com/wp-json/wp/v2/posts`, 13,000+ editorial fonts)

use super::{
    http_client, is_adobe_or_dev_family, map_generic_category, parse_json, url_has_ext,
    GoogleFontEntry, CACHE_TTL,
};
use serde_json::Value;
use std::{collections::HashSet, fs, path::Path, time::SystemTime};

pub(super) const HOMEBREW_CASKS_URL: &str = "https://formulae.brew.sh/api/cask.json";
pub(super) const SCOOP_BUCKET_API_URL: &str =
    "https://api.github.com/repos/matthewjberger/scoop-nerd-fonts/contents/bucket";
pub(super) const FONTGET_FONTSQUIRREL_URL: &str =
    "https://raw.githubusercontent.com/Graphixa/FontGet-Sources/main/sources/font-squirrel.json";
pub(super) const FONTGET_LEAGUE_URL: &str =
    "https://raw.githubusercontent.com/Graphixa/FontGet-Sources/main/sources/league-of-moveable-type.json";
pub(super) const OPEN_FONT_REPO_URL: &str =
    "https://raw.githubusercontent.com/GustavoPeredo/open-font-repository/main/fonts.json";

fn clean_opt_string(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn infer_category_from_metadata(name: &str, desc: &str) -> String {
    let combined = format!("{name} {desc}").to_ascii_lowercase();
    if combined.contains("mono")
        || combined.contains("monospace")
        || combined.contains("coding")
        || combined.contains("programming")
        || combined.contains("terminal")
        || combined.contains("typewriter")
    {
        "mono".into()
    } else if combined.contains("serif") || combined.contains("slab") {
        if combined.contains("sans-serif") || combined.contains("sans serif") {
            "sans".into()
        } else {
            "serif".into()
        }
    } else if combined.contains("script")
        || combined.contains("handwrit")
        || combined.contains("calligraph")
        || combined.contains("cursive")
        || combined.contains("brush")
    {
        "script".into()
    } else if combined.contains("sans")
        || combined.contains("grotesk")
        || combined.contains("grotesque")
        || combined.contains("gothic")
        || combined.contains("ui ")
    {
        "sans".into()
    } else {
        "display".into()
    }
}

fn infer_scripts_from_text(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut scripts = vec!["Latin".to_string()];
    for (needle, label) in [
        ("japanese", "Japanese"),
        ("kana", "Japanese"),
        ("moji", "Japanese"),
        ("cjk", "CJK"),
        ("chinese", "CJK"),
        ("korean", "Korean"),
        ("hangul", "Korean"),
        ("arabic", "Arabic"),
        ("cyrillic", "Cyrillic"),
        ("greek", "Greek"),
        ("devanagari", "Devanagari"),
        ("hebrew", "Hebrew"),
        ("symbol", "Symbols"),
        ("icon", "Symbols"),
        ("nerd", "Symbols"),
    ] {
        if lower.contains(needle) && !scripts.iter().any(|s| s == label) {
            scripts.push(label.to_string());
        }
    }
    scripts
}

fn infer_tags_from_metadata(name: &str, desc: &str, homepage: Option<&str>) -> Vec<String> {
    let combined = format!("{} {} {}", name, desc, homepage.unwrap_or("")).to_ascii_lowercase();
    let mut tags = Vec::new();
    for (needle, tag) in [
        ("sil.org", "sil"),
        ("int10h.org", "retro"),
        ("int10h.org", "dos"),
        ("fontlibrary.org", "fontlibrary"),
        ("velvetyne", "velvetyne"),
        ("moji-waku", "japanese"),
        ("dyslex", "dyslexia"),
        ("hyperlegible", "accessible"),
        ("bitmap", "bitmap"),
        ("pixel", "pixel"),
        ("vga", "retro"),
        ("math", "math"),
        ("stix", "math"),
        ("music", "music"),
        ("bravura", "music"),
        ("nerd", "nerd"),
        ("ligature", "ligatures"),
        ("variable", "variable"),
    ] {
        if combined.contains(needle) && !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
    }
    tags
}

fn format_cask_token_name(raw: &str) -> String {
    raw.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_homebrew_fonts_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for (idx, item) in arr.iter().enumerate() {
        let Some(token) = item["token"].as_str().map(str::trim) else {
            continue;
        };
        if !token.starts_with("font-")
            || item["deprecated"].as_bool() == Some(true)
            || item["disabled"].as_bool() == Some(true)
        {
            continue;
        }
        let Some(url) = item["url"].as_str().map(str::trim) else {
            continue;
        };
        if !url.starts_with("https://")
            || url.contains("github.com/google/fonts")
            || url.contains("fonts.gstatic.com")
            || !(url_has_ext(url, ".zip") || url_has_ext(url, ".ttf") || url_has_ext(url, ".otf"))
        {
            continue;
        }

        let name = item["name"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format_cask_token_name(token.trim_start_matches("font-")));
        if name.is_empty() || !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }

        let desc = clean_opt_string(&item["desc"]);
        let homepage = clean_opt_string(&item["homepage"]);
        let version_tag = clean_opt_string(&item["version"]);
        let sha256 = clean_opt_string(&item["sha256"]).filter(|h| {
            !h.eq_ignore_ascii_case("no_check")
                && h.len() == 64
                && h.bytes().all(|b| b.is_ascii_hexdigit())
        });
        let desc_text = desc.as_deref().unwrap_or("");
        let category = infer_category_from_metadata(&name, desc_text);
        let scripts = infer_scripts_from_text(&format!("{name} {desc_text}"));
        let tags = infer_tags_from_metadata(&name, desc_text, homepage.as_deref());
        let source = if is_adobe_or_dev_family(&name) {
            "adobe_dev"
        } else {
            "homebrew"
        };

        out.push(GoogleFontEntry {
            name,
            category,
            styles: 1,
            popularity: 1_500 + (idx as i64),
            date: String::new(),
            scripts,
            designers: Vec::new(),
            is_variable: desc_text.to_ascii_lowercase().contains("variable"),
            axes: Vec::new(),
            variants: vec!["400".into()],
            source: source.into(),
            source_id: token.to_string(),
            source_label: Some("Homebrew Open".into()),
            version_tag,
            preview_url: None,
            download_urls: vec![url.to_string()],
            description: desc,
            homepage,
            license: Some("Open Source (see homepage)".into()),
            tags,
            sha256,
            redistribution_allowed: Some(true),
            download_mode: Some("direct".into()),
        });
    }

    out
}

pub(super) async fn fetch_homebrew_fonts(
    client: &reqwest::Client,
    data_dir: &Path,
    force: bool,
) -> Result<Vec<GoogleFontEntry>, String> {
    let slim_cache = data_dir.join("homebrew-fonts-slim.json");
    let fresh = fs::metadata(&slim_cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|d| d < CACHE_TTL);

    if fresh && !force {
        if let Some(entries) = fs::read_to_string(&slim_cache)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<GoogleFontEntry>>(&t).ok())
        {
            return Ok(entries);
        }
    }

    let fetched = async {
        let resp = client
            .get(HOMEBREW_CASKS_URL)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(format!("HTTP {} from {HOMEBREW_CASKS_URL}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        let val = parse_json(&text)?;
        let entries = parse_homebrew_fonts_catalog(&val);
        if entries.is_empty() {
            return Err("Homebrew cask feed contained no font entries".into());
        }
        if let Some(parent) = slim_cache.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(slim_json) = serde_json::to_string(&entries) {
            let _ = fs::write(&slim_cache, slim_json);
        }
        Ok::<_, String>(entries)
    }
    .await;

    match fetched {
        Ok(entries) => Ok(entries),
        Err(err) => fs::read_to_string(&slim_cache)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<GoogleFontEntry>>(&t).ok())
            .ok_or(err),
    }
}

pub fn parse_scoop_bucket_index(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for (idx, item) in arr.iter().enumerate() {
        if item["type"].as_str() != Some("file") {
            continue;
        }
        let Some(filename) = item["name"].as_str().map(str::trim) else {
            continue;
        };
        if !filename.to_ascii_lowercase().ends_with(".json") {
            continue;
        }
        let stem = filename
            .strip_suffix(".json")
            .or_else(|| filename.strip_suffix(".JSON"))
            .unwrap_or(filename);
        let display_name = format_cask_token_name(stem);
        if display_name.is_empty() || !seen.insert(display_name.to_ascii_lowercase()) {
            continue;
        }

        let raw_url = item["download_url"]
            .as_str()
            .map(str::trim)
            .filter(|u| u.starts_with("https://raw.githubusercontent.com/"))
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!("https://raw.githubusercontent.com/matthewjberger/scoop-nerd-fonts/master/bucket/{filename}")
            });
        let category = infer_category_from_metadata(&display_name, "");
        let lower = display_name.to_ascii_lowercase();
        let mut tags = vec!["scoop".to_string()];
        if lower.contains("nf") || lower.contains("nerd") {
            tags.push("nerd".into());
        }

        out.push(GoogleFontEntry {
            name: display_name,
            category,
            styles: 1,
            popularity: 2_500 + (idx as i64),
            date: String::new(),
            scripts: if lower.contains("nf") {
                vec!["Latin".into(), "Symbols".into()]
            } else {
                vec!["Latin".into()]
            },
            designers: Vec::new(),
            is_variable: false,
            axes: Vec::new(),
            variants: vec!["400".into()],
            source: "scoop".into(),
            source_id: raw_url,
            source_label: Some("Scoop Fonts".into()),
            version_tag: None,
            preview_url: None,
            download_urls: Vec::new(),
            description: Some(
                "Verified manifest from Scoop Fonts Bucket (resolves upstream binary on install)"
                    .into(),
            ),
            homepage: Some(format!(
                "https://github.com/matthewjberger/scoop-nerd-fonts/blob/master/bucket/{filename}"
            )),
            license: Some("Open Source (Scoop Bucket)".into()),
            tags,
            sha256: None,
            redistribution_allowed: Some(true),
            download_mode: Some("direct".into()),
        });
    }

    out
}

pub fn parse_scoop_manifest(v: &Value) -> Result<(Vec<String>, Option<String>), String> {
    let mut urls = Vec::new();
    match &v["url"] {
        Value::String(u) => urls.push(u.trim().to_string()),
        Value::Array(arr) => {
            for item in arr {
                if let Some(u) = item.as_str() {
                    urls.push(u.trim().to_string());
                }
            }
        }
        _ => {}
    }

    let valid_urls: Vec<String> = urls
        .into_iter()
        .map(|u| u.split('#').next().unwrap_or(&u).trim().to_string())
        .filter(|u| {
            u.starts_with("https://")
                && (url_has_ext(u, ".zip") || url_has_ext(u, ".ttf") || url_has_ext(u, ".otf"))
        })
        .collect();

    if valid_urls.is_empty() {
        return Err("Scoop manifest contained no HTTPS .zip/.ttf/.otf font download URL".into());
    }

    let raw_hash = match &v["hash"] {
        Value::String(h) => Some(h.trim()),
        Value::Array(arr) => arr.first().and_then(Value::as_str).map(str::trim),
        _ => None,
    };
    let sha256 = raw_hash.and_then(|h| {
        let hex = h
            .strip_prefix("sha256:")
            .or_else(|| h.strip_prefix("SHA256:"))
            .unwrap_or(h)
            .trim();
        if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(hex.to_ascii_lowercase())
        } else {
            None
        }
    });

    Ok((valid_urls, sha256))
}

pub fn parse_fontget_sources_catalog(
    v: &Value,
    source_key: &str,
    source_label: &str,
    download_mode: &str,
    redistribution_allowed: bool,
) -> Vec<GoogleFontEntry> {
    let Some(map) = v["fonts"].as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();

    for (idx, (slug, obj)) in map.iter().enumerate() {
        let name = clean_opt_string(&obj["name"])
            .or_else(|| clean_opt_string(&obj["family"]))
            .unwrap_or_else(|| format_cask_token_name(slug));
        if name.is_empty() {
            continue;
        }
        let desc = clean_opt_string(&obj["description"]);
        let license = clean_opt_string(&obj["license"]).map(|l| {
            if source_key == "fontsquirrel" && l.eq_ignore_ascii_case("other") {
                "Commercial Free (Font Squirrel)".into()
            } else {
                l
            }
        });
        let homepage =
            clean_opt_string(&obj["source_url"]).or_else(|| clean_opt_string(&obj["metadata_url"]));

        let mut designers = Vec::new();
        for key in ["foundry", "designer"] {
            if let Some(d) = clean_opt_string(&obj[key]) {
                if !designers
                    .iter()
                    .any(|x: &String| x.eq_ignore_ascii_case(&d))
                {
                    designers.push(d);
                }
            }
        }

        let cat_hint = obj["categories"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        let category = if cat_hint.is_empty() {
            infer_category_from_metadata(&name, desc.as_deref().unwrap_or(""))
        } else {
            map_generic_category(cat_hint)
        };

        let tags: Vec<String> = obj["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let scripts: Vec<String> = obj["languages"]
            .as_array()
            .map(|a| {
                let mut set = Vec::new();
                for lang in a.iter().filter_map(Value::as_str) {
                    let mapped = if lang.starts_with("Latin") {
                        "Latin"
                    } else {
                        lang.trim()
                    };
                    if !mapped.is_empty() && !set.iter().any(|x| x == mapped) {
                        set.push(mapped.to_string());
                    }
                }
                if set.is_empty() {
                    vec!["Latin".into()]
                } else {
                    set
                }
            })
            .unwrap_or_else(|| vec!["Latin".into()]);

        let mut download_urls = Vec::new();
        let mut variants = Vec::new();
        if let Some(vars) = obj["variants"].as_array() {
            for var in vars {
                let w = var["weight"].as_i64().unwrap_or(400);
                let italic = var["style"].as_str() == Some("italic");
                variants.push(if italic {
                    format!("{w}i")
                } else {
                    w.to_string()
                });
                if let Some(files) = var["files"].as_object() {
                    for ext_key in ["zip", "ttf", "otf"] {
                        if let Some(u) = files.get(ext_key).and_then(Value::as_str) {
                            let u = u.trim();
                            if u.starts_with("https://") && !download_urls.iter().any(|x| x == u) {
                                download_urls.push(u.to_string());
                            }
                        }
                    }
                }
            }
        }
        if variants.is_empty() {
            variants.push("400".into());
        }

        out.push(GoogleFontEntry {
            name,
            category,
            styles: variants.len().max(1),
            popularity: obj["popularity"]
                .as_i64()
                .map(|p| 3_000 + (1_000 - p.clamp(0, 1_000)))
                .unwrap_or(3_500 + (idx as i64)),
            date: clean_opt_string(&obj["last_modified"]).unwrap_or_default(),
            scripts,
            designers,
            is_variable: tags.iter().any(|t| t.eq_ignore_ascii_case("variable")),
            axes: Vec::new(),
            variants,
            source: source_key.to_string(),
            source_id: slug.to_string(),
            source_label: Some(source_label.to_string()),
            version_tag: clean_opt_string(&obj["version"]),
            preview_url: None,
            download_urls,
            description: desc,
            homepage,
            license,
            tags,
            sha256: None,
            redistribution_allowed: Some(redistribution_allowed),
            download_mode: Some(download_mode.to_string()),
        });
    }

    out
}

pub fn parse_open_font_repository(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(items) = v["items"].as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(idx, f)| {
            let name = clean_opt_string(&f["family"])?;
            let files = f["files"].as_object()?;
            let mut download_urls = Vec::new();
            for u in files.values().filter_map(Value::as_str) {
                let u = u.trim();
                if u.starts_with("https://") && (url_has_ext(u, ".ttf") || url_has_ext(u, ".otf")) {
                    download_urls.push(u.to_string());
                }
            }
            if download_urls.is_empty() {
                return None;
            }
            let variants: Vec<String> = f["variants"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect()
                })
                .filter(|v: &Vec<String>| !v.is_empty())
                .unwrap_or_else(|| vec!["400".into()]);
            Some(GoogleFontEntry {
                category: infer_category_from_metadata(&name, ""),
                styles: variants.len().max(1),
                popularity: 1_200 + (idx as i64),
                date: clean_opt_string(&f["lastModified"]).unwrap_or_default(),
                scripts: super::map_scripts(f),
                designers: Vec::new(),
                is_variable: false,
                axes: Vec::new(),
                variants,
                source: "open_foundries".into(),
                source_id: name.to_ascii_lowercase().replace(' ', "-"),
                source_label: Some("Open Font Repo".into()),
                version_tag: clean_opt_string(&f["version"]),
                name,
                preview_url: None,
                download_urls,
                description: Some("Curated non-Google open-source font repository".into()),
                homepage: Some("https://github.com/GustavoPeredo/open-font-repository".into()),
                license: Some("Open Source".into()),
                tags: vec!["open-font-repo".into()],
                sha256: None,
                redistribution_allowed: Some(true),
                download_mode: Some("direct".into()),
            })
        })
        .collect()
}

fn decode_html_text(raw: &str) -> String {
    let mut no_tags = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => no_tags.push(ch),
            _ => {}
        }
    }
    no_tags
        .replace("&amp;", "&")
        .replace("&#038;", "&")
        .replace("&#8211;", "-")
        .replace("&#8212;", "—")
        .replace("&#8217;", "'")
        .replace("&#8216;", "'")
        .replace("&#8220;", "\"")
        .replace("&#8221;", "\"")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("[&hellip;]", "...")
        .trim()
        .to_string()
}

pub fn parse_fontesk_wp_posts(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(idx, post)| {
            let raw_title = post["title"]["rendered"].as_str()?;
            let cleaned_title = decode_html_text(raw_title);
            let name = cleaned_title
                .strip_suffix(" Font")
                .unwrap_or(&cleaned_title)
                .trim()
                .to_string();
            let homepage = post["link"].as_str()?.trim().to_string();
            if name.is_empty() || !homepage.starts_with("https://") {
                return None;
            }
            let excerpt = post["excerpt"]["rendered"]
                .as_str()
                .map(decode_html_text)
                .filter(|s| !s.is_empty());

            let mut tags = Vec::new();
            let mut category_hints = Vec::new();
            let mut commercial_hint = false;

            if let Some(groups) = post["_embedded"]["wp:term"].as_array() {
                for group in groups {
                    if let Some(terms) = group.as_array() {
                        for term in terms {
                            let tax = term["taxonomy"].as_str().unwrap_or("");
                            let tname = term["name"]
                                .as_str()
                                .map(decode_html_text)
                                .unwrap_or_default();
                            if tname.is_empty() {
                                continue;
                            }
                            if tname.to_ascii_lowercase().contains("commercial") {
                                commercial_hint = true;
                            }
                            if tax == "category" {
                                category_hints.push(tname.clone());
                            }
                            if !tags.iter().any(|x: &String| x.eq_ignore_ascii_case(&tname)) {
                                tags.push(tname);
                            }
                        }
                    }
                }
            }

            if excerpt
                .as_deref()
                .is_some_and(|e| e.to_ascii_lowercase().contains("commercial"))
            {
                commercial_hint = true;
            }

            let category = category_hints
                .first()
                .map(|c| map_generic_category(c))
                .unwrap_or_else(|| {
                    infer_category_from_metadata(&name, excerpt.as_deref().unwrap_or(""))
                });

            let license = if commercial_hint {
                "Free for Commercial Use (Fontesk)"
            } else {
                "Free / Verify License on Fontesk"
            };

            Some(GoogleFontEntry {
                name,
                category,
                styles: 1,
                popularity: (idx as i64) + 1,
                date: clean_opt_string(&post["date"]).unwrap_or_default(),
                scripts: vec!["Latin".into()],
                designers: Vec::new(),
                is_variable: tags.iter().any(|t| t.eq_ignore_ascii_case("variable")),
                axes: Vec::new(),
                variants: vec!["400".into()],
                source: "fontesk".into(),
                source_id: post["id"]
                    .as_u64()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| homepage.clone()),
                source_label: Some("Fontesk".into()),
                version_tag: None,
                preview_url: None,
                download_urls: Vec::new(),
                description: excerpt,
                homepage: Some(homepage),
                license: Some(license.into()),
                tags,
                sha256: None,
                redistribution_allowed: Some(false),
                download_mode: Some("origin_link".into()),
            })
        })
        .collect()
}

pub async fn search_fontesk_fonts(query: &str) -> Result<Vec<GoogleFontEntry>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let url = reqwest::Url::parse_with_params(
        "https://fontesk.com/wp-json/wp/v2/posts",
        &[
            ("search", trimmed),
            ("per_page", "20"),
            ("_embed", "wp:term"),
        ],
    )
    .map_err(|e| e.to_string())?;
    let client = http_client()?;
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Fontesk search failed: {e}"))?;
    let text = resp
        .error_for_status()
        .map_err(|e| format!("Fontesk search error: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let val = parse_json(&text)?;
    Ok(parse_fontesk_wp_posts(&val))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_parser_filters_non_google_https_fonts_and_maps_metadata() {
        let valid_sha = "a".repeat(64);
        let payload = serde_json::json!([
            {
                "token": "visual-studio-code",
                "name": ["VS Code"],
                "url": "https://example.org/app.zip",
                "deprecated": false,
                "disabled": false
            },
            {
                "token": "font-inter",
                "name": ["Inter"],
                "url": "https://github.com/google/fonts/raw/main/ofl/inter/Inter.ttf",
                "deprecated": false,
                "disabled": false
            },
            {
                "token": "font-http-only",
                "name": ["HttpOnly"],
                "url": "http://example.org/HttpOnly.zip",
                "deprecated": false,
                "disabled": false
            },
            {
                "token": "font-old-deprecated",
                "name": ["Old Font"],
                "url": "https://example.org/OldFont.zip",
                "deprecated": true,
                "disabled": false
            },
            {
                "token": "font-opendyslexic",
                "name": ["OpenDyslexic"],
                "desc": "Typeface designed to mitigate reading errors caused by dyslexia",
                "homepage": "https://opendyslexic.org/",
                "version": "0.91.12",
                "sha256": valid_sha,
                "url": "https://github.com/antijingoist/opendyslexic/releases/download/v0.91.12/opendyslexic.zip",
                "deprecated": false,
                "disabled": false
            }
        ]);

        let entries = parse_homebrew_fonts_catalog(&payload);
        assert_eq!(entries.len(), 1);
        let od = &entries[0];
        assert_eq!(od.name, "OpenDyslexic");
        assert_eq!(od.source, "homebrew");
        assert_eq!(od.source_label.as_deref(), Some("Homebrew Open"));
        assert_eq!(od.sha256.as_deref(), Some(valid_sha.as_str()));
        assert_eq!(od.homepage.as_deref(), Some("https://opendyslexic.org/"));
        assert!(od.tags.contains(&"dyslexia".to_string()));
        assert_eq!(od.download_mode.as_deref(), Some("direct"));
    }

    #[test]
    fn scoop_index_and_manifest_parsers_extract_urls_and_sha256() {
        let index = serde_json::json!([
            {
                "type": "file",
                "name": "Maple-Mono-NF.json",
                "download_url": "https://raw.githubusercontent.com/matthewjberger/scoop-nerd-fonts/master/bucket/Maple-Mono-NF.json"
            },
            {
                "type": "file",
                "name": "README.md"
            }
        ]);
        let entries = parse_scoop_bucket_index(&index);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Maple Mono NF");
        assert_eq!(entries[0].source, "scoop");
        assert_eq!(entries[0].category, "mono");

        let single_manifest = serde_json::json!({
            "url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/0xProto.zip",
            "hash": "8951412356611266734e2e9904173d437b804af1ffce4fe1804feef4fe9c8c3b"
        });
        let (urls, sha) = parse_scoop_manifest(&single_manifest).unwrap();
        assert_eq!(urls.len(), 1);
        assert!(sha.is_some());

        let array_manifest = serde_json::json!({
            "url": [
                "https://github.com/example/font/releases/download/v1/A-Regular.ttf#/dl.ttf",
                "http://insecure.example.org/B.ttf"
            ],
            "hash": ["sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"]
        });
        let (urls2, sha2) = parse_scoop_manifest(&array_manifest).unwrap();
        assert_eq!(
            urls2,
            vec!["https://github.com/example/font/releases/download/v1/A-Regular.ttf"]
        );
        assert_eq!(sha2.as_deref(), Some("b".repeat(64).as_str()));
    }

    #[test]
    fn fontget_sources_and_open_font_repo_parsers_map_rich_metadata() {
        let fsq = serde_json::json!({
            "source_info": { "name": "Font Squirrel" },
            "fonts": {
                "1942-report": {
                    "name": "1942 report",
                    "license": "Other",
                    "foundry": "Johan Holmdahl",
                    "categories": ["Display"],
                    "tags": ["typewriter"],
                    "popularity": 40,
                    "source_url": "https://www.fontsquirrel.com/fonts/1942-report",
                    "languages": ["Latin", "Latin Extended"],
                    "variants": [{ "weight": 400, "style": "normal", "files": { "zip": "https://www.fontsquirrel.com/fonts/download/1942-report" } }]
                }
            }
        });
        let parsed_fsq = parse_fontget_sources_catalog(
            &fsq,
            "fontsquirrel",
            "Font Squirrel",
            "origin_link",
            false,
        );
        assert_eq!(parsed_fsq.len(), 1);
        assert_eq!(parsed_fsq[0].name, "1942 report");
        assert_eq!(parsed_fsq[0].tags, vec!["typewriter"]);
        assert_eq!(parsed_fsq[0].designers, vec!["Johan Holmdahl"]);
        assert_eq!(parsed_fsq[0].download_mode.as_deref(), Some("origin_link"));

        let ofr = serde_json::json!({
            "items": [
                {
                    "family": "Agave",
                    "variants": ["Regular", "Bold"],
                    "files": {
                        "Regular": "https://github.com/blobject/agave/releases/latest/download/Agave-Regular.ttf",
                        "Bold": "https://github.com/blobject/agave/releases/latest/download/Agave-Bold.ttf"
                    },
                    "subsets": ["latin", "greek"],
                    "lastModified": "2021-01-06"
                }
            ]
        });
        let parsed_ofr = parse_open_font_repository(&ofr);
        assert_eq!(parsed_ofr.len(), 1);
        assert_eq!(parsed_ofr[0].name, "Agave");
        assert_eq!(parsed_ofr[0].source, "open_foundries");
        assert_eq!(parsed_ofr[0].download_urls.len(), 2);
        assert_eq!(parsed_ofr[0].download_mode.as_deref(), Some("direct"));
    }

    #[test]
    fn fontesk_wp_rest_parser_extracts_title_tags_excerpt_and_origin_link() {
        let wp = serde_json::json!([
            {
                "id": 168216,
                "date": "2026-11-20T10:00:00",
                "link": "https://fontesk.com/super-nugget-font/",
                "title": { "rendered": "Super Nugget &amp; Co Font" },
                "excerpt": { "rendered": "<p>Playful groovy rounded display &#8211; free for commercial use.</p>" },
                "_embedded": {
                    "wp:term": [
                        [{ "taxonomy": "category", "name": "Display" }],
                        [{ "taxonomy": "post_tag", "name": "Groovy" }, { "taxonomy": "post_tag", "name": "Retro" }]
                    ]
                }
            }
        ]);
        let entries = parse_fontesk_wp_posts(&wp);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Super Nugget & Co");
        assert_eq!(entries[0].source, "fontesk");
        assert_eq!(
            entries[0].homepage.as_deref(),
            Some("https://fontesk.com/super-nugget-font/")
        );
        assert!(entries[0].tags.contains(&"Groovy".to_string()));
        assert_eq!(
            entries[0].license.as_deref(),
            Some("Free for Commercial Use (Fontesk)")
        );
        assert_eq!(entries[0].download_mode.as_deref(), Some("origin_link"));
    }
}
