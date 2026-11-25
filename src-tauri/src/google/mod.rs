//! Multi-foundry Discover catalog (Google Fonts, Fontshare, Fontsource Open Foundry,
//! Adobe & Dev Originals, Font Squirrel, and Nerd Fonts) with local 7-day cache and streaming installer.
//! Restricts HTTP redirects strictly to vetted font registry and CDN hosts.

use cosmic_core::sync::extract_zip_fonts;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

mod feeds;
mod registries;
pub use feeds::*;
pub use registries::*;

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";
const GWFH_URL: &str = "https://gwfh.mranftl.com/api/fonts";
const DOWNLOAD_LIST_URL: &str = "https://fonts.google.com/download/list?family=";
const FONTSHARE_URL: &str = "https://api.fontshare.com/v2/fonts?offset=0&limit=100";
const FONTSHARE_PAGE2_URL: &str = "https://api.fontshare.com/v2/fonts?offset=100&limit=100";
const FONTSOURCE_ALL_URL: &str = "https://api.fontsource.org/v1/fonts";
const FONTSOURCE_DETAIL_URL: &str = "https://api.fontsource.org/v1/fonts/";
const NERDFONTS_URL: &str = "https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest";
const BUNNY_URL: &str = "https://fonts.bunny.net/list";
pub(super) const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

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
    "ibm plex sans",
    "ibm plex serif",
    "ibm plex mono",
    "intel one mono",
    "maple mono",
    "0xproto",
    "fantasque sans mono",
    "adwaita sans",
    "adwaita mono",
    "atkinson hyperlegible",
    "atkinson hyperlegible next",
    "atkinson hyperlegible mono",
    "inter",
    "recursive",
    "red hat display",
    "red hat text",
    "red hat mono",
    "ubuntu sans",
    "ubuntu sans mono",
    "space grotesk",
    "public sans",
];

pub(super) fn is_adobe_or_dev_family(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    ADOBE_DEV_FAMILIES.contains(&lower.as_str())
}

fn is_allowed_host(host: &str) -> bool {
    matches!(
        host,
        "fonts.google.com"
            | "fonts.googleapis.com"
            | "fonts.gstatic.com"
            | "gwfh.mranftl.com"
            | "api.fontshare.com"
            | "cdn.fontshare.com"
            | "api.fontsource.org"
            | "cdn.jsdelivr.net"
            | "www.fontsquirrel.com"
            | "fontsquirrel.com"
            | "api.github.com"
            | "github.com"
            | "fonts.bunny.net"
            | "cdn.bunny.net"
            | "formulae.brew.sh"
            | "fontesk.com"
            | "www.fontesk.com"
    ) || host.ends_with(".githubusercontent.com")
}

fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent("cosmic-font-foundry")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_secs))
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

pub(super) fn http_client() -> Result<reqwest::Client, String> {
    build_http_client(22)
}

fn download_http_client() -> Result<reqwest::Client, String> {
    build_http_client(120)
}

/// Google prefixes JSON responses with `)]}'`.
pub(super) fn parse_json(text: &str) -> Result<Value, String> {
    let t = text.trim_start();
    let t = t.strip_prefix(")]}'").unwrap_or(t);
    serde_json::from_str(t).map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GoogleAxis {
    pub tag: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
    #[serde(default)]
    pub version_tag: Option<String>,
    pub preview_url: Option<String>,
    pub download_urls: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub redistribution_allowed: Option<bool>,
    #[serde(default)]
    pub download_mode: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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

pub(super) fn map_generic_category(raw: &str) -> String {
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
                    let homepage = Some(format!(
                        "https://fonts.google.com/specimen/{}",
                        name.replace(' ', "+")
                    ));
                    Some(GoogleFontEntry {
                        source: source.into(),
                        source_id: name.clone(),
                        source_label: None,
                        version_tag: None,
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
                        homepage,
                        license: Some("SIL OFL / Open Source".into()),
                        redistribution_allowed: Some(true),
                        download_mode: Some("direct".into()),
                        ..Default::default()
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
                    homepage: Some(format!("https://www.fontshare.com/fonts/{slug}")),
                    source_id: slug,
                    source_label: None,
                    version_tag: None,
                    preview_url,
                    download_urls,
                    license: Some("ITF-FFL (Free Personal & Commercial)".into()),
                    redistribution_allowed: Some(false),
                    download_mode: Some("direct".into()),
                    ..Default::default()
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

fn parse_fontsource_catalog_filtered(v: &Value, include_google: bool) -> Vec<GoogleFontEntry> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter(|f| {
            let ty = f["type"].as_str().unwrap_or("");
            if include_google {
                ty == "other" || ty == "google"
            } else {
                ty == "other"
            }
        })
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
            let is_other = f["type"].as_str() == Some("other");
            let source = if is_adobe_or_dev_family(&name) {
                "adobe_dev"
            } else if is_other {
                "fontsource"
            } else {
                "google"
            };
            Some(GoogleFontEntry {
                name,
                category: map_generic_category(f["category"].as_str().unwrap_or("")),
                styles: variants.len().max(1),
                popularity: if is_other {
                    50 + (idx as i64)
                } else {
                    5_000 + (idx as i64)
                },
                date: f["lastModified"].as_str().unwrap_or("").to_string(),
                scripts: map_scripts(f),
                designers: Vec::new(),
                is_variable: f["variable"].as_bool().unwrap_or(false),
                axes: Vec::new(),
                variants,
                source: source.into(),
                homepage: Some(format!("https://fontsource.org/fonts/{id}")),
                source_id: id,
                source_label: None,
                version_tag: None,
                preview_url,
                download_urls,
                license: Some("Open Source".into()),
                redistribution_allowed: Some(true),
                download_mode: Some("direct".into()),
                ..Default::default()
            })
        })
        .collect()
}

fn parse_fontsource_other_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    parse_fontsource_catalog_filtered(v, false)
}

fn parse_fontsource_all_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    parse_fontsource_catalog_filtered(v, true)
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
                homepage: Some(format!("https://www.fontsquirrel.com/fonts/{slug}")),
                source_id: slug.clone(),
                source_label: None,
                version_tag: None,
                preview_url: None,
                download_urls: vec![format!(
                    "https://www.fontsquirrel.com/fonts/download/{slug}"
                )],
                license: Some("Commercial Free (Font Squirrel)".into()),
                redistribution_allowed: Some(false),
                download_mode: Some("origin_link".into()),
                ..Default::default()
            })
        })
        .collect()
}

