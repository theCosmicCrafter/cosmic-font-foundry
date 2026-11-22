use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cosmic_foundry::compiler::{compile_ttf, import_ufo};
use cosmic_foundry::import::fork_font_file;
use cosmic_foundry::ingest::{import_svg_folder, triage_path, LicenseClass};
use cosmic_foundry::model::{compute_bbox, FontMetrics, FoundryProject, GlyphOutline};
use cosmic_foundry::outline_hash::find_outline_duplicates;
use cosmic_foundry::qa::{audit_project, autofix_project, QaReport, QaSeverity};
use cosmic_foundry::styles::{generate_style_variant, StyleVariantKind};
use cosmic_foundry::trace::{trace_glyph_sheet, TraceOptions};
use cosmic_foundry::typography::auto_space_project;
use cosmic_foundry::vibe::search_fonts_by_vibe;
use serde_json::json;

const MAX_DIR_WALK_DEPTH: usize = 16;

fn print_global_help() {
    println!(
        "\
cosmic-foundry — headless Rust font foundry CLI

USAGE:
  cosmic-foundry <COMMAND> [OPTIONS]
  cosmic-foundry --help | -h

GLOBAL FLAGS:
  -h, --help    Show help (works globally or after a subcommand)
      --json    Emit structured JSON output

SUBCOMMANDS:
  trace-sheet <image_path> --cols <N> --rows <N> --chars <STR> --out <OUT.ttf> [--family <NAME>] [--threshold <0-255>]
      Trace a grid sprite sheet into glyphs, auto-space, and compile to TTF.

  svg-dir <dir> --out <OUT.ttf> [--family <NAME>] [--upm <1000>]
      Assemble a folder of SVG glyph files and compile to TTF.

  qa <font_or_ufo_path> [--json]
      Audit a .ttf/.otf font or UFO directory. Exits with code 2 if errors > 0.

  autofix <font_or_ufo_path> --out <OUT.ttf> [--json]
      Repair fixable QA issues and compile a clean TTF.

  style <font_or_ufo_path> --kind <bold|light|condensed|extended|small_caps|oblique|inline> [--amount <F64>] --out <OUT.ttf> [--json]
      Generate a parametric style variant and compile to TTF.

  dedup <dir_or_files...> [--json]
      Detect canonical outline-identical font duplicates across files/directories.

  triage <zip_or_dir_or_file> [--json]
      Scan a zip archive, directory, or font file for licenses, unsafe files, and duplicates.

  vibe <query> --dir <FONT_DIR> [--top <N>] [--json]
      Rank fonts in a directory against a natural-language typographic vibe query."
    );
}

fn print_subcommand_help(cmd: &str) -> bool {
    let text = match cmd {
        "trace-sheet" => {
            "\
USAGE:
  cosmic-foundry trace-sheet <image_path> --cols <N> --rows <N> --chars <STR> --out <OUT.ttf> [--family <NAME>] [--threshold <0-255>] [--json]"
        }
        "svg-dir" => {
            "\
USAGE:
  cosmic-foundry svg-dir <dir> --out <OUT.ttf> [--family <NAME>] [--upm <1000>] [--json]"
        }
        "qa" => {
            "\
USAGE:
  cosmic-foundry qa <font_or_ufo_path> [--json]

Exits with code 2 when report.errors > 0."
        }
        "autofix" => {
            "\
USAGE:
  cosmic-foundry autofix <font_or_ufo_path> --out <OUT.ttf> [--json]"
        }
        "style" => {
            "\
USAGE:
  cosmic-foundry style <font_or_ufo_path> --kind <bold|light|condensed|extended|small_caps|oblique|inline> [--amount <F64>] --out <OUT.ttf> [--json]"
        }
        "dedup" => {
            "\
USAGE:
  cosmic-foundry dedup <dir_or_files...> [--json]"
        }
        "triage" => {
            "\
USAGE:
  cosmic-foundry triage <zip_or_dir_or_file> [--json]"
        }
        "vibe" => {
            "\
USAGE:
  cosmic-foundry vibe <query> --dir <FONT_DIR> [--top <N>] [--json]"
        }
        _ => return false,
    };
    println!("{text}");
    true
}

