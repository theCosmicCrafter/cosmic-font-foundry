//! Incremental parallel font cache (`fonts-cache.json`).
//! Renders cached families immediately on startup, then re-parses only added or modified files via Rayon.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::UNIX_EPOCH,
};

use super::{
    inventory::{group_families, path_key, FontEntry, FontFamily},
    reader::{self, FileReadResult, FontFace},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct CacheRecord {
    pub mtime: u64,
    pub size: u64,
    pub result: Result<FileReadResult, String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Cache {
    /// Schema version: bumps automatically force a clean parallel re-index.
    #[serde(default)]
    pub version: u32,
    pub files: HashMap<String, CacheRecord>,
    pub families: Vec<FontFamily>,
}

pub const CACHE_VERSION: u32 = 5;

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let secs = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((secs, meta.len()))
}

impl Cache {
    pub fn load(path: &Path) -> Cache {
        let c: Cache = fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if c.version != CACHE_VERSION {
            Cache {
                version: CACHE_VERSION,
                ..Default::default()
            }
        } else {
            c
        }
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, bytes).is_ok() {
                let _ = fs::rename(&tmp, path);
            }
        }
    }

    /// Reads new or changed files in parallel, prunes deleted entries, and rebuilds families.
    /// Returns the number of files parsed from disk.
    pub fn update(&mut self, entries: &[FontEntry]) -> usize {
        self.version = CACHE_VERSION;
        let pending: Vec<(String, &Path, (u64, u64))> = entries
            .iter()
            .filter_map(|e| {
                let s = stamp(&e.path)?;
                let k = path_key(&e.path);
                match self.files.get(&k) {
                    Some(r) if (r.mtime, r.size) == s => None,
                    _ => Some((k, e.path.as_path(), s)),
                }
            })
            .collect();

        let parsed: Vec<(String, CacheRecord)> = pending
            .par_iter()
            .map(|(k, path, (mtime, size))| {
                (
                    k.clone(),
                    CacheRecord {
                        mtime: *mtime,
                        size: *size,
                        result: reader::read_file(path),
                    },
                )
            })
            .collect();

        let count = parsed.len();
        self.files.extend(parsed);

        let alive: HashSet<String> = entries.iter().map(|e| path_key(&e.path)).collect();
        self.files.retain(|k, _| alive.contains(k));

        let files = &self.files;
        self.families = group_families(entries, &|p: &Path| {
            files
                .get(&path_key(p))
                .map(|r| r.result.clone())
                .unwrap_or_else(|| Err("unable to read font file".into()))
        });
        count
    }

    pub fn faces(&self, path: &Path) -> Option<Vec<FontFace>> {
        self.files
            .get(&path_key(path))
            .and_then(|r| r.result.as_ref().ok().map(|res| res.faces.clone()))
    }
}