fn format_nerdfont_display_name(stem: &str) -> String {
    let clean = stem.replace('_', " ").trim().to_string();
    if clean.to_ascii_lowercase().contains("nerd") {
        clean
    } else {
        format!("{clean} Nerd Font")
    }
}

fn nerdfont_preview_url(stem: &str) -> Option<String> {
    let slug = match stem.to_ascii_lowercase().as_str() {
        "jetbrainsmono" => "jetbrains-mono",
        "firacode" => "fira-code",
        "firamono" => "fira-mono",
        "hack" => "hack",
        "cascadiacode" => "cascadia-code",
        "cascadiamono" => "cascadia-mono",
        "ubuntumono" => "ubuntu-mono",
        "ubuntu" => "ubuntu",
        "ubuntusans" => "ubuntu-sans",
        "ubuntusansmono" => "ubuntu-sans-mono",
        "spacemono" => "space-mono",
        "victormono" => "victor-mono",
        "iosevka" => "iosevka",
        "inconsolata" => "inconsolata",
        "robotomono" => "roboto-mono",
        "saucecodepro" => "source-code-pro",
        "geistmono" => "geist-mono",
        "commitmono" => "commit-mono",
        "monaspace" => "monaspace-neon",
        "blexmono" => "ibm-plex-mono",
        "anonymicepro" => "anonymous-pro",
        "fantasquesansmono" => "fantasque-sans-mono",
        "sharetechmono" => "share-tech-mono",
        _ => return None,
    };
    Some(format!(
        "https://cdn.jsdelivr.net/fontsource/fonts/{slug}@latest/latin-400-normal.woff2"
    ))
}

pub fn parse_nerdfonts_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(assets) = v["assets"].as_array() else {
        return Vec::new();
    };
    let version_tag = v["tag_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let date = v["published_at"].as_str().unwrap_or("").to_string();
    let mut seen = HashSet::new();

    assets
        .iter()
        .enumerate()
        .filter_map(|(idx, asset)| {
            let asset_name = asset["name"].as_str()?.trim();
            let download_url = asset["browser_download_url"].as_str()?.trim();
            if asset_name.is_empty()
                || download_url.is_empty()
                || !asset_name.to_ascii_lowercase().ends_with(".zip")
            {
                return None;
            }
            let stem = asset_name
                .strip_suffix(".zip")
                .or_else(|| asset_name.strip_suffix(".ZIP"))?
                .trim();
            if stem.is_empty() {
                return None;
            }
            let name = format_nerdfont_display_name(stem);
            if !seen.insert(name.to_ascii_lowercase()) {
                return None;
            }
            Some(GoogleFontEntry {
                name,
                category: "mono".into(),
                styles: 1,
                popularity: 100 + (idx as i64),
                date: date.clone(),
                scripts: vec!["Latin".into(), "Symbols".into()],
                designers: vec!["Nerd Fonts".into()],
                is_variable: false,
                axes: Vec::new(),
                variants: vec!["400".into()],
                source: "nerdfonts".into(),
                source_id: asset_name.to_string(),
                source_label: None,
                version_tag: version_tag.clone(),
                preview_url: nerdfont_preview_url(stem),
                download_urls: vec![download_url.to_string()],
                homepage: Some("https://www.nerdfonts.com/font-downloads".into()),
                license: Some("MIT / SIL OFL".into()),
                redistribution_allowed: Some(true),
                download_mode: Some("direct".into()),
                ..Default::default()
            })
        })
        .collect()
}

