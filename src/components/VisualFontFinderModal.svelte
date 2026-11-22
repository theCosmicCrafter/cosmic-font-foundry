<script>
  import { onDestroy } from "svelte";
  import {
    ScanSearch,
    Upload,
    Image,
    FileCode,
    Sparkles,
    Search,
    Eye,
    GitBranch,
    X,
  } from "@lucide/svelte";
  import { invokeCmd, fontStyle } from "../lib/fonts.svelte.js";
  import { inTauri } from "../lib/settings.svelte.js";

  let {
    open = false,
    fonts = [],
    onClose = null,
    onSelectFamily = null,
    onForkInFoundry = null,
  } = $props();

  const VIBE_PRESETS = [
    "chunky geometric sans",
    "editorial high contrast serif",
    "condensed brutalist stencil",
    "distressed rough horror",
    "clean coding monospace",
    "slanted calligraphic script",
  ];

  let mode = $state("image");
  let sampleChars = $state("AHMNOORSTaeon");
  let vibeQuery = $state("");
  let imageBytes = $state(null);
  let imagePreviewUrl = $state("");
  let imageMeta = $state(null);
  let dragActive = $state(false);
  let svgText = $state("");
  let svgFileName = $state("");
  let searching = $state(false);
  let error = $state("");
  let results = $state([]);
  let hasSearched = $state(false);

  let imageInputEl = $state(null);
  let svgInputEl = $state(null);

  const canSearch = $derived(
    inTauri &&
      !searching &&
      (mode === "image"
        ? Boolean(imageBytes?.length)
        : mode === "svg"
          ? svgText.trim().length > 0
          : vibeQuery.trim().length > 0),
  );

  function revokePreview() {
    if (imagePreviewUrl) {
      URL.revokeObjectURL(imagePreviewUrl);
      imagePreviewUrl = "";
    }
  }

  onDestroy(() => {
    revokePreview();
  });

  function closeModal() {
    onClose?.();
  }

  function handleKeydown(e) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      closeModal();
    }
  }

  function formatBytes(bytes) {
    if (!bytes) return "0 B";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  async function loadImageBlob(blob, label = "image") {
    const allowedExt = /\.(png|jpe?g|webp)$/i;
    const isImageType =
      blob.type === "image/png" ||
      blob.type === "image/jpeg" ||
      blob.type === "image/webp" ||
      blob.type.startsWith("image/");
    if (!isImageType && !allowedExt.test(label)) {
      error = "Unsupported file type. Please choose a PNG, JPG, or WebP image.";
      return;
    }

    const buf = await blob.arrayBuffer();
    const bytes = new Uint8Array(buf);
    if (!bytes.length) {
      error = "The selected image is empty.";
      return;
    }

    revokePreview();
    imageBytes = bytes;
    imagePreviewUrl = URL.createObjectURL(blob);
    imageMeta = {
      name: label || "Clipboard image",
      size: bytes.byteLength,
      type: blob.type || "image",
    };
    error = "";
  }

  function clearImage() {
    revokePreview();
    imageBytes = null;
    imageMeta = null;
    error = "";
  }

  function handlePasteEvent(e) {
    if (!open || mode !== "image") return;
    const items = e.clipboardData?.items;
    if (!items?.length) return;
    for (const item of items) {
      if (item.type?.startsWith("image/")) {
        const blob = item.getAsFile();
        if (blob) {
          e.preventDefault();
          loadImageBlob(blob, blob.name || "Clipboard screenshot");
          return;
        }
      }
    }
  }

  function handleDragOver(e) {
    e.preventDefault();
    dragActive = true;
  }

  function handleDragLeave(e) {
    if (e.currentTarget.contains(e.relatedTarget)) return;
    dragActive = false;
  }

  function handleDrop(e) {
    e.preventDefault();
    dragActive = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) loadImageBlob(file, file.name);
  }

  function handleImageInputChange(e) {
    const file = e.currentTarget.files?.[0];
    if (file) loadImageBlob(file, file.name);
    e.currentTarget.value = "";
  }

  async function handleSvgInputChange(e) {
    const file = e.currentTarget.files?.[0];
    if (!file) return;
    try {
      svgText = await file.text();
      svgFileName = file.name;
      error = "";
    } catch (err) {
      error = `Could not read SVG file: ${String(err)}`;
    } finally {
      e.currentTarget.value = "";
    }
  }

  async function findMatchingFonts() {
    if (!inTauri || searching) return;
    const chars = sampleChars.trim() || "AHMNOORSTaeon";

    if (mode === "image" && !imageBytes?.length) {
      error = "Upload, drop, or paste an image first.";
      return;
    }
    if (mode === "svg" && !svgText.trim()) {
      error = "Paste an SVG path or document, or load an .svg file first.";
      return;
    }
    if (mode === "vibe" && !vibeQuery.trim()) {
      error = "Describe the font style or vibe you want to search for.";
      return;
    }

    searching = true;
    error = "";
    hasSearched = false;
    try {
      const hits =
        mode === "image"
          ? await invokeCmd("library_visual_match_image", {
              bytes: Array.from(imageBytes),
              sampleChars: chars,
              topK: 8,
            })
          : mode === "svg"
            ? await invokeCmd("library_visual_match_svg", {
                svgText: svgText.trim(),
                sampleChars: chars,
                topK: 8,
              })
            : await invokeCmd("foundry_vibe_search", {
                query: vibeQuery.trim(),
                topK: 10,
              });
      results = Array.isArray(hits) ? hits : [];
      hasSearched = true;
    } catch (err) {
      error = String(err);
      results = [];
    } finally {
      searching = false;
    }
  }

  function resolvePreviewStyle(hit) {
    const targetName = hit.family?.toLowerCase();
    const matched = fonts.find(
      (f) =>
        f.name === hit.family ||
        (targetName && f.name?.toLowerCase() === targetName) ||
        (hit.path && f.styles?.some((s) => s.file === hit.path)),
    );
    if (matched) return fontStyle(matched);
    const safeFamily = String(hit.family ?? "").replace(/["\\]/g, "");
    return {
      family: safeFamily ? `"${safeFamily}", var(--sans)` : "var(--sans)",
      weight: 400,
      italic: false,
    };
  }

  function inspectHit(hit) {
    onSelectFamily?.(hit.family);
    closeModal();
  }

  function forkHit(hit) {
    onForkInFoundry?.({ name: hit.family, path: hit.path });
    closeModal();
  }
</script>

<svelte:window onkeydowncapture={handleKeydown} onpaste={handlePasteEvent} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="veil" onclick={closeModal}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Visual Font Finder"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="head">
        <div class="title-wrap">
          <span class="title-ico"><ScanSearch size={17} strokeWidth={2.1} /></span>
          <div>
            <h2>Offline Visual Font Finder</h2>
            <p class="sub">Match a screenshot, glyph photo, or SVG outline against your local library</p>
          </div>
        </div>
        <button type="button" class="close" title="Close" aria-label="Close" onclick={closeModal}>
          <X size={14} strokeWidth={2} />
        </button>
      </div>

      <div class="scroll">
        {#if !inTauri}
          <div class="notice">
            <Sparkles size={14} strokeWidth={2} />
            <span>
              Offline visual font matching runs locally via the Cosmic Font Foundry desktop engine and is unavailable in web preview mode.
            </span>
          </div>
        {/if}

        <div class="mode-bar" role="tablist" aria-label="Visual input mode">
          <button
            type="button"
            role="tab"
            class="mode-tab"
            class:active={mode === "image"}
            aria-selected={mode === "image"}
            onclick={() => {
              mode = "image";
              error = "";
            }}
          >
            <Image size={14} strokeWidth={2} />
            <span>Image / Screenshot</span>
          </button>
          <button
            type="button"
            role="tab"
            class="mode-tab"
            class:active={mode === "svg"}
            aria-selected={mode === "svg"}
            onclick={() => {
              mode = "svg";
              error = "";
            }}
          >
            <FileCode size={14} strokeWidth={2} />
            <span>SVG Outline</span>
          </button>
          <button
            type="button"
            role="tab"
            class="mode-tab"
            class:active={mode === "vibe"}
            aria-selected={mode === "vibe"}
            onclick={() => {
              mode = "vibe";
              error = "";
            }}
          >
            <Sparkles size={14} strokeWidth={2} />
            <span>Vibe Search</span>
          </button>
        </div>

        {#if mode === "image"}
          <section class="input-card">
            <input
              bind:this={imageInputEl}
              type="file"
              accept="image/png,image/jpeg,image/webp,image/*"
              class="sr-input"
              onchange={handleImageInputChange}
            />

            {#if imagePreviewUrl && imageMeta}
              <div class="preview-box">
                <img class="thumb" src={imagePreviewUrl} alt="Uploaded glyph specimen" />
                <div class="preview-meta">
                  <div class="meta-name">{imageMeta.name}</div>
                  <div class="meta-sub num">
                    {formatBytes(imageMeta.size)} · {imageMeta.type}
                  </div>
                  <div class="preview-actions">
                    <button type="button" class="btn sm" onclick={() => imageInputEl?.click()}>
                      <Upload size={12} />
                      <span>Replace Image</span>
                    </button>
                    <button type="button" class="btn sm ghost" onclick={clearImage}>
                      Clear
                    </button>
                  </div>
                </div>
              </div>
            {:else}
              <div
                class="dropzone"
                class:dragging={dragActive}
                role="region"
                aria-label="Image drop zone"
                ondragover={handleDragOver}
                ondragleave={handleDragLeave}
                ondrop={handleDrop}
              >
                <div class="dz-icon"><Upload size={20} strokeWidth={1.8} /></div>
                <div class="dz-title">Drop a PNG, JPG, or WebP screenshot here</div>
                <div class="dz-sub">Or press <kbd>Ctrl+V</kbd> anywhere to paste an image from your clipboard</div>
                <button type="button" class="btn" onclick={() => imageInputEl?.click()}>
                  <Image size={13} />
                  <span>Choose Image File</span>
                </button>
              </div>
            {/if}
          </section>
        {:else if mode === "svg"}
          <section class="input-card">
            <input
              bind:this={svgInputEl}
              type="file"
              accept=".svg,image/svg+xml"
              class="sr-input"
              onchange={handleSvgInputChange}
            />
            <div class="svg-head">
              <label class="field-label" for="vff-svg-input">
                SVG <code>&lt;path d="..."&gt;</code> or full <code>&lt;svg&gt;</code> markup
              </label>
              <div class="svg-tools">
                {#if svgFileName}
                  <span class="file-chip num">{svgFileName}</span>
                {/if}
                <button type="button" class="btn sm" onclick={() => svgInputEl?.click()}>
                  <Upload size={12} />
                  <span>Load .svg</span>
                </button>
                {#if svgText}
                  <button
                    type="button"
                    class="btn sm ghost"
                    onclick={() => {
                      svgText = "";
                      svgFileName = "";
                    }}
                  >
                    Clear
                  </button>
                {/if}
              </div>
            </div>
            <textarea
              id="vff-svg-input"
              class="svg-area"
              rows="6"
              bind:value={svgText}
              placeholder={'Paste <path d="M120 700 L350 40 ..." /> or full <svg>...</svg>'}
              spellcheck="false"
            ></textarea>
          </section>
        {:else}
          <section class="input-card">
            <label class="field-label" for="vff-vibe-input">
              Describe typographic style or mood (offline Typographic DNA matching)
            </label>
            <input
              id="vff-vibe-input"
              type="text"
              class="text-input"
              bind:value={vibeQuery}
              placeholder="e.g. chunky geometric sans, distressed woodcut serif, condensed stencil…"
              onkeydown={(e) => e.key === "Enter" && canSearch && findMatchingFonts()}
            />
            <div class="vibe-chips">
              {#each VIBE_PRESETS as preset (preset)}
                <button
                  type="button"
                  class="vibe-chip"
                  class:active={vibeQuery === preset}
                  onclick={() => (vibeQuery = preset)}
                >
                  {preset}
                </button>
              {/each}
            </div>
          </section>
        {/if}

        <section class="query-bar">
          {#if mode !== "vibe"}
            <div class="sample-wrap">
              <label class="field-label" for="vff-sample-chars">Candidate Glyphs</label>
              <input
                id="vff-sample-chars"
                type="text"
                class="text-input num"
                bind:value={sampleChars}
                placeholder="AHMNOORSTaeon"
              />
            </div>
          {/if}
          <button
            type="button"
            class="btn primary search-btn"
            onclick={findMatchingFonts}
            disabled={!canSearch}
          >
            <Search size={13} strokeWidth={2.2} />
            <span>{searching ? "Matching…" : mode === "vibe" ? "Search by Vibe" : "Find Matching Fonts"}</span>
          </button>
        </section>

        {#if error}
          <div class="err-box" role="alert">{error}</div>
        {/if}

        {#if results.length > 0}
          <section class="results-section">
            <div class="results-head">
              <span class="field-label">Top Matches · {results.length}</span>
            </div>
            <div class="results-list">
              {#each results as hit, idx (`${hit.path ?? hit.family}-${hit.matched_char}-${idx}`)}
                {@const preview = resolvePreviewStyle(hit)}
                <article class="hit-card">
                  <div
                    class="hit-glyph"
                    style:font-family={preview.family}
                    style:font-weight={preview.weight}
                    style:font-style={preview.italic ? "italic" : "normal"}
                    title={`Matched glyph "${hit.matched_char}"`}
                  >
                    {hit.matched_char || "Aa"}
                  </div>

                  <div class="hit-body">
                    <div class="hit-top">
                      <span class="hit-family">{hit.family}</span>
                      <span class="score-pill num">{Math.round(hit.score)}% match</span>
                      {#if hit.matched_char}
                        <span class="char-badge num" title="Matched character">
                          “{hit.matched_char}”
                        </span>
                      {/if}
                      {#if Array.isArray(hit.matched_traits)}
                        {#each hit.matched_traits.slice(0, 3) as trait (trait)}
                          <span class="trait-badge">{trait}</span>
                        {/each}
                      {/if}
                    </div>
                    <div
                      class="hit-specimen"
                      style:font-family={preview.family}
                      style:font-weight={preview.weight}
                      style:font-style={preview.italic ? "italic" : "normal"}
                    >
                      {hit.matched_char ? `${hit.matched_char} · ` : ""}Hamburgefonstiv 0123456789
                    </div>
                    {#if hit.path}
                      <div class="hit-path num" title={hit.path}>{hit.path}</div>
                    {/if}
                  </div>

                  <div class="hit-actions">
                    <button type="button" class="btn sm" onclick={() => inspectHit(hit)}>
                      <Eye size={12} strokeWidth={2} />
                      <span>Inspect</span>
                    </button>
                    <button type="button" class="btn sm accent" onclick={() => forkHit(hit)}>
                      <GitBranch size={12} strokeWidth={2} />
                      <span>Fork in Foundry</span>
                    </button>
                  </div>
                </article>
              {/each}
            </div>
          </section>
        {:else if hasSearched && !searching && !error}
          <div class="empty-results">
            No close matches were found for those candidate glyphs. Try adding the visible letter to Candidate Glyphs or cropping closer to a single character.
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 25;
    background: var(--velo);
    backdrop-filter: blur(5px);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: vff-fade 0.16s ease-out;
  }
  @keyframes vff-fade {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .modal {
    width: 740px;
    max-width: calc(100vw - 40px);
    max-height: 84vh;
    border-radius: 14px;
    background: var(--win);
    color: var(--ink);
    box-shadow: var(--float-shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .head {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--line);
  }
  .title-wrap {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .title-ico {
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    flex: none;
  }
  .title-wrap h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    color: var(--ink);
  }
  .sub {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--ink2);
  }
  .close {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink3);
    display: grid;
    place-items: center;
  }
  .close:hover {
    background: var(--hov);
    color: var(--ink);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .notice {
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--accent-soft);
    border: 1px solid var(--line2);
    color: var(--ink);
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .mode-bar {
    display: inline-flex;
    align-self: flex-start;
    padding: 4px;
    border-radius: 10px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    gap: 4px;
  }
  .mode-tab {
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink2);
    font: 600 12px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 7px;
    transition: background-color 0.14s, color 0.14s;
  }
  .mode-tab:hover {
    color: var(--ink);
    background: var(--hov);
  }
  .mode-tab.active {
    background: var(--field);
    color: var(--accent);
    box-shadow: var(--pill-shadow);
  }

  .input-card {
    padding: 14px;
    border-radius: 12px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sr-input {
    display: none;
  }

  .dropzone {
    padding: 24px 18px;
    border-radius: 10px;
    border: 1.5px dashed var(--line2);
    background: var(--field);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 8px;
    transition: border-color 0.15s, background-color 0.15s;
  }
  .dropzone.dragging {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .dz-icon {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
  }
  .dz-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
  }
  .dz-sub {
    font-size: 11.5px;
    color: var(--ink3);
  }
  .dz-sub kbd {
    font: 600 10.5px var(--mono);
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line2);
    color: var(--ink2);
  }

  .preview-box {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px;
    border-radius: 10px;
    background: var(--field);
    border: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .thumb {
    width: 112px;
    height: 84px;
    object-fit: contain;
    border-radius: 8px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    flex: none;
  }
  .preview-meta {
    flex: 1;
    min-width: 180px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .meta-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    word-break: break-word;
  }
  .meta-sub {
    font-size: 11px;
    color: var(--ink3);
  }
  .preview-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
  }

  .svg-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }
  .svg-tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .file-chip {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 5px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink2);
  }
  .svg-area {
    width: 100%;
    min-height: 116px;
    resize: vertical;
    padding: 10px 12px;
    border-radius: 9px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 400 12px/1.45 var(--mono);
    outline: none;
  }
  .svg-area:focus {
    border-color: var(--accent);
  }

  .query-bar {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    flex-wrap: wrap;
  }
  .sample-wrap {
    flex: 1;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .field-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--ink2);
  }
  .field-label code {
    font: 500 11px var(--mono);
    color: var(--accent);
  }
  .text-input {
    height: 34px;
    padding: 0 11px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font-size: 12.5px;
    outline: none;
  }
  .text-input:focus {
    border-color: var(--accent);
  }
  .vibe-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .vibe-chip {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    cursor: pointer;
  }
  .vibe-chip:hover,
  .vibe-chip.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
  }

  .btn {
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 600 12px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: all 0.14s;
  }
  .btn:hover:not(:disabled) {
    background: var(--chip);
  }
  .btn.sm {
    height: 27px;
    padding: 0 10px;
    font-size: 11.5px;
  }
  .btn.ghost {
    background: transparent;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    box-shadow: var(--glow);
  }
  .btn.accent {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
    box-shadow: none;
  }
  .search-btn {
    height: 34px;
    padding: 0 15px;
  }

  .num {
    font-family: var(--mono);
  }

  .err-box {
    padding: 10px 12px;
    border-radius: 8px;
    background: rgba(251, 113, 133, 0.14);
    color: var(--danger);
    font-size: 12px;
  }

  .results-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .results-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .results-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hit-card {
    padding: 12px 14px;
    border-radius: 11px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .hit-glyph {
    width: 52px;
    height: 52px;
    border-radius: 9px;
    background: var(--field);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    font-size: 28px;
    line-height: 1;
    color: var(--ink);
    flex: none;
  }
  .hit-body {
    flex: 1;
    min-width: 210px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .hit-top {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
  }
  .hit-family {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--ink);
  }
  .score-pill {
    font-size: 10.5px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .char-badge {
    font-size: 10.5px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 5px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink2);
  }
  .trait-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink2);
  }
  .hit-specimen {
    font-size: 16px;
    line-height: 1.3;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit-path {
    font-size: 10.5px;
    color: var(--ink3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .empty-results {
    padding: 26px 16px;
    text-align: center;
    font-size: 12.5px;
    color: var(--ink3);
    border-radius: 10px;
    background: var(--surface, var(--side));
    border: 1px dashed var(--line2);
  }
</style>
