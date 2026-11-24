//! Custom user font repositories, RSS/Atom/JSON/GitHub Releases feeds, and 13 curated presets.

use super::{
    http_client, map_generic_category, parse_json, CustomFontSource, GithubFontRepo,
    GoogleFontEntry,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::IpAddr,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// ---- Custom User Repositories & Feeds (RSS / Atom / JSON / GitHub Releases) ----

const CUSTOM_SOURCES_FILE: &str = "custom-sources.json";
const CUSTOM_CACHE_FILE: &str = "custom-sources-cache.json";

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_public_url(url: &reqwest::Url) -> bool {
    if !matches!(url.scheme(), "https" | "http") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
    {
        return false;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                !(v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_multicast())
            }
            IpAddr::V6(v6) => {
                !(v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80)
            }
        };
    }
    true
}

pub(super) fn custom_http_client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent("cosmic-font-foundry")
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if is_public_url(attempt.url()) && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}

pub(super) fn url_has_ext(raw: &str, ext: &str) -> bool {
    let base = raw.split(['?', '#']).next().unwrap_or(raw);
    base.to_ascii_lowercase().ends_with(ext)
}

fn slugify_id(name: &str, url: &str) -> String {
    let mut base: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    let clean = base.trim_matches('-');
    let stem = if clean.is_empty() { "feed" } else { clean };
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{stem}-{:06x}", h & 0xffffff)
}

fn normalize_custom_feed_url(raw_url: &str, kind: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("Feed or repository URL cannot be empty.".into());
    }
    if trimmed.starts_with("preset:") {
        return Ok(trimmed.to_string());
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|e| format!("Invalid URL: {e}"))?;
    if !is_public_url(&parsed) {
        return Err("Only public HTTP/HTTPS URLs are permitted for custom libraries.".into());
    }

    // Automatically convert `https://github.com/owner/repo` or `/releases` into the GitHub Releases JSON API.
    if parsed.host_str() == Some("github.com") && matches!(kind, "auto" | "github") {
        let segs: Vec<&str> = parsed
            .path_segments()
            .map(|s| s.filter(|p| !p.is_empty()).collect())
            .unwrap_or_default();
        if (segs.len() == 2 || (segs.len() == 3 && segs[2] == "releases"))
            && !trimmed.ends_with(".atom")
            && !trimmed.ends_with(".xml")
        {
            return Ok(format!(
                "https://api.github.com/repos/{}/{}/releases?per_page=15",
                segs[0], segs[1]
            ));
        }
    }

    Ok(parsed.to_string())
}