fn format_slug_display_name(slug: &str) -> String {
    slug.split('-')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_bunny_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(map) = v.as_object() else {
        return Vec::new();
    };
    map.iter()
        .enumerate()
        .filter_map(|(idx, (slug, obj))| {
            let slug = slug.trim();
            if slug.is_empty() || !obj.is_object() {
                return None;
            }
            let name = obj["familyName"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format_slug_display_name(slug));
            if name.is_empty() {
                return None;
            }
            let def_subset = obj["defSubset"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("latin");
            let weights: Vec<u64> = obj["weights"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .filter(|w: &Vec<u64>| !w.is_empty())
                .unwrap_or_else(|| vec![400]);
            let styles_list: Vec<&str> = obj["styles"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .filter(|s: &Vec<&str>| !s.is_empty())
                .unwrap_or_else(|| vec!["normal"]);
            let mut variants = Vec::new();
            let mut download_urls = Vec::new();
            for &w in &weights {
                for &st in &styles_list {
                    variants.push(if st == "italic" {
                        format!("{w}i")
                    } else {
                        w.to_string()
                    });
                    download_urls.push(format!(
                        "https://cdn.jsdelivr.net/fontsource/fonts/{slug}@latest/{def_subset}-{w}-{st}.ttf"
                    ));
                }
            }
            let mut scripts = map_scripts(obj);
            if !scripts.iter().any(|s| s == "Latin") {
                scripts.insert(0, "Latin".into());
            }
            Some(GoogleFontEntry {
                name,
                category: map_generic_category(obj["category"].as_str().unwrap_or("")),
                styles: variants.len().max(1),
                popularity: 50_000 + (idx as i64),
                date: String::new(),
                scripts,
                designers: Vec::new(),
                is_variable: obj["isVariable"].as_bool().unwrap_or(false),
                axes: Vec::new(),
                variants,
                source: "bunny".into(),
                homepage: Some(format!("https://fonts.bunny.net/family/{slug}")),
                source_id: slug.to_string(),
                source_label: None,
                version_tag: None,
                preview_url: None,
                download_urls,
                license: Some("Open Source".into()),
                redistribution_allowed: Some(true),
                download_mode: Some("direct".into()),
                ..Default::default()
            })
        })
        .collect()
}

pub(super) fn parse_gwfh_catalog(v: &Value) -> Vec<GoogleFontEntry> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .enumerate()
        .filter_map(|(idx, f)| {
            let name = f["family"].as_str()?.trim().to_string();
            let id = f["id"].as_str()?.trim().to_string();
            if name.is_empty() || id.is_empty() {
                return None;
            }
            let variants: Vec<String> = f["variants"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(|s| match s {
                            "regular" => "400".to_string(),
                            "italic" => "400i".to_string(),
                            other if other.ends_with("italic") => {
                                format!("{}i", other.trim_end_matches("italic"))
                            }
                            other => other.to_string(),
                        })
                        .collect()
                })
                .filter(|v: &Vec<String>| !v.is_empty())
                .unwrap_or_else(|| vec!["400".into()]);
            let source = if is_adobe_or_dev_family(&name) {
                "adobe_dev"
            } else {
                "google"
            };
            Some(GoogleFontEntry {
                source: source.into(),
                source_id: id,
                source_label: None,
                version_tag: None,
                homepage: Some(format!(
                    "https://fonts.google.com/specimen/{}",
                    name.replace(' ', "+")
                )),
                name,
                category: map_generic_category(f["category"].as_str().unwrap_or("")),
                styles: variants.len().max(1),
                popularity: f["popularity"].as_i64().unwrap_or((idx as i64) + 1),
                date: f["lastModified"].as_str().unwrap_or("").to_string(),
                scripts: map_scripts(f),
                designers: Vec::new(),
                is_variable: false,
                axes: Vec::new(),
                variants,
                preview_url: None,
                download_urls: Vec::new(),
                license: Some("SIL OFL / Open Source".into()),
                redistribution_allowed: Some(true),
                download_mode: Some("direct".into()),
                ..Default::default()
            })
        })
        .collect()
}

pub(super) fn verify_sha256_if_present(bytes: &[u8], expected: Option<&str>) -> Result<(), String> {
    let Some(raw) = expected.map(str::trim) else {
        return Ok(());
    };
    if raw.is_empty() || raw.eq_ignore_ascii_case("no_check") {
        return Ok(());
    }
    let hex = raw
        .strip_prefix("sha256:")
        .or_else(|| raw.strip_prefix("SHA256:"))
        .unwrap_or(raw)
        .trim();
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(());
    }
    let actual = cosmic_core::reader::sha256_bytes(bytes);
    if actual.eq_ignore_ascii_case(hex) {
        Ok(())
    } else {
        Err(format!(
            "SHA-256 verification failed (expected {hex}, got {actual})"
        ))
    }
}

