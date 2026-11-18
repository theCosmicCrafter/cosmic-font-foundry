//! Google Fonts Discover catalog (with local 7-day cache) and pre-install specimen metadata.
//! Restricts network redirects strictly to `fonts.google.com` and `fonts.gstatic.com`.

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";
const DOWNLOAD_LIST_URL: &str = "https://fonts.google.com/download/list?family=";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

fn http_client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent("cosmic-font-foundry")
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let ok = attempt
                .url()
                .host_str()
                .is_some_and(|h| h == "fonts.google.com" || h == "fonts.gstatic.com");
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
fn parse_google_json(text: &str) -> Result<Value, String> {
    let t = text.trim_start();
    let t = t.strip_prefix(")]}'").unwrap_or(t);
    serde_json::from_str(t).map_err(|e| e.to_string())
}

#[derive(Serialize, Clone)]
pub struct GoogleAxis {
    pub tag: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

#[derive(Serialize, Clone)]
pub struct GoogleFontEntry {
    pub name: String,
    pub category: &'static str,
    pub styles: usize,
    pub popularity: i64,
    pub date: String,
    pub scripts: Vec<String>,
    pub designers: Vec<String>,
    pub is_variable: bool,
    pub axes: Vec<GoogleAxis>,
    pub variants: Vec<String>,
}

fn map_category(f: &Value) -> &'static str {
    let script = f["primaryScript"].as_str().unwrap_or("");
    if !script.is_empty() && script != "Latn" {
        return "other";
    }
    match f["category"].as_str().unwrap_or("") {
        "Sans Serif" => "sans",
        "Serif" => "serif",
        "Handwriting" => "script",
        "Monospace" => "mono",
        _ => "display",
    }
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

fn compact_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    v["familyMetadataList"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|f| {
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
                        name: f["family"].as_str()?.to_string(),
                        category: map_category(f),
                        styles: f["fonts"].as_object().map_or(1, |o| o.len()),
                        popularity: f["popularity"].as_i64().unwrap_or(i64::MAX),
                        date: f["dateAdded"].as_str().unwrap_or("").to_string(),
                        scripts: map_scripts(f),
                        designers,
                        is_variable,
                        axes,
                        variants,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn fetch_catalog(data_dir: &Path, force: bool) -> Result<Vec<GoogleFontEntry>, String> {
    let file = data_dir.join("google-metadata.json");
    let fresh = fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|d| d < CACHE_TTL);
    if fresh && !force {
        if let Some(v) = fs::read_to_string(&file)
            .ok()
            .and_then(|t| parse_google_json(&t).ok())
        {
            return Ok(compact_catalog(&v));
        }
    }
    let fetched = async {
        let text = http_client()?
            .get(METADATA_URL)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let val = parse_google_json(&text)?;
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let _ = fs::write(&file, &text);
        Ok::<_, String>(val)
    }
    .await;

    match fetched {
        Ok(v) => Ok(compact_catalog(&v)),
        Err(err) => fs::read_to_string(&file)
            .ok()
            .and_then(|t| parse_google_json(&t).ok())
            .map(|v| compact_catalog(&v))
            .ok_or(err),
    }
}

fn select_font_files(refs: &[Value]) -> Vec<(String, String)> {
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

pub async fn download_family(
    family: &str,
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let client = http_client()?;
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
    let val = parse_google_json(&text)?;
    let files = select_font_files(
        val["manifest"]["fileRefs"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]),
    );
    if files.is_empty() {
        return Err("Google Fonts returned no downloadable font files for this family".into());
    }
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
        // Sanitize brackets/commas in Google variable font filenames for Windows Explorer/GDI compatibility.
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
