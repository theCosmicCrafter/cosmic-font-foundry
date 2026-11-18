//! Scans the local Windows font inventory (read-only) and prints benchmark timings.
//! Run: `cargo run --release --example scan`

use cosmic_fonts_lib::fonts::{self, cache::Cache, Environment};
use std::time::Instant;

fn main() {
    let tmp = std::env::temp_dir().join("cosmic-font-foundry-scan");
    let env = Environment {
        touch_windows: false,
        ..Environment::real(tmp)
    };
    let t = Instant::now();
    let entries = fonts::collect_entries(&env, &[]);
    let t_list = t.elapsed();

    let mut cache = Cache::default();
    let parsed = cache.update(&entries);
    let t_parse = t.elapsed();

    let mut cache2 = Cache {
        version: cache.version,
        files: cache.files.clone(),
        families: vec![],
    };
    let t2 = Instant::now();
    cache2.update(&fonts::collect_entries(&env, &[]));
    println!(
        "{} files ({t_list:?} list), {parsed} parsed in {t_parse:?}; warm pass {:?}",
        entries.len(),
        t2.elapsed()
    );
    println!(
        "{} families, {} read errors",
        cache.families.len(),
        cache.families.iter().filter(|f| f.error.is_some()).count()
    );
    let mut counts = std::collections::BTreeMap::new();
    for fam in &cache.families {
        *counts.entry(fam.category.clone()).or_insert(0) += 1;
    }
    println!("{counts:?}");
    for cat in ["sans", "serif", "script", "display", "mono", "other"] {
        let sample: Vec<_> = cache
            .families
            .iter()
            .filter(|f| f.category == cat)
            .take(14)
            .map(|f| f.name.as_str())
            .collect();
        println!("{cat}: {}", sample.join(", "));
    }
    for fam in cache.families.iter().filter(|f| f.error.is_some()).take(5) {
        println!("error: {} {:?}", fam.name, fam.error);
    }
}