pub(super) fn should_prefer_google_desktop_download(entry: &GoogleFontEntry) -> bool {
    if matches!(entry.source.as_str(), "google" | "bunny") {
        return true;
    }
    entry
        .download_urls
        .iter()
        .any(|u| u.contains("cdn.jsdelivr.net/fontsource") || u.contains("fonts.gstatic.com"))
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
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(format!("HTTP {} from {url}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if text.trim().is_empty() {
            return Err(format!("Empty response from {url}"));
        }
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
    let custom_client = custom_http_client()?;
    let google_file = data_dir.join("google-metadata.json");
    let fontshare_file = data_dir.join("fontshare-metadata.json");
    let fontshare_p2_file = data_dir.join("fontshare-metadata-p2.json");
    let fontsource_file = data_dir.join("fontsource-all.json");
    let fontsquirrel_file = data_dir.join("fontget-fontsquirrel.json");
    let league_file = data_dir.join("fontget-league.json");
    let open_repo_file = data_dir.join("open-font-repository.json");
    let scoop_file = data_dir.join("scoop-fonts-index.json");
    let nerdfonts_file = data_dir.join("nerdfonts-metadata.json");
    let bunny_file = data_dir.join("bunny-metadata.json");

    let (
        ((g_res, fs_res, fs2_res, src_res), (sq_res, lg_res, ofr_res)),
        ((hb_res, sc_res), (nf_res, bunny_res)),
    ) = futures_util::future::join(
        futures_util::future::join(
            futures_util::future::join4(
                fetch_cached_json(&client, METADATA_URL, &google_file, force),
                fetch_cached_json(&client, FONTSHARE_URL, &fontshare_file, force),
                fetch_cached_json(&client, FONTSHARE_PAGE2_URL, &fontshare_p2_file, force),
                fetch_cached_json(&client, FONTSOURCE_ALL_URL, &fontsource_file, force),
            ),
            futures_util::future::join3(
                fetch_cached_json(
                    &custom_client,
                    FONTGET_FONTSQUIRREL_URL,
                    &fontsquirrel_file,
                    force,
                ),
                fetch_cached_json(&custom_client, FONTGET_LEAGUE_URL, &league_file, force),
                fetch_cached_json(&custom_client, OPEN_FONT_REPO_URL, &open_repo_file, force),
            ),
        ),
        futures_util::future::join(
            futures_util::future::join(
                fetch_homebrew_fonts(&client, data_dir, force),
                fetch_cached_json(&client, SCOOP_BUCKET_API_URL, &scoop_file, force),
            ),
            futures_util::future::join(
                fetch_cached_json(&client, NERDFONTS_URL, &nerdfonts_file, force),
                fetch_cached_json(&client, BUNNY_URL, &bunny_file, force),
            ),
        ),
    )
    .await;

    let mut combined = Vec::new();
    let mut seen = HashSet::new();
    let mut first_err = None;

    let g_entries_res: Result<Vec<GoogleFontEntry>, String> = match g_res.as_ref() {
        Ok(v) => Ok(compact_google_catalog(v)),
        Err(orig_err) => {
            let gwfh_file = data_dir.join("gwfh-metadata.json");
            match fetch_cached_json(&client, GWFH_URL, &gwfh_file, force).await {
                Ok(v) => Ok(parse_gwfh_catalog(&v)),
                Err(_) => Err(orig_err.clone()),
            }
        }
    };
    let google_ok = g_entries_res.is_ok();

    let sq_entries_res = sq_res.as_ref().map(|v| {
        parse_fontget_sources_catalog(v, "fontsquirrel", "Font Squirrel", "origin_link", false)
    });
    let lg_entries_res = lg_res.as_ref().map(|v| {
        parse_fontget_sources_catalog(
            v,
            "open_foundries",
            "League of Moveable Type",
            "direct",
            true,
        )
    });

    for entries in [
        g_entries_res,
        fs_res
            .as_ref()
            .map(parse_fontshare_catalog)
            .map_err(Clone::clone),
        fs2_res
            .as_ref()
            .map(parse_fontshare_catalog)
            .map_err(Clone::clone),
        lg_entries_res.map_err(Clone::clone),
        ofr_res
            .as_ref()
            .map(parse_open_font_repository)
            .map_err(Clone::clone),
        hb_res,
        src_res
            .as_ref()
            .map(|v| parse_fontsource_catalog_filtered(v, !google_ok))
            .map_err(Clone::clone),
        sq_entries_res.map_err(Clone::clone),
        sc_res
            .as_ref()
            .map(parse_scoop_bucket_index)
            .map_err(Clone::clone),
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

    if let Ok(nf_val) = nf_res {
        for entry in parse_nerdfonts_catalog(&nf_val) {
            if seen.insert(entry.name.to_lowercase()) {
                combined.push(entry);
            }
        }
    }

    match bunny_res.as_ref().map(parse_bunny_catalog) {
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
    if let Some(hb) = fs::read_to_string(data_dir.join("homebrew-fonts-slim.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<GoogleFontEntry>>(&t).ok())
        .and_then(|list| {
            list.into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(family))
        })
    {
        return Some(hb);
    }
    if let Some(v) = fs::read_to_string(data_dir.join("fontget-league.json"))
        .ok()
        .and_then(|t| parse_json(&t).ok())
    {
        if let Some(e) = parse_fontget_sources_catalog(
            &v,
            "open_foundries",
            "League of Moveable Type",
            "direct",
            true,
        )
        .into_iter()
        .find(|e| e.name.eq_ignore_ascii_case(family))
        {
            return Some(e);
        }
    }
    if let Some(v) = fs::read_to_string(data_dir.join("fontget-fontsquirrel.json"))
        .ok()
        .and_then(|t| parse_json(&t).ok())
    {
        if let Some(e) =
            parse_fontget_sources_catalog(&v, "fontsquirrel", "Font Squirrel", "origin_link", false)
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(family))
        {
            return Some(e);
        }
    }

    type CatalogParser = fn(&Value) -> Vec<GoogleFontEntry>;
    let sources: [(&str, CatalogParser); 9] = [
        ("fontshare-metadata.json", parse_fontshare_catalog),
        ("fontshare-metadata-p2.json", parse_fontshare_catalog),
        ("open-font-repository.json", parse_open_font_repository),
        ("fontsource-all.json", parse_fontsource_all_catalog),
        ("fontsource-other.json", parse_fontsource_other_catalog),
        ("scoop-fonts-index.json", parse_scoop_bucket_index),
        ("fontsquirrel-metadata.json", parse_fontsquirrel_catalog),
        ("nerdfonts-metadata.json", parse_nerdfonts_catalog),
        ("bunny-metadata.json", parse_bunny_catalog),
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
    expected_sha256: Option<&str>,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let zip_bytes = download_bytes(client, zip_url, |frac| {
        on_progress((frac * 90.0).min(90.0));
    })
    .await?;
    verify_sha256_if_present(&zip_bytes, expected_sha256)?;
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

async fn download_google_desktop_family(
    client: &reqwest::Client,
    family: &str,
    dest: &Path,
    on_progress: &mut impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
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
    download_files(client, &files, dest, &mut *on_progress).await
}

fn origin_link_error(entry: &GoogleFontEntry) -> String {
    format!(
        "This font must be downloaded from its official page: {}",
        entry.homepage.as_deref().unwrap_or("origin site")
    )
}

pub async fn download_family(
    data_dir: &Path,
    family: &str,
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>, String> {
    let client = download_http_client()?;
    if let Some(entry) = find_cached_custom_entry(data_dir, family) {
        if entry.download_mode.as_deref() == Some("origin_link") {
            return Err(origin_link_error(&entry));
        }
        if should_prefer_google_desktop_download(&entry) {
            if let Ok(paths) =
                download_google_desktop_family(&client, family, dest, &mut on_progress).await
            {
                return Ok(paths);
            }
        }
        let custom_client = custom_download_http_client()?;
        if let Some(zip_url) = entry.download_urls.iter().find(|u| url_has_ext(u, ".zip")) {
            return download_zip_family(
                &custom_client,
                zip_url,
                dest,
                entry.sha256.as_deref(),
                on_progress,
            )
            .await;
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
            match download_files(&custom_client, &files, dest, &mut on_progress).await {
                Ok(paths) => return Ok(paths),
                Err(_)
                    if entry
                        .download_urls
                        .first()
                        .is_some_and(|u| u.contains("cdn.jsdelivr.net/fontsource")) =>
                {
                    // Fall through to Google Fonts / Fontsource detail resolution if a CDN weight URL fails.
                }
                Err(e) => return Err(e),
            }
        } else {
            return Err(
                "Custom feed entry contains preview/web links only (no direct .ttf, .otf, or .zip archive)."
                    .into(),
            );
        }
    }

    if let Some(entry) = find_cached_non_google_entry(data_dir, family) {
        if entry.download_mode.as_deref() == Some("origin_link") {
            return Err(origin_link_error(&entry));
        }
        if entry.source == "scoop" {
            let public_client = custom_download_http_client()?;
            let text = public_client
                .get(&entry.source_id)
                .send()
                .await
                .map_err(|e| format!("Failed to fetch Scoop manifest: {e}"))?
                .error_for_status()
                .map_err(|e| format!("Scoop manifest HTTP error: {e}"))?
                .text()
                .await
                .map_err(|e| e.to_string())?;
            let val = parse_json(&text)?;
            let (urls, sha256) = parse_scoop_manifest(&val)?;
            if let Some(zip_url) = urls.iter().find(|u| url_has_ext(u, ".zip")) {
                return download_zip_family(
                    &public_client,
                    zip_url,
                    dest,
                    sha256.as_deref(),
                    on_progress,
                )
                .await;
            }
            let files: Vec<(String, String)> = urls
                .iter()
                .enumerate()
                .map(|(i, u)| {
                    let ext = if url_has_ext(u, ".otf") { "otf" } else { "ttf" };
                    (
                        format!("{}-{}.{}", entry.name.replace(' ', "-"), i, ext),
                        u.clone(),
                    )
                })
                .collect();
            if files.len() == 1 {
                let bytes = download_bytes(&public_client, &files[0].1, |f| {
                    on_progress((f * 95.0).min(95.0));
                })
                .await?;
                verify_sha256_if_present(&bytes, sha256.as_deref())?;
                let _ = fs::remove_dir_all(dest);
                fs::create_dir_all(dest).map_err(|e| e.to_string())?;
                let out_path = dest.join(sanitize_font_filename(&files[0].0, 0));
                fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
                on_progress(99.0);
                return Ok(vec![out_path]);
            }
            return download_files(&public_client, &files, dest, &mut on_progress).await;
        }
        if entry.source == "homebrew"
            || entry.source == "open_foundries"
            || entry.source_label.as_deref() == Some("Homebrew Open")
        {
            let public_client = custom_download_http_client()?;
            if let Some(zip_url) = entry.download_urls.iter().find(|u| url_has_ext(u, ".zip")) {
                return download_zip_family(
                    &public_client,
                    zip_url,
                    dest,
                    entry.sha256.as_deref(),
                    on_progress,
                )
                .await;
            }
            let font_urls: Vec<String> = entry
                .download_urls
                .iter()
                .filter(|u| url_has_ext(u, ".ttf") || url_has_ext(u, ".otf"))
                .cloned()
                .collect();
            if !font_urls.is_empty() {
                if font_urls.len() == 1 {
                    let bytes = download_bytes(&public_client, &font_urls[0], |f| {
                        on_progress((f * 95.0).min(95.0));
                    })
                    .await?;
                    verify_sha256_if_present(&bytes, entry.sha256.as_deref())?;
                    let _ = fs::remove_dir_all(dest);
                    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
                    let ext = if url_has_ext(&font_urls[0], ".otf") {
                        "otf"
                    } else {
                        "ttf"
                    };
                    let out_path = dest.join(sanitize_font_filename(
                        &format!("{}.{}", entry.source_id, ext),
                        0,
                    ));
                    fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
                    on_progress(99.0);
                    return Ok(vec![out_path]);
                }
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
                return download_files(&public_client, &files, dest, &mut on_progress).await;
            }
        }
        if entry.source == "nerdfonts" {
            if let Some(zip_url) = entry.download_urls.first() {
                return download_zip_family(
                    &client,
                    zip_url,
                    dest,
                    entry.sha256.as_deref(),
                    on_progress,
                )
                .await;
            }
        }
        if should_prefer_google_desktop_download(&entry) {
            if let Ok(res) =
                download_google_desktop_family(&client, family, dest, &mut on_progress).await
            {
                return Ok(res);
            }
        }
        let is_fontsource_cdn = entry
            .download_urls
            .first()
            .is_some_and(|u| u.contains("cdn.jsdelivr.net/fontsource"));
        let urls = if entry.source == "fontsource" || entry.source == "bunny" || is_fontsource_cdn {
            resolve_fontsource_urls(&client, &entry).await
        } else {
            entry.download_urls.clone()
        };
        if !urls.is_empty() {
            let files: Vec<(String, String)> = urls
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
            match download_files(&client, &files, dest, &mut on_progress).await {
                Ok(res) => return Ok(res),
                Err(_)
                    if entry.source == "bunny" || entry.source == "google" || is_fontsource_cdn => {
                }
                Err(err) => return Err(err),
            }
        }
    }

    download_google_desktop_family(&client, family, dest, &mut on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerdfonts_catalog_parser_extracts_zip_assets_and_maps_previews() {
        let payload = serde_json::json!({
            "tag_name": "v3.3.0",
            "published_at": "2025-01-15T12:00:00Z",
            "assets": [
                {
                    "name": "JetBrainsMono.zip",
                    "browser_download_url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/JetBrainsMono.zip"
                },
                {
                    "name": "JetBrainsMono.tar.xz",
                    "browser_download_url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/JetBrainsMono.tar.xz"
                },
                {
                    "name": "FiraCode.zip",
                    "browser_download_url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/FiraCode.zip"
                },
                {
                    "name": "Agave.zip",
                    "browser_download_url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/Agave.zip"
                },
                {
                    "name": "SHA256SUMS",
                    "browser_download_url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/SHA256SUMS"
                }
            ]
        });

        let entries = parse_nerdfonts_catalog(&payload);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].name, "JetBrainsMono Nerd Font");
        assert_eq!(entries[0].source, "nerdfonts");
        assert_eq!(entries[0].source_id, "JetBrainsMono.zip");
        assert_eq!(entries[0].category, "mono");
        assert_eq!(entries[0].scripts, vec!["Latin", "Symbols"]);
        assert_eq!(entries[0].designers, vec!["Nerd Fonts"]);
        assert_eq!(entries[0].version_tag.as_deref(), Some("v3.3.0"));
        assert_eq!(
            entries[0].preview_url.as_deref(),
            Some("https://cdn.jsdelivr.net/fontsource/fonts/jetbrains-mono@latest/latin-400-normal.woff2")
        );
        assert_eq!(
            entries[0].download_urls,
            vec!["https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/JetBrainsMono.zip"]
        );

        assert_eq!(entries[1].name, "FiraCode Nerd Font");
        assert_eq!(
            entries[1].preview_url.as_deref(),
            Some(
                "https://cdn.jsdelivr.net/fontsource/fonts/fira-code@latest/latin-400-normal.woff2"
            )
        );

        assert_eq!(entries[2].name, "Agave Nerd Font");
        assert!(entries[2].preview_url.is_none());
    }

    #[test]
    fn github_search_repos_parser_maps_fields_and_fallbacks() {
        let payload = serde_json::json!({
            "items": [
                {
                    "full_name": "tonsky/FiraCode",
                    "name": "FiraCode",
                    "description": "Free monospaced font with programming ligatures",
                    "stargazers_count": 76000,
                    "license": { "spdx_id": "OFL-1.1" },
                    "html_url": "https://github.com/tonsky/FiraCode",
                    "releases_url": "https://api.github.com/repos/tonsky/FiraCode/releases{/id}",
                    "updated_at": "2025-02-01T10:00:00Z"
                },
                {
                    "full_name": "example/NoLicenseFont",
                    "name": "NoLicenseFont",
                    "description": null,
                    "stargazers_count": 42,
                    "license": null,
                    "html_url": "https://github.com/example/NoLicenseFont",
                    "releases_url": "",
                    "updated_at": "2025-01-20T00:00:00Z"
                }
            ]
        });

        let repos = parse_github_search_repos(&payload);
        assert_eq!(repos.len(), 2);

        assert_eq!(repos[0].full_name, "tonsky/FiraCode");
        assert_eq!(repos[0].name, "FiraCode");
        assert_eq!(
            repos[0].description,
            "Free monospaced font with programming ligatures"
        );
        assert_eq!(repos[0].stars, 76000);
        assert_eq!(repos[0].license, "OFL-1.1");
        assert_eq!(repos[0].html_url, "https://github.com/tonsky/FiraCode");
        assert_eq!(
            repos[0].releases_url,
            "https://api.github.com/repos/tonsky/FiraCode/releases"
        );
        assert_eq!(repos[0].updated_at, "2025-02-01T10:00:00Z");

        assert_eq!(repos[1].description, "");
        assert_eq!(repos[1].license, "Unknown");
        assert_eq!(
            repos[1].releases_url,
            "https://api.github.com/repos/example/NoLicenseFont/releases"
        );
    }

    #[test]
    fn allowed_hosts_include_github_and_bunny() {
        assert!(is_allowed_host("api.github.com"));
        assert!(is_allowed_host("github.com"));
        assert!(is_allowed_host("fonts.bunny.net"));
        assert!(is_allowed_host("objects.githubusercontent.com"));
        assert!(is_allowed_host("release-assets.githubusercontent.com"));
        assert!(!is_allowed_host("evil.example.com"));
    }

    #[test]
    fn bunny_catalog_parser_maps_categories_variants_and_fallbacks() {
        let payload = serde_json::json!({
            "space-grotesk": {
                "familyName": "Space Grotesk",
                "category": "sans-serif",
                "weights": [400, 700],
                "styles": ["normal", "italic"],
                "isVariable": true,
                "defSubset": "latin",
                "subsets": ["latin", "latin-ext"]
            },
            "ibm-plex-mono": {
                "familyName": "IBM Plex Mono",
                "category": "monospace",
                "weights": [400],
                "styles": ["normal"],
                "isVariable": false,
                "defSubset": "latin"
            },
            "caveat": {
                "category": "handwriting"
            }
        });

        let entries = parse_bunny_catalog(&payload);
        assert_eq!(entries.len(), 3);

        let sg = entries
            .iter()
            .find(|e| e.source_id == "space-grotesk")
            .unwrap();
        assert_eq!(sg.name, "Space Grotesk");
        assert_eq!(sg.category, "sans");
        assert_eq!(sg.styles, 4);
        assert!(sg.is_variable);
        assert_eq!(sg.source, "bunny");
        assert!(sg.popularity >= 50_000);
        assert!(sg.preview_url.is_none());
        assert_eq!(sg.scripts, vec!["Latin", "Latin Ext"]);
        assert_eq!(sg.variants, vec!["400", "400i", "700", "700i"]);
        assert!(sg.download_urls.contains(
            &"https://cdn.jsdelivr.net/fontsource/fonts/space-grotesk@latest/latin-400-normal.ttf"
                .to_string()
        ));

        let mono = entries
            .iter()
            .find(|e| e.source_id == "ibm-plex-mono")
            .unwrap();
        assert_eq!(mono.category, "mono");
        assert_eq!(mono.styles, 1);
        assert!(!mono.is_variable);
        assert_eq!(mono.scripts, vec!["Latin"]);

        let caveat = entries.iter().find(|e| e.source_id == "caveat").unwrap();
        assert_eq!(caveat.name, "Caveat");
        assert_eq!(caveat.category, "script");
        assert_eq!(caveat.variants, vec!["400"]);
    }

    #[test]
    fn google_font_entry_deserializes_legacy_cache_without_new_fields() {
        let legacy = serde_json::json!({
            "name": "Inter",
            "category": "sans",
            "styles": 9,
            "popularity": 2,
            "date": "2024-01-01",
            "scripts": ["Latin"],
            "designers": ["Rasmus Andersson"],
            "is_variable": true,
            "axes": [],
            "variants": ["400"],
            "source": "google",
            "source_id": "Inter",
            "preview_url": null,
            "download_urls": []
        });
        let entry: GoogleFontEntry = serde_json::from_value(legacy).unwrap();
        assert_eq!(entry.name, "Inter");
        assert!(entry.description.is_none());
        assert!(entry.homepage.is_none());
        assert!(entry.license.is_none());
        assert!(entry.tags.is_empty());
        assert!(entry.sha256.is_none());
        assert!(entry.redistribution_allowed.is_none());
        assert!(entry.download_mode.is_none());
    }

    #[test]
    fn sha256_verifier_accepts_matching_and_rejects_mismatch() {
        let data = b"cosmic-font-foundry";
        let digest = cosmic_core::reader::sha256_bytes(data);
        assert!(verify_sha256_if_present(data, Some(&digest)).is_ok());
        assert!(verify_sha256_if_present(data, Some(&format!("sha256:{digest}"))).is_ok());
        assert!(verify_sha256_if_present(data, Some("no_check")).is_ok());
        assert!(verify_sha256_if_present(data, None).is_ok());
        let bad = "0".repeat(64);
        assert!(verify_sha256_if_present(data, Some(&bad)).is_err());
    }

    #[test]
    fn gwfh_catalog_parser_extracts_entries_and_variants() {
        let payload = serde_json::json!([
            {
                "id": "roboto",
                "family": "Roboto",
                "variants": ["regular", "italic", "700italic"],
                "subsets": ["latin", "cyrillic"],
                "category": "sans-serif",
                "lastModified": "2026-02-19",
                "popularity": 1
            }
        ]);
        let entries = parse_gwfh_catalog(&payload);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Roboto");
        assert_eq!(entries[0].category, "sans");
        assert_eq!(entries[0].variants, vec!["400", "400i", "700i"]);
        assert_eq!(entries[0].scripts, vec!["Cyrillic", "Latin"]);
        assert_eq!(entries[0].download_mode.as_deref(), Some("direct"));
    }

    #[test]
    fn desktop_binary_guard_routes_bunny_and_fontsource_to_google_first() {
        let bunny = GoogleFontEntry {
            name: "Space Grotesk".into(),
            source: "bunny".into(),
            ..Default::default()
        };
        assert!(should_prefer_google_desktop_download(&bunny));

        let fs_google = GoogleFontEntry {
            name: "Inter".into(),
            source: "fontsource".into(),
            download_urls: vec![
                "https://cdn.jsdelivr.net/fontsource/fonts/inter@latest/latin-400-normal.ttf"
                    .into(),
            ],
            ..Default::default()
        };
        assert!(should_prefer_google_desktop_download(&fs_google));

        let nerd = GoogleFontEntry {
            name: "JetBrainsMono Nerd Font".into(),
            source: "nerdfonts".into(),
            download_urls: vec!["https://github.com/ryanoasis/nerd-fonts/releases/download/v3.3.0/JetBrainsMono.zip".into()],
            ..Default::default()
        };
        assert!(!should_prefer_google_desktop_download(&nerd));
    }
}