fn next_flag_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    *idx += 1;
    args.get(*idx)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn write_output_bytes(out_path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(out_path, bytes).map_err(|e| format!("failed to write '{}': {e}", out_path.display()))
}

fn load_project(path: &Path, default_family: Option<&str>) -> Result<FoundryProject, String> {
    if !path.exists() {
        return Err(format!("path does not exist: '{}'", path.display()));
    }
    if path.is_dir() {
        import_ufo(path)
    } else {
        fork_font_file(path, default_family, 256)
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
}

fn collect_font_files_recursive(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > MAX_DIR_WALK_DEPTH {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("failed to read directory '{}': {e}", dir.display()))?;
    for entry_res in entries {
        let Ok(entry) = entry_res else {
            continue;
        };
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            collect_font_files_recursive(&path, depth + 1, out)?;
        } else if ft.is_file() && is_font_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_font_files_from_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for input in inputs {
        let meta = fs::symlink_metadata(input)
            .map_err(|e| format!("failed to inspect '{}': {e}", input.display()))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_font_files_recursive(input, 0, &mut paths)?;
        } else if meta.is_file() && is_font_file(input) {
            paths.push(input.clone());
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn format_qa_table(report: &QaReport) -> String {
    let mut lines = vec![format!(
        "QA Score: {}/100 | Errors: {} | Warnings: {} | Total Issues: {}",
        report.score,
        report.errors,
        report.warnings,
        report.issues.len()
    )];
    if !report.issues.is_empty() {
        lines.push(format!(
            "{:<8} {:<28} {:<10} {}",
            "SEVERITY", "CODE", "GLYPH", "MESSAGE"
        ));
        lines.push("-".repeat(78));
        for issue in &report.issues {
            let sev = match issue.severity {
                QaSeverity::Error => "ERROR",
                QaSeverity::Warn => "WARN",
                QaSeverity::Info => "INFO",
            };
            let glyph = issue.glyph_key.as_deref().unwrap_or("-");
            lines.push(format!(
                "{:<8} {:<28} {:<10} {}",
                sev, issue.code, glyph, issue.message
            ));
        }
    }
    lines.join("\n")
}

fn parse_style_kind(raw: &str) -> Result<StyleVariantKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bold" => Ok(StyleVariantKind::Bold),
        "light" => Ok(StyleVariantKind::Light),
        "condensed" => Ok(StyleVariantKind::Condensed),
        "extended" => Ok(StyleVariantKind::Extended),
        "small_caps" | "small-caps" | "smallcaps" => Ok(StyleVariantKind::SmallCaps),
        "oblique" => Ok(StyleVariantKind::Oblique),
        "inline" => Ok(StyleVariantKind::Inline),
        other => Err(format!(
            "unknown style kind '{other}' (expected bold|light|condensed|extended|small_caps|oblique|inline)"
        )),
    }
}

fn license_label(class: &LicenseClass) -> &'static str {
    match class {
        LicenseClass::Ofl => "ofl",
        LicenseClass::Apache => "apache",
        LicenseClass::Mit => "mit",
        LicenseClass::PublicDomain => "public_domain",
        LicenseClass::PersonalOnly => "personal_only",
        LicenseClass::Commercial => "commercial",
        LicenseClass::Unknown => "unknown",
    }
}

