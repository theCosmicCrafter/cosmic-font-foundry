//! Multi-foundry Discover catalog (Google Fonts, Fontshare, Fontsource Open Foundry,
//! Adobe & Dev Originals, and Font Squirrel) with local 7-day cache and streaming installer.
//! Restricts HTTP redirects strictly to vetted font registry and CDN hosts.

use cosmic_core::sync::extract_zip_fonts;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";
const DOWNLOAD_LIST_URL: &str = "https://fonts.google.com/download/list?family=";
const FONTSHARE_URL: &str = "https://api.fontshare.com/v2/fonts?offset=0&limit=100";
const FONTSOURCE_OTHER_URL: &str = "https://api.fontsource.org/v1/fonts?type=other";
const FONTSOURCE_DETAIL_URL: &str = "https://api.fontsource.org/v1/fonts/";
const FONTSQUIRREL_URL: &str = "https://www.fontsquirrel.com/api/fontlist/all";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

const ADOBE_DEV_FAMILIES: &[&str] = &[
    "source sans 3",
    "source sans pro",
    "source serif 4",
    "source serif pro",
    "source code pro",
    "monaspace neon",
    "monaspace argon",
    "monaspace krypton",
    "monaspace xenon",
    "monaspace radon",
    "iosevka",
    "iosevka aile",
    "iosevka etoile",
    "iosevka curly",
    "iosevka curly slab",
    "iosevka charon",
    "iosevka charon mono",
    "commit mono",
    "geist",
    "geist mono",
    "geist pixel",
    "geist sans",
    "cascadia code",
    "cascadia mono",
    "fira code",
    "fira mono",
    "jetbrains mono",
    "victor mono",
    "hack",
    "league mono",
];

fn is_adobe_or_dev_family(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    ADOBE_DEV_FAMILIES.contains(&lower.as_str())
}

fn is_allowed_host(host: &str) -> bool {
    matches!(
        host,
        "fonts.google.com"
            | "fonts.gstatic.com"
            | "api.fontshare.com"
            | "cdn.fontshare.com"
            | "api.fontsource.org"
            | "cdn.jsdelivr.net"
            | "www.fontsquirrel.com"
            | "fontsquirrel.com"
    )
}

fn http_client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent("cosmic-font-foundry")
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let ok = attempt.url().host_str().is_some_and(is_allowed_host);
            if ok && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}

