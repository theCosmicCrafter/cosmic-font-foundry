# VecGlypher C++/CUDA + Pure-Rust AI Font Generation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use plan-driven-development to implement this plan task-by-task.

**Goal:** Integrate `VecGlypher/VecGlypher-27b-it` (and compatible VLM/SVG autoregressive models) into Cosmic Font Foundry on the `dev` branch using native C++/CUDA inference (`llama.cpp` / Ollama / `vLLM`) paired with Cosmic's pure-Rust geometry, spacing, kerning, and OpenType compilation engine (`cosmic-foundry`).

**Architecture:**
- **C++/CUDA Inference Tier:** Queries local Ollama (`http://127.0.0.1:11434/api/chat` with `hf.co/wahidmounir/VecGlypher-27b-it-Q4_K_M-GGUF`) or OpenAI-compatible C++/CUDA servers (`llama-server`, `vLLM` `/v1/chat/completions`) over persistent HTTP connections with zero bundled ML bloat in the desktop binary.
- **Pure-Rust Protocol, Exemplar Rasterizer & Fault-Tolerant Decoder (`src-tauri/foundry/src/vecglypher.rs`):** Implements the exact `xk-huang/VecGlypher` prompt specification (`SYSTEM_PROMPT`, `Font design requirements: ...`, `Text content: ...`), rasterizes seed glyphs in pure Rust to `192×192` PNG base64 exemplars for few-shot image-to-glyph conditioning, and ports `SVGSimplifier` + `render_normalized_svg` (`transform="translate(tx ty) scale(sx sy)"` baking and truncated `<path>` recovery) into Rust.
- **Automated Foundry Finishing Pipeline:** Feeds decoded contours straight through `remove_overlap` -> `add_extrema` -> `normalize_winding` -> `auto_space_project` -> `auto_kern_project` -> `generate_latin_composites` -> `autofix_project` -> `.ttf`/`.woff`/`.ufo`/Variable `.ttf`.

**Tech Stack:** Rust (`cosmic-foundry`, `kurbo`, `linesweeper`, `image`, `write-fonts`, `reqwest`), C++/CUDA (`Ollama` / `llama.cpp` / `vLLM`), Tauri 2, Svelte 5.

---

### Task 1: Pure-Rust VecGlypher Protocol, Fault-Tolerant SVG Decoder & `192×192` Exemplar Rasterizer

**Files:**
- Create: `src-tauri/foundry/src/vecglypher.rs`
- Modify: `src-tauri/foundry/src/lib.rs`
- Test: `src-tauri/foundry/tests/phase6_vecglypher_tests.rs`

**Steps:**
1. Write unit/integration tests in `tests/phase6_vecglypher_tests.rs` verifying:
   - Exact VecGlypher system/user prompt construction (`format_user_prompt`, `build_ollama_chat_payload`, `build_openai_chat_payload`) for both text-referenced and image-referenced modes.
   - `decode_vecglypher_svg` handling normal `<path d="M... Q... Z"/>`, `<path>` with `transform="translate(tx ty) scale(sx sy)"`, `<think>...</think>` wrappers, and truncated unclosed `<path d="...` outputs.
   - `render_glyph_exemplar_png` and `render_glyph_exemplar_base64` producing valid `192×192` PNG images from `GlyphOutline` contours.
2. Implement `src-tauri/foundry/src/vecglypher.rs` with zero new Cargo dependencies and zero `.unwrap()`/`.expect()`/`panic!()`.
3. Run `cargo test --manifest-path src-tauri/foundry/Cargo.toml phase6_vecglypher_tests`.

---

### Task 2: Vector Studio AI Glyph Generator Drawer (`AiGlyphGeneratorDrawer.svelte`)

**Files:**
- Create: `src/components/foundry/AiGlyphGeneratorDrawer.svelte`

**Steps:**
1. Build `AiGlyphGeneratorDrawer.svelte` supporting backend detection (`foundry_ai_check_backend`), 1-click Ollama GGUF pull command copy (`ollama run hf.co/wahidmounir/VecGlypher-27b-it-Q4_K_M-GGUF`), VecGlypher style prompt presets, few-shot seed glyph picker (up to 8 project glyphs), single-glyph vs batch charset generation (`A-Z`, `a-z`, `0-9`, `Alphanumeric 62`, `Missing Only`), and live `"ai-glyph-progress"` event streaming with cancel support.
2. Verify with `svelte/compiler` (`0` warnings).

---

### Task 3: Tauri Streaming Batch Engine, MCP Tool & Studio Integration

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/agent.rs`
- Modify: `src/views/Forge.svelte`
- Modify: `src/views/AgentHub.svelte`
- Modify: `src/i18n/en.js` & `src/i18n/es.js`

**Steps:**
1. Add `foundry_ai_check_backend`, `foundry_ai_generate_glyph`, `foundry_ai_generate_font_batch`, and `foundry_ai_cancel_batch` to `src-tauri/src/lib.rs` and register in `tauri::generate_handler!`.
2. Add `cosmic_foundry_ai_generate_glyph` (16th MCP tool) in `src-tauri/src/agent.rs` and `src/views/AgentHub.svelte`.
3. Wire `AiGlyphGeneratorDrawer.svelte` and the **AI Generate** toolbar button into `src/views/Forge.svelte` with matched `en`/`es` i18n keys.
4. Run full independent verification (`fmt`, `clippy --all-targets -- -D warnings`, `cargo test` across all 3 crates, `npm run vite:build`, i18n parity, IPC parity, zero panic audit), commit on `dev`, and push to `origin/dev`.