fn read_custom_sources(data_dir: &Path) -> Vec<CustomFontSource> {
    fs::read_to_string(data_dir.join(CUSTOM_SOURCES_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_custom_sources(data_dir: &Path, sources: &[CustomFontSource]) {
    let _ = fs::create_dir_all(data_dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(sources) {
        let _ = fs::write(data_dir.join(CUSTOM_SOURCES_FILE), bytes);
    }
}

fn read_custom_cache(data_dir: &Path) -> HashMap<String, Vec<GoogleFontEntry>> {
    fs::read_to_string(data_dir.join(CUSTOM_CACHE_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_custom_cache(data_dir: &Path, cache: &HashMap<String, Vec<GoogleFontEntry>>) {
    let _ = fs::create_dir_all(data_dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(cache) {
        let _ = fs::write(data_dir.join(CUSTOM_CACHE_FILE), bytes);
    }
}

pub(super) fn enabled_custom_entries(data_dir: &Path) -> Vec<GoogleFontEntry> {
    let sources = read_custom_sources(data_dir);
    let cache = read_custom_cache(data_dir);
    let mut out = Vec::new();
    for src in sources.into_iter().filter(|s| s.enabled) {
        if let Some(entries) = cache.get(&src.id) {
            out.extend(entries.iter().cloned());
        }
    }
    out
}

pub(super) fn find_cached_custom_entry(data_dir: &Path, family: &str) -> Option<GoogleFontEntry> {
    enabled_custom_entries(data_dir)
        .into_iter()
        .find(|e| e.name.eq_ignore_ascii_case(family))
}

pub fn list_custom_sources(data_dir: &Path) -> Vec<CustomFontSource> {
    read_custom_sources(data_dir)
}

pub fn remove_custom_source(data_dir: &Path, id: &str) -> Vec<CustomFontSource> {
    let mut sources = read_custom_sources(data_dir);
    sources.retain(|s| s.id != id);
    write_custom_sources(data_dir, &sources);
    let mut cache = read_custom_cache(data_dir);
    cache.remove(id);
    write_custom_cache(data_dir, &cache);
    sources
}

pub fn toggle_custom_source(data_dir: &Path, id: &str, enabled: bool) -> Vec<CustomFontSource> {
    let mut sources = read_custom_sources(data_dir);
    for src in &mut sources {
        if src.id == id {
            src.enabled = enabled;
        }
    }
    write_custom_sources(data_dir, &sources);
    sources
}

pub async fn add_custom_source(
    data_dir: &Path,
    name: String,
    url: String,
    kind: String,
) -> Result<Vec<CustomFontSource>, String> {
    let clean_name = name.trim().to_string();
    if clean_name.is_empty() {
        return Err("Please enter a library name.".into());
    }
    let normalized_url = normalize_custom_feed_url(&url, &kind)?;
    let id = slugify_id(&clean_name, &normalized_url);
    let effective_kind = if normalized_url.starts_with("preset:") {
        "preset".to_string()
    } else if kind.trim().is_empty() {
        "auto".to_string()
    } else {
        kind.trim().to_string()
    };

    let entries =
        fetch_and_parse_custom_source(&id, &clean_name, &normalized_url, &effective_kind).await?;

    let mut sources = read_custom_sources(data_dir);
    sources.retain(|s| s.id != id && !s.url.eq_ignore_ascii_case(&normalized_url));
    sources.push(CustomFontSource {
        id: id.clone(),
        name: clean_name,
        url: normalized_url,
        kind: effective_kind,
        enabled: true,
        last_synced: now_unix_secs(),
        font_count: entries.len(),
        last_error: None,
    });
    write_custom_sources(data_dir, &sources);

    let mut cache = read_custom_cache(data_dir);
    cache.insert(id, entries);
    write_custom_cache(data_dir, &cache);
    Ok(sources)
}

pub async fn sync_custom_source(
    data_dir: &Path,
    id: &str,
) -> Result<Vec<CustomFontSource>, String> {
    let mut sources = read_custom_sources(data_dir);
    let Some(target) = sources.iter().find(|s| s.id == id).cloned() else {
        return Err(format!("Custom source '{id}' not found."));
    };
    let fetched =
        fetch_and_parse_custom_source(&target.id, &target.name, &target.url, &target.kind).await;

    let mut cache = read_custom_cache(data_dir);
    match fetched {
        Ok(entries) => {
            for s in &mut sources {
                if s.id == id {
                    s.last_synced = now_unix_secs();
                    s.font_count = entries.len();
                    s.last_error = None;
                }
            }
            cache.insert(id.to_string(), entries);
            write_custom_cache(data_dir, &cache);
            write_custom_sources(data_dir, &sources);
            Ok(sources)
        }
        Err(err_msg) => {
            for s in &mut sources {
                if s.id == id {
                    s.last_error = Some(err_msg.clone());
                }
            }
            write_custom_sources(data_dir, &sources);
            Err(err_msg)
        }
    }
}

pub(super) async fn refresh_enabled_custom_sources(data_dir: &Path) {
    let sources = read_custom_sources(data_dir);
    for src in sources.into_iter().filter(|s| s.enabled) {
        let _ = sync_custom_source(data_dir, &src.id).await;
    }
}

async fn fetch_and_parse_custom_source(
    source_id_key: &str,
    source_name: &str,
    url: &str,
    kind: &str,
) -> Result<Vec<GoogleFontEntry>, String> {
    if let Some(preset_key) = url.strip_prefix("preset:") {
        let entries = resolve_preset_entries(preset_key, source_id_key, source_name);
        if entries.is_empty() {
            return Err(format!("Unknown preset catalog: {preset_key}"));
        }
        return Ok(entries);
    }

    let client = custom_http_client()?;
    let body = client
        .get(url)
        .header(
            "Accept",
            "application/json, application/feed+json, application/rss+xml, application/atom+xml, application/xml, text/xml;q=0.9, */*;q=0.8",
        )
        .send()
        .await
        .map_err(|e| format!("Failed to reach {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP error from {url}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed reading feed body: {e}"))?;

    let trimmed = body.trim_start();
    let src_tag = format!("custom:{source_id_key}");
    let entries = if kind == "rss"
        || trimmed.starts_with('<')
            && (trimmed.contains("<rss")
                || trimmed.contains("<feed")
                || trimmed.contains("<item")
                || trimmed.contains("<entry"))
    {
        parse_rss_or_atom_feed(trimmed, &src_tag, source_name)
    } else {
        parse_custom_json_catalog(trimmed, &src_tag, source_name)?
    };

    if entries.is_empty() {
        return Err("Feed parsed cleanly, but contained no recognizable font entries.".into());
    }
    Ok(entries)
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn strip_cdata_and_tags(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if let Some(rest) = s.strip_prefix("<![CDATA[") {
        if let Some(inner) = rest.strip_suffix("]]>") {
            s = inner.to_string();
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    decode_xml_entities(out.trim())
}

fn extract_tag_inner(block: &str, tag: &str) -> Option<String> {
    let open_exact = format!("<{tag}>");
    let open_attr = format!("<{tag} ");
    let start_idx = block.find(&open_exact).or_else(|| block.find(&open_attr))?;
    let after_open = block[start_idx..].find('>')? + start_idx + 1;
    let close_tag = format!("</{tag}>");
    let end_idx = block[after_open..].find(&close_tag)? + after_open;
    let val = strip_cdata_and_tags(&block[after_open..end_idx]);
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

fn extract_quoted_attrs(tag_text: &str, attr: &str) -> Vec<String> {
    let mut results = Vec::new();
    for quote in ['"', '\''] {
        let needle = format!("{attr}={quote}");
        let mut rest = tag_text;
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos + needle.len()..];
            if let Some(end) = after.find(quote) {
                results.push(decode_xml_entities(&after[..end]));
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
    }
    results
}

fn collect_font_urls(raw: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();

    for attr in ["url", "href", "src"] {
        for candidate in extract_quoted_attrs(raw, attr) {
            if is_font_asset_url(&candidate) && seen.insert(candidate.clone()) {
                urls.push(candidate);
            }
        }
    }

    for token in raw.split(|c: char| {
        c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']')
    }) {
        let cleaned = decode_xml_entities(token.trim_end_matches([',', '.', ';']));
        if (cleaned.starts_with("https://") || cleaned.starts_with("http://"))
            && is_font_asset_url(&cleaned)
            && seen.insert(cleaned.clone())
        {
            urls.push(cleaned);
        }
    }

    urls
}

fn is_font_asset_url(url: &str) -> bool {
    url_has_ext(url, ".ttf")
        || url_has_ext(url, ".otf")
        || url_has_ext(url, ".woff2")
        || url_has_ext(url, ".zip")
}

fn infer_category_from_text(cat_hint: Option<&str>, name: &str) -> String {
    if let Some(hint) = cat_hint {
        if !hint.trim().is_empty() {
            let mapped = map_generic_category(hint);
            if mapped != "display" || hint.to_ascii_lowercase().contains("display") {
                return mapped;
            }
        }
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("mono") || lower.contains("code") || lower.contains("terminal") {
        "mono".into()
    } else if lower.contains("serif") || lower.contains("slab") {
        "serif".into()
    } else if lower.contains("script") || lower.contains("hand") || lower.contains("brush") {
        "script".into()
    } else if lower.contains("sans") || lower.contains("grotesk") || lower.contains("gothic") {
        "sans".into()
    } else {
        "display".into()
    }
}

fn clean_release_title(raw_title: &str, fallback_source_name: &str) -> String {
    let trimmed = raw_title.trim();
    if trimmed.is_empty() {
        return fallback_source_name.to_string();
    }
    let looks_like_tag = (trimmed.starts_with('v')
        && trimmed[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit()))
        || trimmed.chars().all(|c| c.is_ascii_digit() || c == '.');
    if looks_like_tag {
        format!("{fallback_source_name} ({trimmed})")
    } else {
        trimmed.to_string()
    }
}

fn extract_version_tag(raw: &str) -> Option<String> {
    for tok in raw.split_whitespace() {
        let cleaned = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        let digits = cleaned
            .strip_prefix('v')
            .or_else(|| cleaned.strip_prefix('V'))
            .unwrap_or(cleaned);
        if digits.contains('.') && digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(cleaned.to_string());
        }
    }
    None
}

pub fn parse_rss_or_atom_feed(xml: &str, src_tag: &str, source_name: &str) -> Vec<GoogleFontEntry> {
    let use_entry = !xml.contains("<item") && xml.contains("<entry");
    let (open_a, open_b, close) = if use_entry {
        ("<entry>", "<entry ", "</entry>")
    } else {
        ("<item>", "<item ", "</item>")
    };

    let mut entries = Vec::new();
    let mut seen_names = HashSet::new();
    let mut cursor = xml;
    let mut idx: i64 = 1;

    while let Some(start) = cursor
        .find(open_a)
        .into_iter()
        .chain(cursor.find(open_b))
        .min()
    {
        let body_start = start + 1;
        let Some(rel_end) = cursor[body_start..].find(close) else {
            break;
        };
        let block = &cursor[start..body_start + rel_end + close.len()];
        cursor = &cursor[body_start + rel_end + close.len()..];

        let raw_title = extract_tag_inner(block, "title").unwrap_or_default();
        let name = clean_release_title(&raw_title, source_name);
        if name.is_empty() || !seen_names.insert(name.to_lowercase()) {
            continue;
        }

        let category_hint = extract_tag_inner(block, "category");
        let category = infer_category_from_text(category_hint.as_deref(), &name);
        let author = extract_tag_inner(block, "dc:creator")
            .or_else(|| extract_tag_inner(block, "name"))
            .or_else(|| extract_tag_inner(block, "author"));
        let date = extract_tag_inner(block, "pubDate")
            .or_else(|| extract_tag_inner(block, "published"))
            .or_else(|| extract_tag_inner(block, "updated"))
            .unwrap_or_default();

        let all_urls = collect_font_urls(block);
        let preview_url = all_urls.iter().find(|u| url_has_ext(u, ".woff2")).cloned();
        let mut download_urls: Vec<String> = all_urls
            .iter()
            .filter(|u| url_has_ext(u, ".ttf") || url_has_ext(u, ".otf") || url_has_ext(u, ".zip"))
            .cloned()
            .collect();
        if download_urls.is_empty() {
            if let Some(fallback_link) = extract_tag_inner(block, "link") {
                if fallback_link.starts_with("http") {
                    download_urls.push(fallback_link);
                }
            }
        }

        entries.push(GoogleFontEntry {
            name: name.clone(),
            category,
            styles: download_urls.len().max(1),
            popularity: idx,
            date,
            scripts: vec!["Latin".into()],
            designers: author.into_iter().collect(),
            is_variable: name.to_ascii_lowercase().contains("variable"),
            axes: Vec::new(),
            variants: vec!["400".into()],
            source: src_tag.to_string(),
            source_id: name,
            source_label: Some(source_name.to_string()),
            version_tag: extract_version_tag(&raw_title),
            preview_url,
            download_urls,
        });
        idx += 1;
    }

    entries
}

pub fn parse_custom_json_catalog(
    raw: &str,
    src_tag: &str,
    source_name: &str,
) -> Result<Vec<GoogleFontEntry>, String> {
    let val = parse_json(raw).map_err(|e| format!("Invalid JSON feed: {e}"))?;
    let items: &[Value] = if let Some(arr) = val.as_array() {
        arr
    } else if let Some(arr) = val["fonts"].as_array() {
        arr
    } else if let Some(arr) = val["families"].as_array() {
        arr
    } else if let Some(arr) = val["items"].as_array() {
        arr
    } else {
        return Err("JSON feed must be an array or contain a 'fonts'/'items' array.".into());
    };

    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    for (idx, item) in items.iter().enumerate() {
        // GitHub Releases format: object has `assets` array + `tag_name`.
        if let Some(assets) = item["assets"].as_array() {
            let asset_urls: Vec<String> = assets
                .iter()
                .filter_map(|a| a["browser_download_url"].as_str())
                .filter(|u| is_font_asset_url(u))
                .map(ToString::to_string)
                .collect();
            if asset_urls.is_empty() {
                continue;
            }
            let raw_name = item["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| item["tag_name"].as_str())
                .unwrap_or(source_name);
            let name = clean_release_title(raw_name, source_name);
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            let preview_url = asset_urls
                .iter()
                .find(|u| url_has_ext(u, ".woff2"))
                .cloned();
            let download_urls: Vec<String> = asset_urls
                .into_iter()
                .filter(|u| {
                    url_has_ext(u, ".ttf") || url_has_ext(u, ".otf") || url_has_ext(u, ".zip")
                })
                .collect();
            let author = item["author"]["login"].as_str().map(ToString::to_string);
            entries.push(GoogleFontEntry {
                category: infer_category_from_text(None, &name),
                styles: download_urls.len().max(1),
                popularity: (idx as i64) + 1,
                date: item["published_at"].as_str().unwrap_or("").to_string(),
                scripts: vec!["Latin".into()],
                designers: author.into_iter().collect(),
                is_variable: name.to_ascii_lowercase().contains("variable"),
                axes: Vec::new(),
                variants: vec!["400".into()],
                source: src_tag.to_string(),
                source_id: name.clone(),
                source_label: Some(source_name.to_string()),
                version_tag: item["tag_name"]
                    .as_str()
                    .or_else(|| item["version"].as_str())
                    .map(ToString::to_string),
                name,
                preview_url,
                download_urls,
            });
            continue;
        }

        // JSON Feed 1.1 or Generic Font JSON format
        let raw_name = item["name"]
            .as_str()
            .or_else(|| item["family"].as_str())
            .or_else(|| item["title"].as_str())
            .unwrap_or("");
        let name = raw_name.trim().to_string();
        if name.is_empty() || !seen.insert(name.to_lowercase()) {
            continue;
        }

        let mut all_urls = Vec::new();
        for key in ["download_urls", "files"] {
            if let Some(arr) = item[key].as_array() {
                for u in arr.iter().filter_map(Value::as_str) {
                    all_urls.push(u.to_string());
                }
            }
        }
        if let Some(arr) = item["attachments"].as_array() {
            for att in arr {
                if let Some(u) = att["url"].as_str() {
                    all_urls.push(u.to_string());
                }
            }
        }
        for key in ["download_url", "url", "ttf_url", "zip_url"] {
            if let Some(u) = item[key].as_str() {
                all_urls.push(u.to_string());
            }
        }

        let preview_url = item["preview_url"]
            .as_str()
            .or_else(|| item["woff2_url"].as_str())
            .map(ToString::to_string)
            .or_else(|| all_urls.iter().find(|u| url_has_ext(u, ".woff2")).cloned());
        let download_urls: Vec<String> = all_urls
            .into_iter()
            .filter(|u| url_has_ext(u, ".ttf") || url_has_ext(u, ".otf") || url_has_ext(u, ".zip"))
            .collect();

        let designers: Vec<String> = item["designers"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .or_else(|| {
                item["authors"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|x| x["name"].as_str())
                        .map(ToString::to_string)
                        .collect()
                })
            })
            .or_else(|| item["author"].as_str().map(|s| vec![s.to_string()]))
            .unwrap_or_default();

        let cat_hint = item["category"].as_str().or_else(|| {
            item["tags"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
        });

        entries.push(GoogleFontEntry {
            category: infer_category_from_text(cat_hint, &name),
            styles: item["styles"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or_else(|| download_urls.len().max(1)),
            popularity: (idx as i64) + 1,
            date: item["date"]
                .as_str()
                .or_else(|| item["date_published"].as_str())
                .unwrap_or("")
                .to_string(),
            scripts: vec!["Latin".into()],
            designers,
            is_variable: item["is_variable"].as_bool().unwrap_or(false),
            axes: Vec::new(),
            variants: vec!["400".into()],
            source: src_tag.to_string(),
            source_id: name.clone(),
            source_label: Some(source_name.to_string()),
            version_tag: item["version"]
                .as_str()
                .or_else(|| item["tag_name"].as_str())
                .map(ToString::to_string),
            name,
            preview_url,
            download_urls,
        });
    }

    Ok(entries)
}

fn truncate_description(raw: &str, max_chars: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let take_len = max_chars.saturating_sub(3);
    let prefix: String = trimmed.chars().take(take_len).collect();
    format!("{prefix}...")
}

pub fn parse_github_search_repos(v: &Value) -> Vec<GithubFontRepo> {
    let Some(items) = v["items"].as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let full_name = item["full_name"].as_str()?.trim().to_string();
            let html_url = item["html_url"].as_str()?.trim().to_string();
            if full_name.is_empty() || html_url.is_empty() {
                return None;
            }
            let name = item["name"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&full_name)
                .to_string();
            let description = item["description"]
                .as_str()
                .map(|s| truncate_description(s, 180))
                .unwrap_or_default();
            let stars = item["stargazers_count"].as_u64().unwrap_or(0);
            let license = item["license"]["spdx_id"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown")
                .to_string();
            let releases_url = item["releases_url"]
                .as_str()
                .map(|s| s.replace("{/id}", ""))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("https://api.github.com/repos/{full_name}/releases"));
            let updated_at = item["updated_at"].as_str().unwrap_or("").to_string();
            Some(GithubFontRepo {
                full_name,
                name,
                description,
                stars,
                license,
                html_url,
                releases_url,
                updated_at,
            })
        })
        .collect()
}

pub async fn search_github_fonts(query: &str) -> Result<Vec<GithubFontRepo>, String> {
    let trimmed = query.trim();
    let q = if trimmed.is_empty() {
        "topic:font license:ofl-1.1".to_string()
    } else {
        format!("{trimmed} font in:name,description,topics")
    };
    let url = reqwest::Url::parse_with_params(
        "https://api.github.com/search/repositories",
        &[("q", q.as_str()), ("sort", "stars"), ("per_page", "20")],
    )
    .map_err(|e| e.to_string())?;
    let client = http_client()?;
    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub search request failed: {e}"))?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(
            "GitHub API rate limit reached (unauthenticated limit: 10 searches/min). Wait a moment and try again."
                .into(),
        );
    }
    let text = resp
        .error_for_status()
        .map_err(|e| format!("GitHub search error: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let val = parse_json(&text)?;
    Ok(parse_github_search_repos(&val))
}

struct PresetSpec<'a> {
    name: &'a str,
    category: &'a str,
    designer: &'a str,
    styles: usize,
    is_variable: bool,
    slug: &'a str,
    scripts: &'a [&'a str],
    preview_url: Option<String>,
    download_url: String,
}

impl<'a> PresetSpec<'a> {
    fn fontsource(
        name: &'a str,
        category: &'a str,
        designer: &'a str,
        styles: usize,
        is_variable: bool,
        slug: &'a str,
    ) -> Self {
        Self {
            name,
            category,
            designer,
            styles,
            is_variable,
            slug,
            scripts: &["Latin"],
            preview_url: Some(format!(
                "https://cdn.jsdelivr.net/fontsource/fonts/{slug}@latest/latin-400-normal.woff2"
            )),
            download_url: format!(
                "https://cdn.jsdelivr.net/fontsource/fonts/{slug}@latest/latin-400-normal.ttf"
            ),
        }
    }

    fn with_variant(mut self, subset: &str, weight: u16, scripts: &'a [&'a str]) -> Self {
        self.scripts = scripts;
        self.preview_url = Some(format!(
            "https://cdn.jsdelivr.net/fontsource/fonts/{}@latest/{subset}-{weight}-normal.woff2",
            self.slug
        ));
        self.download_url = format!(
            "https://cdn.jsdelivr.net/fontsource/fonts/{}@latest/{subset}-{weight}-normal.ttf",
            self.slug
        );
        self
    }

    fn nerdfont(name: &'a str, asset_stem: &str, preview_slug: Option<&str>) -> Self {
        Self {
            name,
            category: "mono",
            designer: "Nerd Fonts",
            styles: 1,
            is_variable: false,
            slug: "",
            scripts: &["Latin", "Symbols"],
            preview_url: preview_slug.map(|slug| {
                format!("https://cdn.jsdelivr.net/fontsource/fonts/{slug}@latest/latin-400-normal.woff2")
            }),
            download_url: format!(
                "https://github.com/ryanoasis/nerd-fonts/releases/latest/download/{asset_stem}.zip"
            ),
        }
    }
}

fn resolve_preset_entries(
    preset_key: &str,
    source_id_key: &str,
    source_name: &str,
) -> Vec<GoogleFontEntry> {
    let src_tag = format!("custom:{source_id_key}");
    let items: Vec<PresetSpec<'_>> = match preset_key {
        "velvetyne" => vec![
            PresetSpec::fontsource("Syne", "display", "Bonjour Monde", 5, true, "syne")
                .with_variant("latin", 700, &["Latin"]),
            PresetSpec::fontsource(
                "Silkscreen",
                "display",
                "Jason Kottke / Velvetyne",
                2,
                false,
                "silkscreen",
            ),
            PresetSpec::fontsource("Unbounded", "display", "Studio Koto", 8, true, "unbounded")
                .with_variant("latin", 600, &["Latin"]),
            PresetSpec::fontsource(
                "Bricolage Grotesque",
                "sans",
                "Mathieu Triay",
                8,
                true,
                "bricolage-grotesque",
            )
            .with_variant("latin", 500, &["Latin"]),
        ],
        "leagueof" => vec![
            PresetSpec::fontsource(
                "League Spartan",
                "sans",
                "Matt Bailey & Tyler Finck",
                9,
                true,
                "league-spartan",
            )
            .with_variant("latin", 600, &["Latin"]),
            PresetSpec::fontsource("Orbitron", "display", "Matt McInerney", 6, true, "orbitron")
                .with_variant("latin", 600, &["Latin"]),
            PresetSpec::fontsource(
                "League Gothic",
                "display",
                "Micah Rich & Caroline Hadilaksono",
                2,
                true,
                "league-gothic",
            ),
            PresetSpec::fontsource("League Mono", "mono", "Tyler Finck", 8, true, "league-mono"),
        ],
        "indie_mono" => vec![
            PresetSpec::fontsource("Fira Code", "mono", "Nikita Prokopov", 5, true, "fira-code"),
            PresetSpec::fontsource(
                "JetBrains Mono",
                "mono",
                "JetBrains",
                16,
                true,
                "jetbrains-mono",
            ),
            PresetSpec::fontsource("Victor Mono", "mono", "Rune B", 14, false, "victor-mono"),
            PresetSpec::fontsource(
                "Space Mono",
                "mono",
                "Colophon Foundry",
                4,
                false,
                "space-mono",
            ),
            PresetSpec::fontsource(
                "Commit Mono",
                "mono",
                "Eigil Nikolajsen",
                4,
                false,
                "commit-mono",
            ),
            PresetSpec::fontsource(
                "Monaspace Neon",
                "mono",
                "GitHub Next",
                8,
                true,
                "monaspace-neon",
            ),
            PresetSpec::fontsource("Geist Mono", "mono", "Vercel", 9, true, "geist-mono"),
            PresetSpec::fontsource("Iosevka", "mono", "Belleve Invis", 18, false, "iosevka"),
        ],
        "nerd_fonts" => vec![
            PresetSpec::nerdfont(
                "JetBrainsMono Nerd Font",
                "JetBrainsMono",
                Some("jetbrains-mono"),
            ),
            PresetSpec::nerdfont("FiraCode Nerd Font", "FiraCode", Some("fira-code")),
            PresetSpec::nerdfont("Hack Nerd Font", "Hack", Some("hack")),
            PresetSpec::nerdfont(
                "CascadiaCode Nerd Font",
                "CascadiaCode",
                Some("cascadia-code"),
            ),
            PresetSpec::nerdfont("Meslo Nerd Font", "Meslo", None),
            PresetSpec::nerdfont("Monaspace Nerd Font", "Monaspace", Some("monaspace-neon")),
        ],
        "historical" => vec![
            PresetSpec::fontsource(
                "EB Garamond",
                "serif",
                "Georg Duffner",
                10,
                true,
                "eb-garamond",
            ),
            PresetSpec::fontsource(
                "Cormorant Garamond",
                "serif",
                "Christian Thalmann",
                10,
                false,
                "cormorant-garamond",
            ),
            PresetSpec::fontsource(
                "Libre Baskerville",
                "serif",
                "Impallari Type",
                3,
                false,
                "libre-baskerville",
            ),
            PresetSpec::fontsource(
                "IM Fell English",
                "serif",
                "Igino Marini",
                2,
                false,
                "im-fell-english",
            ),
            PresetSpec::fontsource("Fraunces", "serif", "Undercase Type", 14, true, "fraunces"),
            PresetSpec::fontsource(
                "Spectral",
                "serif",
                "Production Type",
                14,
                false,
                "spectral",
            ),
            PresetSpec::fontsource(
                "Bodoni Moda",
                "serif",
                "Indestructible Type",
                12,
                true,
                "bodoni-moda",
            ),
        ],
        "retro_pixel" => vec![
            PresetSpec::fontsource(
                "Silkscreen",
                "display",
                "Jason Kottke",
                2,
                false,
                "silkscreen",
            ),
            PresetSpec::fontsource(
                "Press Start 2P",
                "display",
                "CodeMan38",
                1,
                false,
                "press-start-2p",
            ),
            PresetSpec::fontsource("VT323", "mono", "Peter Hull", 1, false, "vt323"),
            PresetSpec::fontsource(
                "Pixelify Sans",
                "display",
                "Stefie Justprince",
                4,
                true,
                "pixelify-sans",
            ),
            PresetSpec::fontsource(
                "Jersey 10",
                "display",
                "Sarah Cadigan-Fried",
                1,
                false,
                "jersey-10",
            ),
            PresetSpec::fontsource("Tiny5", "display", "Gisselle Trejos", 1, false, "tiny5"),
        ],
        "global_scripts" => vec![
            PresetSpec::fontsource(
                "Vazirmatn",
                "sans",
                "Saber Rastikerdar",
                9,
                true,
                "vazirmatn",
            )
            .with_variant("arabic", 400, &["Arabic", "Latin"]),
            PresetSpec::fontsource("Cairo", "sans", "Mohamed Gaber", 8, true, "cairo")
                .with_variant("arabic", 400, &["Arabic", "Latin"]),
            PresetSpec::fontsource("Rubik", "sans", "Hubert & Fischer", 14, true, "rubik")
                .with_variant("latin", 400, &["Latin", "Cyrillic", "Hebrew", "Arabic"]),
            PresetSpec::fontsource(
                "Noto Sans JP",
                "other",
                "Google / Monotype",
                9,
                true,
                "noto-sans-jp",
            )
            .with_variant("japanese", 400, &["Japanese", "Latin"]),
            PresetSpec::fontsource(
                "Noto Sans Arabic",
                "other",
                "Google / Monotype",
                9,
                true,
                "noto-sans-arabic",
            )
            .with_variant("arabic", 400, &["Arabic"]),
            PresetSpec::fontsource(
                "Shippori Mincho",
                "serif",
                "Fontdasu",
                5,
                false,
                "shippori-mincho",
            )
            .with_variant("japanese", 400, &["Japanese", "Latin"]),
        ],
        "sil_scripts" => vec![
            PresetSpec::fontsource(
                "Gentium Plus",
                "serif",
                "SIL International",
                4,
                false,
                "gentium-plus",
            )
            .with_variant("latin", 400, &["Latin", "Greek", "Cyrillic"]),
            PresetSpec::fontsource(
                "Charis SIL",
                "serif",
                "SIL International",
                4,
                false,
                "charis-sil",
            )
            .with_variant("latin", 400, &["Latin", "Cyrillic"]),
            PresetSpec::fontsource("Andika", "sans", "SIL International", 4, false, "andika")
                .with_variant("latin", 400, &["Latin", "Cyrillic"]),
            PresetSpec::fontsource(
                "Scheherazade New",
                "other",
                "SIL International",
                4,
                false,
                "scheherazade-new",
            )
            .with_variant("arabic", 400, &["Arabic", "Latin"]),
            PresetSpec::fontsource(
                "Abyssinica SIL",
                "other",
                "SIL International",
                1,
                false,
                "abyssinica-sil",
            )
            .with_variant("latin", 400, &["Latin"]),
            PresetSpec::fontsource("Padauk", "other", "SIL International", 4, false, "padauk")
                .with_variant("latin", 400, &["Latin"]),
        ],
        "collletttivo" => vec![
            PresetSpec::fontsource(
                "Instrument Serif",
                "serif",
                "Rodrigo Fuenzalida",
                2,
                false,
                "instrument-serif",
            ),
            PresetSpec::fontsource(
                "Climate Crisis",
                "display",
                "Daniel Coull",
                1,
                true,
                "climate-crisis",
            ),
            PresetSpec::fontsource(
                "Plus Jakarta Sans",
                "sans",
                "Tokotype",
                14,
                true,
                "plus-jakarta-sans",
            ),
            PresetSpec::fontsource(
                "Rubik Glitch",
                "display",
                "Meir Sadan",
                1,
                false,
                "rubik-glitch",
            ),
            PresetSpec::fontsource(
                "Archivo Black",
                "display",
                "Omnibus-Type",
                1,
                false,
                "archivo-black",
            ),
        ],
        "use_modify" => vec![
            PresetSpec::fontsource("Work Sans", "sans", "Wei Huang", 18, true, "work-sans"),
            PresetSpec::fontsource("Epilogue", "sans", "Tyler Finck", 18, true, "epilogue"),
            PresetSpec::fontsource(
                "Space Grotesk",
                "sans",
                "Florian Karsten",
                5,
                true,
                "space-grotesk",
            ),
            PresetSpec::fontsource("Fraunces", "serif", "Undercase Type", 14, true, "fraunces"),
            PresetSpec::fontsource(
                "Commissioner",
                "sans",
                "Kostas Bartsokas",
                18,
                true,
                "commissioner",
            ),
        ],
        "oldschool_pc" => vec![
            PresetSpec::fontsource(
                "IBM Plex Mono",
                "mono",
                "Mike Abbink / Bold Monday",
                14,
                false,
                "ibm-plex-mono",
            ),
            PresetSpec::fontsource("VT323", "mono", "Peter Hull", 1, false, "vt323"),
            PresetSpec::fontsource(
                "Press Start 2P",
                "display",
                "CodeMan38",
                1,
                false,
                "press-start-2p",
            ),
            PresetSpec::fontsource(
                "Silkscreen",
                "display",
                "Jason Kottke",
                2,
                false,
                "silkscreen",
            ),
            PresetSpec::fontsource(
                "Share Tech Mono",
                "mono",
                "Ralph du Carrois",
                1,
                false,
                "share-tech-mono",
            ),
            PresetSpec::fontsource("Cousine", "mono", "Steve Matteson", 4, false, "cousine"),
        ],
        "indestructible" => vec![
            PresetSpec::fontsource(
                "Jost",
                "sans",
                "Owen Earl (indestructible type*)",
                9,
                true,
                "jost",
            ),
            PresetSpec::fontsource(
                "Besley",
                "serif",
                "Owen Earl (indestructible type*)",
                12,
                true,
                "besley",
            ),
            PresetSpec::fontsource(
                "Bodoni Moda",
                "serif",
                "Owen Earl (indestructible type*)",
                12,
                true,
                "bodoni-moda",
            ),
            PresetSpec::fontsource(
                "Libre Bodoni",
                "serif",
                "Pablo Impallari",
                4,
                true,
                "libre-bodoni",
            ),
            PresetSpec::fontsource(
                "Newsreader",
                "serif",
                "Production Type",
                14,
                true,
                "newsreader",
            ),
        ],
        "open_foundry" => vec![
            PresetSpec::fontsource("Inter", "sans", "Rasmus Andersson", 18, true, "inter"),
            PresetSpec::fontsource(
                "Instrument Sans",
                "sans",
                "Rodrigo Fuenzalida",
                8,
                true,
                "instrument-sans",
            ),
            PresetSpec::fontsource("Archivo", "sans", "Omnibus-Type", 18, true, "archivo"),
            PresetSpec::fontsource("DM Sans", "sans", "Colophon Foundry", 12, true, "dm-sans"),
            PresetSpec::fontsource("Manrope", "sans", "Mikhail Sharanda", 7, true, "manrope"),
        ],
        "horror_display" => vec![
            PresetSpec::fontsource("Creepster", "display", "Sideshow", 1, false, "creepster"),
            PresetSpec::fontsource("Nosifer", "display", "Typomondo", 1, false, "nosifer"),
            PresetSpec::fontsource("Eater", "display", "Typomondo", 1, false, "eater"),
            PresetSpec::fontsource("Butcherman", "display", "Typomondo", 1, false, "butcherman"),
            PresetSpec::fontsource(
                "Metal Mania",
                "display",
                "Open Window",
                1,
                false,
                "metal-mania",
            ),
            PresetSpec::fontsource(
                "UnifrakturMaguntia",
                "display",
                "j. 'mach' wust",
                1,
                false,
                "unifrakturmaguntia",
            ),
            PresetSpec::fontsource(
                "Pirata One",
                "display",
                "Rodrigo Fuenzalida",
                1,
                false,
                "pirata-one",
            ),
            PresetSpec::fontsource(
                "Cinzel Decorative",
                "display",
                "Natanael Gama",
                3,
                false,
                "cinzel-decorative",
            ),
            PresetSpec::fontsource("Rye", "display", "Nicole Fally", 1, false, "rye"),
            PresetSpec::fontsource(
                "Special Elite",
                "display",
                "Astigmatic",
                1,
                false,
                "special-elite",
            ),
        ],
        "handwriting_brush" => vec![
            PresetSpec::fontsource("Caveat", "script", "Pablo Impallari", 4, true, "caveat"),
            PresetSpec::fontsource("Pacifico", "script", "Vernon Adams", 1, false, "pacifico"),
            PresetSpec::fontsource(
                "Permanent Marker",
                "script",
                "Font Diner",
                1,
                false,
                "permanent-marker",
            ),
            PresetSpec::fontsource("Rock Salt", "script", "Sideshow", 1, false, "rock-salt"),
            PresetSpec::fontsource("Satisfy", "script", "Sideshow", 1, false, "satisfy"),
            PresetSpec::fontsource(
                "Dancing Script",
                "script",
                "Impallari Type",
                4,
                true,
                "dancing-script",
            ),
            PresetSpec::fontsource(
                "Kaushan Script",
                "script",
                "Impallari Type",
                1,
                false,
                "kaushan-script",
            ),
            PresetSpec::fontsource("Sacramento", "script", "Astigmatic", 1, false, "sacramento"),
        ],
        "geometric_ui" => vec![
            PresetSpec::fontsource(
                "Plus Jakarta Sans",
                "sans",
                "Tokotype",
                14,
                true,
                "plus-jakarta-sans",
            ),
            PresetSpec::fontsource("Outfit", "sans", "Rodrigo Fuenzalida", 9, true, "outfit"),
            PresetSpec::fontsource("Sora", "sans", "Jonathan Barnbrook", 8, true, "sora"),
            PresetSpec::fontsource("Urbanist", "sans", "Corey Hu", 18, true, "urbanist"),
            PresetSpec::fontsource("Figtree", "sans", "Erik Kennedy", 14, true, "figtree"),
            PresetSpec::fontsource("Onest", "sans", "Dmitry Voloshin", 9, true, "onest"),
            PresetSpec::fontsource(
                "Albert Sans",
                "sans",
                "Andreas Rasmussen",
                18,
                true,
                "albert-sans",
            ),
            PresetSpec::fontsource("Geist", "sans", "Vercel", 9, true, "geist"),
        ],
        "slab_editorial" => vec![
            PresetSpec::fontsource("Bitter", "serif", "Sol Matas", 18, true, "bitter"),
            PresetSpec::fontsource("Arvo", "serif", "Anton Koovit", 4, false, "arvo"),
            PresetSpec::fontsource("Zilla Slab", "serif", "Typotheque", 10, false, "zilla-slab"),
            PresetSpec::fontsource(
                "Josefin Slab",
                "serif",
                "Santiago Orozco",
                14,
                true,
                "josefin-slab",
            ),
            PresetSpec::fontsource(
                "Playfair Display",
                "serif",
                "Claus Eggers Sørensen",
                12,
                true,
                "playfair-display",
            ),
            PresetSpec::fontsource(
                "Abril Fatface",
                "display",
                "TypeTogether",
                1,
                false,
                "abril-fatface",
            ),
            PresetSpec::fontsource(
                "Young Serif",
                "serif",
                "Bastien Sozeau",
                1,
                false,
                "young-serif",
            ),
            PresetSpec::fontsource("Prata", "serif", "Cyreal", 1, false, "prata"),
        ],
        "accessible_reading" => vec![
            PresetSpec::fontsource(
                "Atkinson Hyperlegible",
                "sans",
                "Braille Institute",
                4,
                false,
                "atkinson-hyperlegible",
            ),
            PresetSpec::fontsource("Lexend", "sans", "Bonnie Shaver-Troup", 9, true, "lexend"),
            PresetSpec::fontsource(
                "Inclusive Sans",
                "sans",
                "Olivia King",
                2,
                false,
                "inclusive-sans",
            ),
            PresetSpec::fontsource("Andika", "sans", "SIL International", 4, false, "andika"),
            PresetSpec::fontsource("Readex Pro", "sans", "Thomas Jockin", 6, true, "readex-pro"),
            PresetSpec::fontsource("Public Sans", "sans", "USWDS", 18, true, "public-sans"),
        ],
        "display_poster" => vec![
            PresetSpec::fontsource(
                "Bebas Neue",
                "display",
                "Ryoichi Tsunekawa",
                1,
                false,
                "bebas-neue",
            ),
            PresetSpec::fontsource("Oswald", "display", "Vernon Adams", 6, true, "oswald"),
            PresetSpec::fontsource("Anton", "display", "Vernon Adams", 1, false, "anton"),
            PresetSpec::fontsource("Righteous", "display", "Astigmatic", 1, false, "righteous"),
            PresetSpec::fontsource("Monoton", "display", "Vernon Adams", 1, false, "monoton"),
            PresetSpec::fontsource(
                "Bungee",
                "display",
                "David Jonathan Ross",
                1,
                false,
                "bungee",
            ),
            PresetSpec::fontsource(
                "Rubik Mono One",
                "display",
                "Philipp Hubert",
                1,
                false,
                "rubik-mono-one",
            ),
            PresetSpec::fontsource("Cinzel", "serif", "Natanael Gama", 6, true, "cinzel"),
        ],
        _ => Vec::new(),
    };

    items
        .into_iter()
        .enumerate()
        .map(|(idx, item)| GoogleFontEntry {
            name: item.name.to_string(),
            category: item.category.to_string(),
            styles: item.styles,
            popularity: (idx as i64) + 1,
            date: "2026-01-01".into(),
            scripts: item.scripts.iter().map(|s| (*s).to_string()).collect(),
            designers: vec![item.designer.to_string()],
            is_variable: item.is_variable,
            axes: Vec::new(),
            variants: vec!["400".into()],
            source: src_tag.clone(),
            source_id: item.name.to_string(),
            source_label: Some(source_name.to_string()),
            version_tag: None,
            preview_url: item.preview_url,
            download_urls: vec![item.download_url],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_atom_and_json_parsers_extract_fonts() {
        let rss = r#"
        <rss version="2.0"><channel><title>Indie Type RSS</title>
          <item>
            <title>Cosmic Mono</title>
            <category>Monospace</category>
            <dc:creator>Ada Lovelace</dc:creator>
            <enclosure url="https://fonts.example.org/CosmicMono-Regular.ttf" type="font/ttf" />
            <description>Preview at https://fonts.example.org/CosmicMono.woff2</description>
          </item>
          <item>
            <title>Nebula Serif</title>
            <link>https://fonts.example.org/nebula-serif.zip</link>
          </item>
        </channel></rss>"#;

        let parsed_rss = parse_rss_or_atom_feed(rss, "custom:indie-123", "Indie Type RSS");
        assert_eq!(parsed_rss.len(), 2);
        assert_eq!(parsed_rss[0].name, "Cosmic Mono");
        assert_eq!(parsed_rss[0].category, "mono");
        assert_eq!(parsed_rss[0].source, "custom:indie-123");
        assert_eq!(
            parsed_rss[0].source_label.as_deref(),
            Some("Indie Type RSS")
        );
        assert_eq!(
            parsed_rss[0].preview_url.as_deref(),
            Some("https://fonts.example.org/CosmicMono.woff2")
        );
        assert_eq!(
            parsed_rss[0].download_urls,
            vec!["https://fonts.example.org/CosmicMono-Regular.ttf"]
        );
        assert_eq!(parsed_rss[1].category, "serif");
        assert_eq!(
            parsed_rss[1].download_urls,
            vec!["https://fonts.example.org/nebula-serif.zip"]
        );

        let gh_json = r#"[{
            "tag_name": "v6.2",
            "name": "Fira Code 6.2",
            "published_at": "2024-01-01T00:00:00Z",
            "author": { "login": "tonsky" },
            "assets": [{ "browser_download_url": "https://github.com/tonsky/FiraCode/releases/download/6.2/Fira_Code_v6.2.zip" }]
        }]"#;
        let parsed_json =
            parse_custom_json_catalog(gh_json, "custom:gh-456", "Fira Code GitHub").unwrap();
        assert_eq!(parsed_json.len(), 1);
        assert_eq!(parsed_json[0].name, "Fira Code 6.2");
        assert_eq!(parsed_json[0].version_tag.as_deref(), Some("v6.2"));
        assert_eq!(parsed_json[0].category, "mono");
        assert_eq!(parsed_json[0].designers, vec!["tonsky"]);
        assert_eq!(
            parsed_json[0].download_urls,
            vec!["https://github.com/tonsky/FiraCode/releases/download/6.2/Fira_Code_v6.2.zip"]
        );
    }

    #[test]
    fn custom_url_blocks_loopback_ssrf_and_rewrites_github_repo() {
        assert!(normalize_custom_feed_url("http://127.0.0.1:8080/feed.xml", "rss").is_err());
        assert!(normalize_custom_feed_url("https://localhost/feed.json", "json").is_err());
        assert_eq!(
            normalize_custom_feed_url("https://github.com/tonsky/FiraCode", "auto").unwrap(),
            "https://api.github.com/repos/tonsky/FiraCode/releases?per_page=15"
        );
    }

    #[test]
    fn all_nineteen_presets_resolve_non_empty() {
        for key in [
            "velvetyne",
            "leagueof",
            "indie_mono",
            "nerd_fonts",
            "historical",
            "retro_pixel",
            "global_scripts",
            "sil_scripts",
            "collletttivo",
            "use_modify",
            "oldschool_pc",
            "indestructible",
            "open_foundry",
            "horror_display",
            "handwriting_brush",
            "geometric_ui",
            "slab_editorial",
            "accessible_reading",
            "display_poster",
        ] {
            let entries = resolve_preset_entries(key, "test", "Test Preset");
            assert!(!entries.is_empty(), "preset {key} should not be empty");
        }
        assert_eq!(
            resolve_preset_entries("horror_display", "id", "Horror").len(),
            10
        );
        assert_eq!(
            resolve_preset_entries("handwriting_brush", "id", "Script").len(),
            8
        );
        assert_eq!(
            resolve_preset_entries("geometric_ui", "id", "Geom").len(),
            8
        );
        assert_eq!(
            resolve_preset_entries("slab_editorial", "id", "Slab").len(),
            8
        );
        assert_eq!(
            resolve_preset_entries("accessible_reading", "id", "Access").len(),
            6
        );
        assert_eq!(
            resolve_preset_entries("display_poster", "id", "Poster").len(),
            8
        );
        assert_eq!(resolve_preset_entries("indie_mono", "id", "Indie").len(), 8);
        assert_eq!(resolve_preset_entries("nerd_fonts", "id", "Nerd").len(), 6);
        assert_eq!(resolve_preset_entries("historical", "id", "Hist").len(), 7);
        assert_eq!(
            resolve_preset_entries("retro_pixel", "id", "Retro").len(),
            6
        );
        assert_eq!(
            resolve_preset_entries("global_scripts", "id", "Global").len(),
            6
        );
        assert_eq!(resolve_preset_entries("sil_scripts", "id", "SIL").len(), 6);
        assert_eq!(
            resolve_preset_entries("collletttivo", "id", "Coll").len(),
            5
        );
        assert_eq!(resolve_preset_entries("use_modify", "id", "Use").len(), 5);
        assert_eq!(
            resolve_preset_entries("oldschool_pc", "id", "Oldschool").len(),
            6
        );
        assert_eq!(
            resolve_preset_entries("indestructible", "id", "Indestructible").len(),
            5
        );
        assert_eq!(
            resolve_preset_entries("open_foundry", "id", "Open Foundry").len(),
            5
        );
    }
}