/// Google prefixes JSON responses with `)]}'`.
fn parse_json(text: &str) -> Result<Value, String> {
    let t = text.trim_start();
    let t = t.strip_prefix(")]}'").unwrap_or(t);
    serde_json::from_str(t).map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GoogleAxis {
    pub tag: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GoogleFontEntry {
    pub name: String,
    pub category: String,
    pub styles: usize,
    pub popularity: i64,
    pub date: String,
    pub scripts: Vec<String>,
    pub designers: Vec<String>,
    pub is_variable: bool,
    pub axes: Vec<GoogleAxis>,
    pub variants: Vec<String>,
    pub source: String,
    pub source_id: String,
    #[serde(default)]
    pub source_label: Option<String>,
    pub preview_url: Option<String>,
    pub download_urls: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomFontSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub kind: String,
    pub enabled: bool,
    pub last_synced: u64,
    pub font_count: usize,
    pub last_error: Option<String>,
}

fn map_google_category(f: &Value) -> String {
    let script = f["primaryScript"].as_str().unwrap_or("");
    if !script.is_empty() && script != "Latn" {
        return "other".into();
    }
    match f["category"].as_str().unwrap_or("") {
        "Sans Serif" => "sans",
        "Serif" => "serif",
        "Handwriting" => "script",
        "Monospace" => "mono",
        _ => "display",
    }
    .into()
}

fn map_generic_category(raw: &str) -> String {
    let s = raw.to_ascii_lowercase();
    if s.contains("sans") {
        "sans"
    } else if s.contains("serif") || s.contains("slab") {
        "serif"
    } else if s.contains("hand") || s.contains("script") || s.contains("calligraph") {
        "script"
    } else if s.contains("mono") || s.contains("typewriter") {
        "mono"
    } else {
        "display"
    }
    .into()
}

fn map_scripts(f: &Value) -> Vec<String> {
    let mut out = BTreeSet::new();
    if let Some(subsets) = f["subsets"].as_array() {
        for s in subsets.iter().filter_map(Value::as_str) {
            match s {
                "latin" => {
                    out.insert("Latin".into());
                }
                "latin-ext" => {
                    out.insert("Latin Ext".into());
                }
                "cyrillic" | "cyrillic-ext" => {
                    out.insert("Cyrillic".into());
                }
                "greek" | "greek-ext" => {
                    out.insert("Greek".into());
                }
                "arabic" => {
                    out.insert("Arabic".into());
                }
                "hebrew" => {
                    out.insert("Hebrew".into());
                }
                "devanagari" => {
                    out.insert("Devanagari".into());
                }
                "vietnamese" => {
                    out.insert("Vietnamese".into());
                }
                "thai" => {
                    out.insert("Thai".into());
                }
                "japanese" => {
                    out.insert("Japanese".into());
                }
                "korean" => {
                    out.insert("Korean".into());
                }
                "chinese-simplified" | "chinese-traditional" | "chinese-hongkong" => {
                    out.insert("CJK".into());
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        out.insert("Latin".into());
    }
    out.into_iter().collect()
}

fn parse_axes(f: &Value) -> Vec<GoogleAxis> {
    f["axes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(GoogleAxis {
                        tag: a["tag"].as_str()?.to_string(),
                        min: a["min"].as_f64().unwrap_or(100.0),
                        max: a["max"].as_f64().unwrap_or(900.0),
                        default: a["defaultValue"].as_f64().unwrap_or(400.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn compact_google_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    v["familyMetadataList"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    let name = f["family"].as_str()?.to_string();
                    let axes = parse_axes(f);
                    let is_variable = !axes.is_empty();
                    let variants = f["fonts"]
                        .as_object()
                        .map(|o| o.keys().cloned().collect())
                        .unwrap_or_default();
                    let designers = f["designers"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let source = if is_adobe_or_dev_family(&name) {
                        "adobe_dev"
                    } else {
                        "google"
                    };
                    Some(GoogleFontEntry {
                        source: source.into(),
                        source_id: name.clone(),
                        source_label: None,
                        name,
                        category: map_google_category(f),
                        styles: f["fonts"].as_object().map_or(1, |o| o.len()),
                        popularity: f["popularity"].as_i64().unwrap_or(i64::MAX),
                        date: f["dateAdded"].as_str().unwrap_or("").to_string(),
                        scripts: map_scripts(f),
                        designers,
                        is_variable,
                        axes,
                        variants,
                        preview_url: None,
                        download_urls: Vec::new(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_https_url(raw: &str) -> String {
    if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        raw.to_string()
    }
}

fn parse_fontshare_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(list) = v["fonts"].as_array() else {
        return Vec::new();
    };
    let mut ranked: Vec<(i64, GoogleFontEntry)> = list
        .iter()
        .filter_map(|f| {
            let name = f["name"].as_str()?.to_string();
            let slug = f["slug"].as_str()?.to_string();
            let styles_arr = f["styles"].as_array()?;
            if styles_arr.is_empty() {
                return None;
            }
            let default_style = styles_arr
                .iter()
                .find(|s| s["default"].as_bool() == Some(true))
                .or_else(|| styles_arr.first())?;
            let preview_url = default_style["file"]
                .as_str()
                .map(|u| format!("{}.woff2", normalize_https_url(u)));
            let download_urls: Vec<String> = styles_arr
                .iter()
                .filter_map(|s| s["file"].as_str())
                .map(|u| format!("{}.ttf", normalize_https_url(u)))
                .collect();
            let variants: Vec<String> = styles_arr
                .iter()
                .map(|s| {
                    let w = s["weight"]["number"].as_i64().unwrap_or(400);
                    let italic = s["is_italic"].as_bool().unwrap_or(false);
                    if italic {
                        format!("{w}i")
                    } else {
                        w.to_string()
                    }
                })
                .collect();
            let axes: Vec<GoogleAxis> = f["axes"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| {
                            let tag = a["property"]
                                .as_str()
                                .or_else(|| a["name"].as_str())?
                                .to_string();
                            Some(GoogleAxis {
                                tag,
                                min: a["range_left"].as_f64().unwrap_or(100.0),
                                max: a["range_right"].as_f64().unwrap_or(900.0),
                                default: a["range_default"].as_f64().unwrap_or(400.0),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let designers = f["designers"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|d| d["name"].as_str())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let script = match f["script"].as_str().unwrap_or("latin") {
                "devanagari" => "Devanagari",
                _ => "Latin",
            };
            let views = f["views"].as_i64().unwrap_or(0);
            let is_variable = !axes.is_empty();
            Some((
                views,
                GoogleFontEntry {
                    name,
                    category: map_generic_category(f["category"].as_str().unwrap_or("")),
                    styles: styles_arr.len(),
                    popularity: 0,
                    date: String::new(),
                    scripts: vec![script.into()],
                    designers,
                    is_variable,
                    axes,
                    variants,
                    source: "fontshare".into(),
                    source_id: slug,
                    source_label: None,
                    preview_url,
                    download_urls,
                },
            ))
        })
        .collect();
    ranked.sort_by_key(|a| std::cmp::Reverse(a.0));
    ranked
        .into_iter()
        .enumerate()
        .map(|(i, (_, mut entry))| {
            entry.popularity = (i as i64) + 1;
            entry
        })
        .collect()
}

fn parse_fontsource_other_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter(|f| f["type"].as_str() == Some("other"))
        .enumerate()
        .filter_map(|(idx, f)| {
            let id = f["id"].as_str()?.to_string();
            let name = f["family"].as_str()?.to_string();
            let def_subset = f["defSubset"].as_str().unwrap_or("latin");
            let weights: Vec<u64> = f["weights"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .filter(|w: &Vec<u64>| !w.is_empty())
                .unwrap_or_else(|| vec![400]);
            let styles_list: Vec<&str> = f["styles"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .filter(|s: &Vec<&str>| !s.is_empty())
                .unwrap_or_else(|| vec!["normal"]);
            let default_weight = if weights.contains(&400) {
                400
            } else {
                weights[0]
            };
            let default_style = if styles_list.contains(&"normal") {
                "normal"
            } else {
                styles_list[0]
            };
            let preview_url = Some(format!(
                "https://cdn.jsdelivr.net/fontsource/fonts/{id}@latest/{def_subset}-{default_weight}-{default_style}.woff2"
            ));
            let mut download_urls = Vec::new();
            let mut variants = Vec::new();
            for &w in &weights {
                for &st in &styles_list {
                    variants.push(if st == "italic" {
                        format!("{w}i")
                    } else {
                        w.to_string()
                    });
                    download_urls.push(format!(
                        "https://cdn.jsdelivr.net/fontsource/fonts/{id}@latest/{def_subset}-{w}-{st}.ttf"
                    ));
                }
            }
            let source = if is_adobe_or_dev_family(&name) {
                "adobe_dev"
            } else {
                "fontsource"
            };
            Some(GoogleFontEntry {
                name,
                category: map_generic_category(f["category"].as_str().unwrap_or("")),
                styles: variants.len().max(1),
                popularity: 50 + (idx as i64),
                date: f["lastModified"].as_str().unwrap_or("").to_string(),
                scripts: map_scripts(f),
                designers: Vec::new(),
                is_variable: f["variable"].as_bool().unwrap_or(false),
                axes: Vec::new(),
                variants,
                source: source.into(),
                source_id: id,
                source_label: None,
                preview_url,
                download_urls,
            })
        })
        .collect()
}

fn parse_fontsquirrel_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .enumerate()
        .filter_map(|(idx, f)| {
            let name = f["family_name"].as_str()?.trim().to_string();
            let slug = f["family_urlname"].as_str()?.trim().to_string();
            if name.is_empty() || slug.is_empty() {
                return None;
            }
            let styles = f["family_count"]
                .as_str()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let designers = f["foundry_name"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();
            let source = if is_adobe_or_dev_family(&name) {
                "adobe_dev"
            } else {
                "fontsquirrel"
            };
            Some(GoogleFontEntry {
                name,
                category: map_generic_category(f["classification"].as_str().unwrap_or("")),
                styles,
                popularity: 120 + (idx as i64),
                date: String::new(),
                scripts: vec!["Latin".into()],
                designers,
                is_variable: false,
                axes: Vec::new(),
                variants: vec!["400".into()],
                source: source.into(),
                source_id: slug.clone(),
                source_label: None,
                preview_url: None,
                download_urls: vec![format!(
                    "https://www.fontsquirrel.com/fonts/download/{slug}"
                )],
            })
        })
        .collect()
}

async fn fetch_cached_json(
    client: &reqwest::Client,
    url: &str,
    cache_file: &Path,
    force: bool,
) -> Result<Value, String> {
    let fresh = fs::metadata(cache_file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|d| d < CACHE_TTL);
    if fresh && !force {
        if let Some(v) = fs::read_to_string(cache_file)
            .ok()
            .and_then(|t| parse_json(&t).ok())
        {
            return Ok(v);
        }
    }
    let fetched = async {
        let text = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let val = parse_json(&text)?;
        if let Some(parent) = cache_file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let _ = fs::write(cache_file, &text);
        Ok::<_, String>(val)
    }
    .await;

    match fetched {
        Ok(v) => Ok(v),
        Err(err) => fs::read_to_string(cache_file)
            .ok()
            .and_then(|t| parse_json(&t).ok())
            .ok_or(err),
    }
}

pub async fn fetch_catalog(data_dir: &Path, force: bool) -> Result<Vec<GoogleFontEntry>, String> {
    let client = http_client()?;
    let google_file = data_dir.join("google-metadata.json");
    let fontshare_file = data_dir.join("fontshare-metadata.json");
    let fontsource_file = data_dir.join("fontsource-other.json");
    let fontsquirrel_file = data_dir.join("fontsquirrel-metadata.json");

    let (g_res, fs_res, src_res, sq_res) = futures_util::future::join4(
        fetch_cached_json(&client, METADATA_URL, &google_file, force),
        fetch_cached_json(&client, FONTSHARE_URL, &fontshare_file, force),
        fetch_cached_json(&client, FONTSOURCE_OTHER_URL, &fontsource_file, force),
        fetch_cached_json(&client, FONTSQUIRREL_URL, &fontsquirrel_file, force),
    )
    .await;

    let mut combined = Vec::new();
    let mut seen = HashSet::new();
    let mut first_err = None;

    for entries in [
        g_res.as_ref().map(compact_google_catalog),
        fs_res.as_ref().map(parse_fontshare_catalog),
        src_res.as_ref().map(parse_fontsource_other_catalog),
        sq_res.as_ref().map(parse_fontsquirrel_catalog),
    ] {
        match entries {
            Ok(list) => {
                for entry in list {
                    if seen.insert(entry.name.to_lowercase()) {
                        combined.push(entry);
                    }
                }
            }
            Err(e) if first_err.is_none() => first_err = Some(e.clone()),
            Err(_) => {}
        }
    }

    if force {
        refresh_enabled_custom_sources(data_dir).await;
    }

    for entry in enabled_custom_entries(data_dir) {
        combined.push(entry);
    }

    if combined.is_empty() {
        return Err(first_err.unwrap_or_else(|| "Unable to load font catalogs".into()));
    }
    Ok(combined)
}

fn select_google_font_files(refs: &[Value]) -> Vec<(String, String)> {
    let all: Vec<(String, String)> = refs
        .iter()
        .filter_map(|r| {
            Some((
                r["filename"].as_str()?.to_string(),
                r["url"].as_str()?.to_string(),
            ))
        })
        .filter(|(name, url)| {
            let n = name.to_lowercase();
            (n.ends_with(".ttf") || n.ends_with(".otf"))
                && url.starts_with("https://fonts.gstatic.com/")
        })
        .collect();
    let root: Vec<_> = all
        .iter()
        .filter(|(name, _)| !name.contains('/'))
        .cloned()
        .collect();
    if root.is_empty() {
        all
    } else {
        root
    }
}

fn find_cached_non_google_entry(data_dir: &Path, family: &str) -> Option<GoogleFontEntry> {
    type CatalogParser = fn(&Value) -> Vec<GoogleFontEntry>;
    let sources: [(&str, CatalogParser); 3] = [
        ("fontshare-metadata.json", parse_fontshare_catalog),
        ("fontsource-other.json", parse_fontsource_other_catalog),
        ("fontsquirrel-metadata.json", parse_fontsquirrel_catalog),
    ];
    for (file, parser) in sources {
        if let Some(v) = fs::read_to_string(data_dir.join(file))
            .ok()
            .and_then(|t| parse_json(&t).ok())
        {
            if let Some(e) = parser(&v)
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(family))
            {
                return Some(e);
            }
        }
    }
    None
}

async fn resolve_fontsource_urls(client: &reqwest::Client, entry: &GoogleFontEntry) -> Vec<String> {
    let detail_url = format!("{FONTSOURCE_DETAIL_URL}{}", entry.source_id);
    let Ok(resp) = client.get(&detail_url).send().await else {
        return entry.download_urls.clone();
    };
    let Ok(resp) = resp.error_for_status() else {
        return entry.download_urls.clone();
    };
    let Ok(text) = resp.text().await else {
        return entry.download_urls.clone();
    };
    let Ok(val) = parse_json(&text) else {
        return entry.download_urls.clone();
    };
    let def_subset = val["defSubset"].as_str().unwrap_or("latin");
    let mut urls = Vec::new();
    if let Some(variants) = val["variants"].as_object() {
        for styles in variants.values() {
            let Some(style_map) = styles.as_object() else {
                continue;
            };
            for subsets in style_map.values() {
                let target = subsets
                    .get(def_subset)
                    .or_else(|| subsets.as_object().and_then(|m| m.values().next()));
                if let Some(ttf) = target.and_then(|s| s["url"]["ttf"].as_str()) {
                    urls.push(ttf.to_string());
                }
            }
        }
    }
    if urls.is_empty() {
        entry.download_urls.clone()
    } else {
        urls
    }
}

fn sanitize_font_filename(name: &str, fallback_idx: usize) -> String {
    let base = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{fallback_idx}.ttf"));
    // Sanitize brackets/commas in variable font filenames for Windows Explorer/GDI compatibility.
    base.chars()
        .map(|c| {
            if c == '[' || c == ']' || c == ',' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

async fn download_bytes(
    client: &reqwest::Client,
    url: &str,
    mut on_frac: impl FnMut(f64),
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0) as f64;
    let mut bytes = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        let frac = if total > 0.0 {
            (bytes.len() as f64 / total).min(1.0)
        } else {
            0.5
        };
        on_frac(frac);
    }
    Ok(bytes)
}

async fn download_files(
    client: &reqwest::Client,
    files: &[(String, String)],
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let _ = fs::remove_dir_all(dest);
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let total_files = files.len() as f64;
    let mut downloaded = Vec::new();
    for (idx, (name, file_url)) in files.iter().enumerate() {
        let bytes = download_bytes(client, file_url, |frac| {
            on_progress(((idx as f64 + frac) / total_files * 100.0).min(99.0));
        })
        .await?;
        let out_path = dest.join(sanitize_font_filename(name, idx));
        fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
        downloaded.push(out_path);
    }
    Ok(downloaded)
}

async fn download_zip_family(
    client: &reqwest::Client,
    zip_url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let zip_bytes = download_bytes(client, zip_url, |frac| {
        on_progress((frac * 90.0).min(90.0));
    })
    .await?;
    let extracted = extract_zip_fonts(&zip_bytes)?;
    if extracted.is_empty() {
        return Err("Archive contained no .ttf or .otf font files".into());
    }
    let _ = fs::remove_dir_all(dest);
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for (idx, (fname, bytes)) in extracted.into_iter().enumerate() {
        let out_path = dest.join(sanitize_font_filename(&fname, idx));
        fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
        paths.push(out_path);
    }
    on_progress(99.0);
    Ok(paths)
}

pub async fn download_family(
    data_dir: &Path,
    family: &str,
    dest: &Path,
    on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let client = http_client()?;
    if let Some(entry) = find_cached_custom_entry(data_dir, family) {
        let custom_client = custom_http_client()?;
        if let Some(zip_url) = entry.download_urls.iter().find(|u| url_has_ext(u, ".zip")) {
            return download_zip_family(&custom_client, zip_url, dest, on_progress).await;
        }
        let font_urls: Vec<String> = entry
            .download_urls
            .iter()
            .filter(|u| url_has_ext(u, ".ttf") || url_has_ext(u, ".otf"))
            .cloned()
            .collect();
        if !font_urls.is_empty() {
            let files: Vec<(String, String)> = font_urls
                .into_iter()
                .enumerate()
                .map(|(i, u)| {
                    let ext = if url_has_ext(&u, ".otf") {
                        "otf"
                    } else {
                        "ttf"
                    };
                    (format!("{}-{}.{}", entry.source_id, i, ext), u)
                })
                .collect();
            return download_files(&custom_client, &files, dest, on_progress).await;
        }
        return Err(
            "Custom feed entry contains preview/web links only (no direct .ttf, .otf, or .zip archive)."
                .into(),
        );
    }

    if let Some(entry) = find_cached_non_google_entry(data_dir, family) {
        if entry.source == "fontsquirrel" {
            if let Some(zip_url) = entry.download_urls.first() {
                return download_zip_family(&client, zip_url, dest, on_progress).await;
            }
        }
        let is_fontsource_cdn = entry
            .download_urls
            .first()
            .is_some_and(|u| u.contains("cdn.jsdelivr.net/fontsource"));
        let urls = if entry.source == "fontsource" || is_fontsource_cdn {
            resolve_fontsource_urls(&client, &entry).await
        } else {
            entry.download_urls.clone()
        };
        if !urls.is_empty() {
            let files: Vec<(String, String)> = urls
                .into_iter()
                .enumerate()
                .map(|(i, u)| (format!("{}-{}.ttf", entry.source_id, i), u))
                .collect();
            return download_files(&client, &files, dest, on_progress).await;
        }
    }

    let url = format!("{DOWNLOAD_LIST_URL}{}", family.replace(' ', "+"));
    let text = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let val = parse_json(&text)?;
    let files = select_google_font_files(
        val["manifest"]["fileRefs"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]),
    );
    if files.is_empty() {
        return Err("Font registry returned no downloadable font files for this family".into());
    }
    download_files(&client, &files, dest, on_progress).await
}

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

fn custom_http_client() -> Result<reqwest::Client, String> {
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

fn url_has_ext(raw: &str, ext: &str) -> bool {
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

fn enabled_custom_entries(data_dir: &Path) -> Vec<GoogleFontEntry> {
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

fn find_cached_custom_entry(data_dir: &Path, family: &str) -> Option<GoogleFontEntry> {
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

async fn refresh_enabled_custom_sources(data_dir: &Path) {
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
            name,
            preview_url,
            download_urls,
        });
    }

    Ok(entries)
}

fn resolve_preset_entries(
    preset_key: &str,
    source_id_key: &str,
    source_name: &str,
) -> Vec<GoogleFontEntry> {
    let src_tag = format!("custom:{source_id_key}");
    let items: Vec<(&str, &str, &str, usize, bool, &str, &str)> = match preset_key {
        "velvetyne" => vec![
            (
                "Syne",
                "display",
                "Bonjour Monde",
                5,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/syne@latest/latin-700-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/syne@latest/latin-700-normal.ttf",
            ),
            (
                "Silkscreen",
                "display",
                "Jason Kottke / Velvetyne",
                2,
                false,
                "https://cdn.jsdelivr.net/fontsource/fonts/silkscreen@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/silkscreen@latest/latin-400-normal.ttf",
            ),
            (
                "Unbounded",
                "display",
                "Studio Koto",
                8,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/unbounded@latest/latin-600-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/unbounded@latest/latin-600-normal.ttf",
            ),
            (
                "Bricolage Grotesque",
                "sans",
                "Mathieu Triay",
                8,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/bricolage-grotesque@latest/latin-500-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/bricolage-grotesque@latest/latin-500-normal.ttf",
            ),
        ],
        "leagueof" => vec![
            (
                "League Spartan",
                "sans",
                "Matt Bailey & Tyler Finck",
                9,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/league-spartan@latest/latin-600-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/league-spartan@latest/latin-600-normal.ttf",
            ),
            (
                "Orbitron",
                "display",
                "Matt McInerney",
                6,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/orbitron@latest/latin-600-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/orbitron@latest/latin-600-normal.ttf",
            ),
            (
                "League Gothic",
                "display",
                "Micah Rich & Caroline Hadilaksono",
                2,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/league-gothic@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/league-gothic@latest/latin-400-normal.ttf",
            ),
            (
                "League Mono",
                "mono",
                "Tyler Finck",
                8,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/league-mono@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/league-mono@latest/latin-400-normal.ttf",
            ),
        ],
        "indie_mono" => vec![
            (
                "Fira Code",
                "mono",
                "Nikita Prokopov",
                5,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/fira-code@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/fira-code@latest/latin-400-normal.ttf",
            ),
            (
                "JetBrains Mono",
                "mono",
                "JetBrains",
                16,
                true,
                "https://cdn.jsdelivr.net/fontsource/fonts/jetbrains-mono@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/jetbrains-mono@latest/latin-400-normal.ttf",
            ),
            (
                "Victor Mono",
                "mono",
                "Rune B",
                14,
                false,
                "https://cdn.jsdelivr.net/fontsource/fonts/victor-mono@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/victor-mono@latest/latin-400-normal.ttf",
            ),
            (
                "Space Mono",
                "mono",
                "Colophon Foundry",
                4,
                false,
                "https://cdn.jsdelivr.net/fontsource/fonts/space-mono@latest/latin-400-normal.woff2",
                "https://cdn.jsdelivr.net/fontsource/fonts/space-mono@latest/latin-400-normal.ttf",
            ),
        ],
        _ => Vec::new(),
    };

    items
        .into_iter()
        .enumerate()
        .map(
            |(idx, (name, cat, designer, styles, is_var, woff2, ttf))| GoogleFontEntry {
                name: name.to_string(),
                category: cat.to_string(),
                styles,
                popularity: (idx as i64) + 1,
                date: "2026-01-01".into(),
                scripts: vec!["Latin".into()],
                designers: vec![designer.to_string()],
                is_variable: is_var,
                axes: Vec::new(),
                variants: vec!["400".into()],
                source: src_tag.clone(),
                source_id: name.to_string(),
                source_label: Some(source_name.to_string()),
                preview_url: Some(woff2.to_string()),
                download_urls: vec![ttf.to_string()],
            },
        )
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
}
