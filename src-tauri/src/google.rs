//! Multi-foundry Discover catalog (Google Fonts, Fontshare, and Fontsource Open Foundry)
//! with local 7-day cache and streaming `.ttf`/`.otf` installer.
//! Restricts HTTP redirects strictly to vetted font registry and CDN hosts.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";
const DOWNLOAD_LIST_URL: &str = "https://fonts.google.com/download/list?family=";
const FONTSHARE_URL: &str = "https://api.fontshare.com/v2/fonts?offset=0&limit=100";
const FONTSOURCE_OTHER_URL: &str = "https://api.fontsource.org/v1/fonts?type=other";
const FONTSOURCE_DETAIL_URL: &str = "https://api.fontsource.org/v1/fonts/";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

fn is_allowed_host(host: &str) -> bool {
    matches!(
        host,
        "fonts.google.com"
            | "fonts.gstatic.com"
            | "api.fontshare.com"
            | "cdn.fontshare.com"
            | "api.fontsource.org"
            | "cdn.jsdelivr.net"
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
    pub preview_url: Option<String>,
    pub download_urls: Vec<String>,
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
    match raw.to_ascii_lowercase().as_str() {
        "sans" | "sans-serif" => "sans",
        "serif" | "slab" => "serif",
        "handwritten" | "handwriting" | "script" => "script",
        "mono" | "monospace" => "mono",
        _ => "display",
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
                    Some(GoogleFontEntry {
                        source: "google".into(),
                        source_id: name.clone(),
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
                source: "fontsource".into(),
                source_id: id,
                preview_url,
                download_urls,
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

    let (g_res, fs_res, src_res) = futures_util::future::join3(
        fetch_cached_json(&client, METADATA_URL, &google_file, force),
        fetch_cached_json(&client, FONTSHARE_URL, &fontshare_file, force),
        fetch_cached_json(&client, FONTSOURCE_OTHER_URL, &fontsource_file, force),
    )
    .await;

    let mut combined = Vec::new();
    let mut seen = HashSet::new();
    let mut first_err = None;

    for entries in [
        g_res.as_ref().map(compact_google_catalog),
        fs_res.as_ref().map(parse_fontshare_catalog),
        src_res.as_ref().map(parse_fontsource_other_catalog),
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
    let fs_file = data_dir.join("fontshare-metadata.json");
    if let Some(v) = fs::read_to_string(fs_file)
        .ok()
        .and_then(|t| parse_json(&t).ok())
    {
        if let Some(e) = parse_fontshare_catalog(&v)
            .into_iter()
            .find(|e| e.name.eq_ignore_ascii_case(family))
        {
            return Some(e);
        }
    }
    let src_file = data_dir.join("fontsource-other.json");
    if let Some(v) = fs::read_to_string(src_file)
        .ok()
        .and_then(|t| parse_json(&t).ok())
    {
        if let Some(e) = parse_fontsource_other_catalog(&v)
            .into_iter()
            .find(|e| e.name.eq_ignore_ascii_case(family))
        {
            return Some(e);
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
        let resp = client
            .get(file_url)
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
            on_progress(((idx as f64 + frac) / total_files * 100.0).min(99.0));
        }
        let base = Path::new(name)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{idx}.ttf"));
        // Sanitize brackets/commas in variable font filenames for Windows Explorer/GDI compatibility.
        let sanitized: String = base
            .chars()
            .map(|c| {
                if c == '[' || c == ']' || c == ',' {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let out_path = dest.join(sanitized);
        fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
        downloaded.push(out_path);
    }
    Ok(downloaded)
}

pub async fn download_family(
    data_dir: &Path,
    family: &str,
    dest: &Path,
    on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let client = http_client()?;
    if let Some(entry) = find_cached_non_google_entry(data_dir, family) {
        let urls = if entry.source == "fontsource" {
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
