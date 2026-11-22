# Typography Pipeline & Offline Vibe Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use plan-driven-development to implement this plan task-by-task.

**Goal:** Add a headless art-to-font CLI & batch pipeline, geometric outline deduplication, sandboxed ZIP/folder license triage, and zero-ML natural-language Typographic DNA vibe search to Cosmic Font Foundry.

**Architecture:** Build all core logic in pure Rust (`cosmic-foundry`, MIT-licensed, zero external ML runtimes or new dependencies) using `skrifa`, `kurbo`, and `cosmic-core::sync::extract_zip_fonts`. Expose the capabilities through a standalone `cosmic-foundry` CLI binary, Tauri IPC commands (`src-tauri/src/lib.rs`), MCP Agent Hub tools (`src-tauri/src/agent.rs`), and the Visual Font Finder modal (`src/components/VisualFontFinderModal.svelte`).

**Tech Stack:** Rust (`cosmic-foundry`, `cosmic-core`, `skrifa`, `kurbo`, `write-fonts`, `image`), Tauri 2, Svelte 5, Vitest/Vite.

---

### Task 1: Canonical Outline Hashing & Duplicate Finder (`outline_hash.rs`)

**Files:**
- Create: `src-tauri/foundry/src/outline_hash.rs`
- Modify: `src-tauri/foundry/src/lib.rs`
- Test: `src-tauri/foundry/tests/phase5_pipeline_tests.rs`

**Steps:**
1. Write failing tests in `tests/phase5_pipeline_tests.rs` checking that two TTFs compiled with different family names/metadata but identical geometry yield equal `OutlineFingerprint::digest` values and group cleanly via `find_outline_duplicates`, whereas altered contours produce a distinct digest.
2. Implement `fingerprint_font_bytes`, `fingerprint_font_file`, and `find_outline_duplicates` in `src-tauri/foundry/src/outline_hash.rs` using deterministic 128-bit FNV-1a hashing over UPM-normalized (`1000.0 / upm`) quantized outlines for probe glyphs `A, g, R, S, 0, H, O, e` (with fallback to first 8 mapped glyphs).
3. Run `cargo test --manifest-path src-tauri/foundry/Cargo.toml phase5_pipeline_tests`.

---

### Task 2: Safe Archive Triage & SVG-Folder Batch Assembly (`ingest.rs`)

**Files:**
- Create: `src-tauri/foundry/src/ingest.rs`
- Modify: `src-tauri/foundry/src/lib.rs`
- Test: `src-tauri/foundry/tests/phase5_pipeline_tests.rs`

**Steps:**
1. Write failing tests in `tests/phase5_pipeline_tests.rs` covering `classify_license` (OFL, Apache, MIT, PublicDomain, PersonalOnly, Commercial, Unknown), `import_svg_folder` (`A.svg`, `U+0042.svg` -> valid `.ttf` compilation), and `triage_path` flagging duplicates and unsafe extensions.
2. Implement `classify_license`, `triage_path` (strict whitelist `.ttf/.otf/.woff/.woff2` + RAII temp cleanup + `extract_zip_fonts` + `skrifa` `name` table extraction), and `import_svg_folder` in `src-tauri/foundry/src/ingest.rs`.
3. Run `cargo test --manifest-path src-tauri/foundry/Cargo.toml phase5_pipeline_tests`.

---

### Task 3: Zero-ML Typographic DNA & Vibe Search (`vibe.rs`)

**Files:**
- Create: `src-tauri/foundry/src/vibe.rs`
- Modify: `src-tauri/foundry/src/lib.rs`
- Test: `src-tauri/foundry/tests/phase5_pipeline_tests.rs`

**Steps:**
1. Write failing tests verifying `extract_dna_from_bytes` distinguishes bold/heavy outlines from light/thin outlines and `search_fonts_by_vibe("chunky bold industrial", ...)` ranks the heavy font #1.
2. Implement `TypographicDna` (10 normalized `0.0..=1.0` geometric dimensions: weight, width, contrast, x_height, slant, roundness, roughness, serifness, monospace, openness) and `search_fonts_by_vibe` with natural-language descriptor parsing and weighted cosine/L1 ranking.
3. Run `cargo test --manifest-path src-tauri/foundry/Cargo.toml phase5_pipeline_tests`.

---

### Task 4: Discover Presets Expansion (`indestructible`, `open_foundry`)

**Files:**
- Modify: `src-tauri/src/google.rs`
- Modify: `src/lib/discover.svelte.js`
- Modify: `src/i18n/en.js` & `src/i18n/es.js`

**Steps:**
1. Add `indestructible` (Indestructible Type) and `open_foundry` (Open Foundry & Uncut) presets to `resolve_preset_entries` in `google.rs`, updating `all_thirteen_presets_resolve_non_empty`.
2. Add matching entries in `COMMUNITY_PRESETS`, `webPresetEntries`, and `en.js` / `es.js` i18n dictionaries.
3. Run `cargo test --manifest-path src-tauri/Cargo.toml google::tests`.

---

### Task 5: Headless `cosmic-foundry` CLI Binary, Tauri/MCP Wiring & Vibe Search UI

**Files:**
- Create: `src-tauri/foundry/src/bin/cosmic-foundry.rs`
- Create: `src-tauri/foundry/tests/cli_tests.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/agent.rs`, `src/views/AgentHub.svelte`, `src/components/VisualFontFinderModal.svelte`, `src/i18n/{en,es}.js`

**Steps:**
1. Build pure-std headless CLI `cosmic-foundry` supporting `trace-sheet`, `svg-dir`, `qa`, `autofix`, `style`, `dedup`, `triage`, `vibe`, `--help`, and `--json`, exiting non-zero when `qa` detects errors.
2. Add Tauri commands (`foundry_vibe_search`, `foundry_find_outline_duplicates`, `foundry_triage_archive`), MCP tools (`cosmic_vibe_search_fonts`, `cosmic_find_outline_duplicates` -> 15 total), and the **Vibe Search** tab in `VisualFontFinderModal.svelte`.
3. Run full end-to-end verification (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` across all 3 crates, `npm run vite:build`, i18n parity, IPC parity, zero panic audit).