fn cmd_trace_sheet(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut image_path: Option<PathBuf> = None;
    let mut cols: Option<usize> = None;
    let mut rows: Option<usize> = None;
    let mut chars: Option<String> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut family = "Traced Font".to_string();
    let mut threshold: Option<u8> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cols" => {
                let val = next_flag_value(args, &mut i, "--cols")?;
                cols = Some(
                    val.parse::<usize>()
                        .map_err(|_| format!("invalid --cols value '{val}'"))?,
                );
            }
            "--rows" => {
                let val = next_flag_value(args, &mut i, "--rows")?;
                rows = Some(
                    val.parse::<usize>()
                        .map_err(|_| format!("invalid --rows value '{val}'"))?,
                );
            }
            "--chars" => {
                chars = Some(next_flag_value(args, &mut i, "--chars")?);
            }
            "--out" => {
                out_path = Some(PathBuf::from(next_flag_value(args, &mut i, "--out")?));
            }
            "--family" => {
                family = next_flag_value(args, &mut i, "--family")?;
            }
            "--threshold" => {
                let val = next_flag_value(args, &mut i, "--threshold")?;
                threshold = Some(
                    val.parse::<u8>()
                        .map_err(|_| format!("invalid --threshold (0-255) '{val}'"))?,
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for trace-sheet"));
            }
            pos => {
                if image_path.is_some() {
                    return Err(format!("unexpected positional argument '{pos}'"));
                }
                image_path = Some(PathBuf::from(pos));
            }
        }
        i += 1;
    }

    let image_path = image_path.ok_or("missing required <image_path>")?;
    let cols = cols.ok_or("missing required --cols <N>")?;
    let rows = rows.ok_or("missing required --rows <N>")?;
    let chars = chars.ok_or("missing required --chars <STR>")?;
    let out_path = out_path.ok_or("missing required --out <OUT.ttf>")?;

    let mut opts = TraceOptions::default();
    if let Some(t) = threshold {
        opts.threshold = t;
    }

    let bytes = fs::read(&image_path)
        .map_err(|e| format!("failed to read '{}': {e}", image_path.display()))?;
    let traced = trace_glyph_sheet(&bytes, cols, rows, &chars, &opts)?;
    if traced.is_empty() {
        return Err("no glyphs were traced from the sprite sheet".to_string());
    }

    let metrics = FontMetrics {
        upm: opts.upm,
        ascender: ((opts.upm as f64) * 0.80).round() as i16,
        descender: -(((opts.upm as f64) * 0.20).round() as i16),
        x_height: ((opts.upm as f64) * 0.52).round() as i16,
        cap_height: ((opts.upm as f64) * 0.72).round() as i16,
        line_gap: 0,
    };

    let mut glyphs = BTreeMap::new();
    for (key, contours) in traced {
        let Some(ch) = key.chars().next() else {
            continue;
        };
        let cp = ch as u32;
        let glyph_key = if cp == 0x20 { "space".to_string() } else { key };
        let glyph_name = if cp == 0x20 {
            "space".to_string()
        } else if ch.is_ascii_alphanumeric() {
            ch.to_string()
        } else {
            format!("uni{cp:04X}")
        };
        let bbox = compute_bbox(&contours);
        let advance_width = ((bbox[2] - bbox[0]).max(0.0) + (opts.upm as f64) * 0.12)
            .max((opts.upm as f64) * 0.25)
            .round() as i32;
        let lsb = bbox[0].round() as i32;
        glyphs.insert(
            glyph_key,
            GlyphOutline {
                codepoint: cp,
                name: glyph_name,
                advance_width,
                lsb,
                contours,
            },
        );
    }

    if !glyphs.values().any(|g| g.codepoint == 0x20) {
        glyphs.insert(
            "space".to_string(),
            GlyphOutline {
                codepoint: 0x20,
                name: "space".to_string(),
                advance_width: (metrics.upm as i32 / 3).max(200),
                lsb: 0,
                contours: Vec::new(),
            },
        );
    }

    let final_family = if family.trim().is_empty() {
        "Traced Font".to_string()
    } else {
        family.trim().to_string()
    };

    let mut project = FoundryProject {
        version: 3,
        family_name: final_family.clone(),
        style_name: "Regular".to_string(),
        designer: String::new(),
        license: String::new(),
        metrics,
        glyphs,
        kerning: Vec::new(),
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    };

    let sidebearing = (project.metrics.upm as f64) * 0.06;
    let spaced = auto_space_project(&mut project, sidebearing);
    let compiled = compile_ttf(&project, true)?;
    write_output_bytes(&out_path, &compiled.ttf_bytes)?;

    if json_mode {
        let payload = json!({
            "family": final_family,
            "out": out_path.display().to_string(),
            "glyph_count": compiled.glyph_count,
            "spaced_glyphs": spaced,
            "total_contours": compiled.total_contours,
            "total_points": compiled.total_points,
            "bytes": compiled.ttf_bytes.len(),
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize JSON: {e}"))?;
        println!("{out}");
    } else {
        println!(
            "Traced {} glyph(s) (spaced {}) -> {} ({} bytes)",
            compiled.glyph_count,
            spaced,
            out_path.display(),
            compiled.ttf_bytes.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_svg_dir(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut dir: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut family = "Imported SVG Font".to_string();
    let mut upm: u16 = 1000;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                out_path = Some(PathBuf::from(next_flag_value(args, &mut i, "--out")?));
            }
            "--family" => {
                family = next_flag_value(args, &mut i, "--family")?;
            }
            "--upm" => {
                let val = next_flag_value(args, &mut i, "--upm")?;
                upm = val
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --upm value '{val}'"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for svg-dir"));
            }
            pos => {
                if dir.is_some() {
                    return Err(format!("unexpected positional argument '{pos}'"));
                }
                dir = Some(PathBuf::from(pos));
            }
        }
        i += 1;
    }

    let dir = dir.ok_or("missing required <dir>")?;
    let out_path = out_path.ok_or("missing required --out <OUT.ttf>")?;

    let project = import_svg_folder(&dir, &family, upm)?;
    let compiled = compile_ttf(&project, true)?;
    write_output_bytes(&out_path, &compiled.ttf_bytes)?;

    if json_mode {
        let payload = json!({
            "family": project.family_name,
            "upm": project.metrics.upm,
            "out": out_path.display().to_string(),
            "glyph_count": compiled.glyph_count,
            "total_contours": compiled.total_contours,
            "total_points": compiled.total_points,
            "bytes": compiled.ttf_bytes.len(),
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize JSON: {e}"))?;
        println!("{out}");
    } else {
        println!(
            "Compiled {} glyph(s) from '{}' -> {} ({} bytes)",
            compiled.glyph_count,
            dir.display(),
            out_path.display(),
            compiled.ttf_bytes.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_qa(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut target: Option<PathBuf> = None;

    for arg in args {
        if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for qa"));
        }
        if target.is_some() {
            return Err(format!("unexpected positional argument '{arg}'"));
        }
        target = Some(PathBuf::from(arg));
    }

    let target = target.ok_or("missing required <font_or_ufo_path>")?;
    let project = load_project(&target, Some("Audit"))?;
    let report = audit_project(&project);

    if json_mode {
        let out = serde_json::to_string_pretty(&report)
            .map_err(|e| format!("failed to serialize QA report: {e}"))?;
        println!("{out}");
    } else {
        println!("{}", format_qa_table(&report));
    }

    if report.errors > 0 {
        Ok(ExitCode::from(2))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn cmd_autofix(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut target: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                out_path = Some(PathBuf::from(next_flag_value(args, &mut i, "--out")?));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for autofix"));
            }
            pos => {
                if target.is_some() {
                    return Err(format!("unexpected positional argument '{pos}'"));
                }
                target = Some(PathBuf::from(pos));
            }
        }
        i += 1;
    }

    let target = target.ok_or("missing required <font_or_ufo_path>")?;
    let out_path = out_path.ok_or("missing required --out <OUT.ttf>")?;

    let mut project = load_project(&target, None)?;
    let (fixed_count, report) = autofix_project(&mut project);
    let compiled = compile_ttf(&project, true)?;
    write_output_bytes(&out_path, &compiled.ttf_bytes)?;

    if json_mode {
        let payload = json!({
            "fixed_count": fixed_count,
            "out": out_path.display().to_string(),
            "glyph_count": compiled.glyph_count,
            "bytes": compiled.ttf_bytes.len(),
            "report": report,
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize autofix report: {e}"))?;
        println!("{out}");
    } else {
        println!(
            "Applied {fixed_count} fix(es) -> {} ({} bytes)",
            out_path.display(),
            compiled.ttf_bytes.len()
        );
        println!("{}", format_qa_table(&report));
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_style(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut target: Option<PathBuf> = None;
    let mut kind_raw: Option<String> = None;
    let mut amount: f64 = 0.0;
    let mut out_path: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                kind_raw = Some(next_flag_value(args, &mut i, "--kind")?);
            }
            "--amount" => {
                let val = next_flag_value(args, &mut i, "--amount")?;
                amount = val
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --amount value '{val}'"))?;
            }
            "--out" => {
                out_path = Some(PathBuf::from(next_flag_value(args, &mut i, "--out")?));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for style"));
            }
            pos => {
                if target.is_some() {
                    return Err(format!("unexpected positional argument '{pos}'"));
                }
                target = Some(PathBuf::from(pos));
            }
        }
        i += 1;
    }

    let target = target.ok_or("missing required <font_or_ufo_path>")?;
    let kind_raw = kind_raw.ok_or("missing required --kind <KIND>")?;
    let kind = parse_style_kind(&kind_raw)?;
    let out_path = out_path.ok_or("missing required --out <OUT.ttf>")?;

    let project = load_project(&target, None)?;
    let styled = generate_style_variant(&project, kind, amount);
    let compiled = compile_ttf(&styled, true)?;
    write_output_bytes(&out_path, &compiled.ttf_bytes)?;

    if json_mode {
        let payload = json!({
            "family": styled.family_name,
            "style": styled.style_name,
            "amount": amount,
            "out": out_path.display().to_string(),
            "glyph_count": compiled.glyph_count,
            "bytes": compiled.ttf_bytes.len(),
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize style result: {e}"))?;
        println!("{out}");
    } else {
        println!(
            "Generated {} {} ({} glyph(s)) -> {} ({} bytes)",
            styled.family_name,
            styled.style_name,
            compiled.glyph_count,
            out_path.display(),
            compiled.ttf_bytes.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_dedup(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut inputs = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for dedup"));
        }
        inputs.push(PathBuf::from(arg));
    }

    if inputs.is_empty() {
        return Err("missing required <dir_or_files...>".to_string());
    }

    let font_paths = collect_font_files_from_inputs(&inputs)?;
    let groups = find_outline_duplicates(&font_paths);

    if json_mode {
        let payload = json!({
            "scanned_files": font_paths.len(),
            "group_count": groups.len(),
            "groups": groups,
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize dedup report: {e}"))?;
        println!("{out}");
    } else if groups.is_empty() {
        println!(
            "Scanned {} font file(s): no outline duplicates found.",
            font_paths.len()
        );
    } else {
        println!(
            "Scanned {} font file(s): found {} duplicate group(s):",
            font_paths.len(),
            groups.len()
        );
        for (idx, group) in groups.iter().enumerate() {
            println!(
                "  Group {} [digest={}, glyphs_hashed={}]:",
                idx + 1,
                group.digest,
                group.glyphs_hashed
            );
            for file in &group.files {
                println!("    - {} ({})", file.path, file.family_hint);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_triage(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut target: Option<PathBuf> = None;
    for arg in args {
        if arg.starts_with('-') {
            return Err(format!("unknown option '{arg}' for triage"));
        }
        if target.is_some() {
            return Err(format!("unexpected positional argument '{arg}'"));
        }
        target = Some(PathBuf::from(arg));
    }

    let target = target.ok_or("missing required <zip_or_dir_or_file>")?;
    let report = triage_path(&target)?;

    let mut breakdown: BTreeMap<&'static str, usize> = BTreeMap::new();
    for font in &report.fonts {
        let label = license_label(&font.license_class);
        *breakdown.entry(label).or_insert(0) += 1;
    }

    if json_mode {
        let payload = json!({
            "scanned_files": report.scanned_files,
            "rejected_unsafe": report.rejected_unsafe,
            "duplicate_count": report.duplicate_count,
            "license_breakdown": breakdown,
            "fonts": report.fonts,
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize triage report: {e}"))?;
        println!("{out}");
    } else {
        println!(
            "Triage Summary: scanned={} fonts={} duplicates={} rejected_unsafe={}",
            report.scanned_files,
            report.fonts.len(),
            report.duplicate_count,
            report.rejected_unsafe
        );
        if !breakdown.is_empty() {
            println!("License Breakdown:");
            for (lic, count) in &breakdown {
                println!("  {lic:<16} {count}");
            }
        }
        for font in &report.fonts {
            let dup_tag = if font.is_duplicate { " [DUP]" } else { "" };
            println!(
                "  - {} | {} {} | {}{}",
                font.source_entry,
                font.family,
                font.subfamily,
                license_label(&font.license_class),
                dup_tag
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_vibe(args: &[String], json_mode: bool) -> Result<ExitCode, String> {
    let mut query_parts = Vec::new();
    let mut dir: Option<PathBuf> = None;
    let mut top_k: usize = 10;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                dir = Some(PathBuf::from(next_flag_value(args, &mut i, "--dir")?));
            }
            "--top" => {
                let val = next_flag_value(args, &mut i, "--top")?;
                top_k = val
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top value '{val}'"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for vibe"));
            }
            pos => {
                query_parts.push(pos.to_string());
            }
        }
        i += 1;
    }

    if query_parts.is_empty() {
        return Err("missing required <query>".to_string());
    }
    let query = query_parts.join(" ");
    let dir = dir.ok_or("missing required --dir <FONT_DIR>")?;
    if !dir.is_dir() {
        return Err(format!("not a directory: '{}'", dir.display()));
    }

    let mut font_paths = Vec::new();
    collect_font_files_recursive(&dir, 0, &mut font_paths)?;
    font_paths.sort();

    let candidates: Vec<(String, String, String)> = font_paths
        .iter()
        .map(|p| {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("font")
                .to_string();
            let category_hint = p
                .strip_prefix(&dir)
                .ok()
                .and_then(|rel| rel.parent())
                .and_then(|par| par.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            (stem, p.to_string_lossy().into_owned(), category_hint)
        })
        .collect();

    let hits = search_fonts_by_vibe(&query, &candidates, top_k);

    if json_mode {
        let payload = json!({
            "query": query,
            "scanned_files": candidates.len(),
            "hits": hits,
        });
        let out = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("failed to serialize vibe hits: {e}"))?;
        println!("{out}");
    } else if hits.is_empty() {
        println!(
            "No matching fonts found in '{}' for query \"{query}\".",
            dir.display()
        );
    } else {
        println!(
            "Vibe search \"{query}\" ({} candidate(s)):",
            candidates.len()
        );
        for (rank, hit) in hits.iter().enumerate() {
            let traits = if hit.matched_traits.is_empty() {
                "-".to_string()
            } else {
                hit.matched_traits.join(", ")
            };
            println!(
                "  #{:<2} {:<24} score={:5.1} | w={:.2} width={:.2} contrast={:.2} slant={:.2} serif={:.2} mono={:.2} | traits: {}",
                rank + 1,
                hit.family,
                hit.score,
                hit.dna.weight,
                hit.dna.width,
                hit.dna.contrast,
                hit.dna.slant,
                hit.dna.serifness,
                hit.dna.monospace,
                traits
            );
            println!("      path: {}", hit.path);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn run(raw_args: Vec<String>) -> Result<ExitCode, String> {
    let mut json_mode = false;
    let mut help_requested = false;
    let mut filtered = Vec::with_capacity(raw_args.len());

    for arg in raw_args {
        match arg.as_str() {
            "-h" | "--help" => help_requested = true,
            "--json" => json_mode = true,
            _ => filtered.push(arg),
        }
    }

    if filtered.is_empty() {
        print_global_help();
        return if help_requested {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::from(1))
        };
    }

    let cmd = filtered[0].as_str();
    let rest = &filtered[1..];

    if help_requested {
        if !print_subcommand_help(cmd) {
            print_global_help();
        }
        return Ok(ExitCode::SUCCESS);
    }

    match cmd {
        "trace-sheet" => cmd_trace_sheet(rest, json_mode),
        "svg-dir" => cmd_svg_dir(rest, json_mode),
        "qa" => cmd_qa(rest, json_mode),
        "autofix" => cmd_autofix(rest, json_mode),
        "style" => cmd_style(rest, json_mode),
        "dedup" => cmd_dedup(rest, json_mode),
        "triage" => cmd_triage(rest, json_mode),
        "vibe" => cmd_vibe(rest, json_mode),
        "help" => {
            if let Some(sub) = rest.first() {
                if !print_subcommand_help(sub) {
                    print_global_help();
                }
            } else {
                print_global_help();
            }
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!(
            "unknown subcommand '{other}'. Run `cosmic-foundry --help` for usage."
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}
