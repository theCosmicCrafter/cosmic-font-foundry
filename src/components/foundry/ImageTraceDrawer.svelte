<script>
  import { onDestroy } from "svelte";
  import {
    ImagePlus,
    Upload,
    Scan,
    Spline,
    Grid3x3,
    ChevronDown,
    ChevronRight,
    PenTool,
    Feather,
    X,
    Trash,
  } from "@lucide/svelte";

  let {
    upm = 1000,
    busy = false,
    onTraceSingle = null,
    onTraceSheet = null,
    onApplyOutlineStroke = null,
    onApplyCalligraphy = null,
    onclose = null,
  } = $props();

  const ACCEPTED_EXT = /\.(png|jpe?g|bmp|webp)$/i;

  let fileInput = $state(null);
  let dragOver = $state(false);
  let fileName = $state("");
  let imageBytes = $state(null);
  let previewUrl = $state("");
  let fileError = $state("");

  // Trace options
  let mode = $state("outline"); // "outline" | "centerline"
  let threshold = $state(140);
  let simplifyTol = $state(2.0);
  let smoothCorners = $state(true);
  let invert = $state(false);

  // Batch sheet slicer
  let sheetOpen = $state(false);
  let cols = $state(6);
  let rows = $state(5);
  let chars = $state("ABCDEFGHIJKLMNOPQRSTUVWXYZ");

  // Skeleton & calligraphic stroke expansion
  let strokeWidth = $state(32);
  let roundCaps = $state(true);
  let nibSize = $state(44);
  let nibAngle = $state(35);
  let nibRoundness = $state(0.35);

  function revokePreview() {
    if (previewUrl) {
      URL.revokeObjectURL(previewUrl);
      previewUrl = "";
    }
  }

  onDestroy(() => {
    revokePreview();
  });

  function formatBytes(len) {
    if (!len) return "0 B";
    if (len < 1024) return `${len} B`;
    if (len < 1024 * 1024) return `${(len / 1024).toFixed(1)} KB`;
    return `${(len / (1024 * 1024)).toFixed(2)} MB`;
  }

  async function loadImageFile(file) {
    if (!file) return;
    if (!file.type.startsWith("image/") && !ACCEPTED_EXT.test(file.name)) {
      fileError = "Unsupported file. Use PNG, JPG, BMP, or WebP.";
      return;
    }
    fileError = "";
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    revokePreview();
    previewUrl = URL.createObjectURL(file);
    fileName = file.name;
    imageBytes = bytes;
  }

  function handleFileChange(e) {
    const file = e.currentTarget?.files?.[0];
    if (file) loadImageFile(file);
  }

  function handleDrop(e) {
    e.preventDefault();
    dragOver = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) loadImageFile(file);
  }

  function handleDragOver(e) {
    e.preventDefault();
    dragOver = true;
  }

  function openFilePicker() {
    if (busy) return;
    fileInput?.click();
  }

  function clearImage() {
    revokePreview();
    fileName = "";
    imageBytes = null;
    fileError = "";
    if (fileInput) fileInput.value = "";
  }

  function buildTraceOpts() {
    return {
      mode,
      threshold: Number(threshold),
      smooth_corners: smoothCorners,
      simplify_tol: Number(simplifyTol),
      stroke_width: Number(strokeWidth),
      invert,
      upm: Number(upm) || 1000,
    };
  }

  async function handleTraceSingle() {
    if (busy || !imageBytes?.length || !onTraceSingle) return;
    await onTraceSingle(imageBytes, buildTraceOpts());
  }

  async function handleTraceSheet() {
    if (busy || !imageBytes?.length || !chars.length || !onTraceSheet) return;
    const c = Math.max(1, Math.round(Number(cols) || 6));
    const r = Math.max(1, Math.round(Number(rows) || 5));
    await onTraceSheet(imageBytes, c, r, chars, buildTraceOpts());
  }

  async function handleOutlineStroke() {
    if (busy || !onApplyOutlineStroke) return;
    await onApplyOutlineStroke(Number(strokeWidth), roundCaps);
  }

  async function handleCalligraphy() {
    if (busy || !onApplyCalligraphy) return;
    await onApplyCalligraphy(Number(nibSize), Number(nibAngle), Number(nibRoundness));
  }
</script>

<div class="trace-drawer">
  <div class="drawer-head">
    <div class="drawer-title">
      <Scan size={13} />
      <span class="label">Image Trace &amp; Calligraphic Skeleton</span>
    </div>
    {#if onclose}
      <button type="button" class="mini" title="Close" aria-label="Close trace drawer" onclick={onclose}>
        <X size={12} />
      </button>
    {/if}
  </div>

  <!-- Upload / Dropzone + Mode & Trace Sliders -->
  <div class="trace-grid">
    <div class="drop-col">
      <input
        bind:this={fileInput}
        type="file"
        accept=".png,.jpg,.jpeg,.bmp,.webp,image/*"
        class="sr-input"
        onchange={handleFileChange}
      />

      {#if previewUrl && imageBytes}
        <div
          class="preview-card"
          class:drag-over={dragOver}
          ondragover={handleDragOver}
          ondragleave={() => (dragOver = false)}
          ondrop={handleDrop}
          role="region"
          aria-label="Loaded trace image"
        >
          <img src={previewUrl} alt={fileName} class="thumb" />
          <div class="file-meta">
            <span class="file-name" title={fileName}>{fileName}</span>
            <span class="file-size num">{formatBytes(imageBytes.length)} · {imageBytes.length.toLocaleString()} B</span>
            <div class="file-actions">
              <button type="button" class="btn-inline" disabled={busy} onclick={openFilePicker}>
                <Upload size={11} />Replace
              </button>
              <button
                type="button"
                class="mini"
                title="Clear image"
                aria-label="Clear image"
                disabled={busy}
                onclick={clearImage}
              >
                <Trash size={11} />
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div
          class="dropzone"
          class:drag-over={dragOver}
          role="button"
          tabindex="0"
          ondragover={handleDragOver}
          ondragleave={() => (dragOver = false)}
          ondrop={handleDrop}
          onclick={openFilePicker}
          onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), openFilePicker())}
        >
          <ImagePlus size={18} />
          <span class="drop-main">Drop image or click to browse</span>
          <span class="drop-sub">PNG · JPG · BMP · WebP</span>
        </div>
      {/if}

      {#if fileError}
        <div class="file-err">{fileError}</div>
      {/if}
    </div>

    <div class="opts-col">
      <div class="mode-row">
        <span class="opt-lbl">Trace Mode</span>
        <div class="mode-switch" role="radiogroup" aria-label="Trace mode">
          <button
            type="button"
            class="mode-btn"
            class:active={mode === "outline"}
            role="radio"
            aria-checked={mode === "outline"}
            onclick={() => (mode = "outline")}
          >
            <Scan size={11} />Outline / Logo Scan
          </button>
          <button
            type="button"
            class="mode-btn"
            class:active={mode === "centerline"}
            role="radio"
            aria-checked={mode === "centerline"}
            onclick={() => (mode = "centerline")}
          >
            <Spline size={11} />Centerline Skeleton
          </button>
        </div>
      </div>

      <div class="slider-grid">
        <label class="slider-row">
          <span class="s-lbl">Threshold</span>
          <input type="range" min="10" max="245" step="1" bind:value={threshold} />
          <span class="s-val num">{threshold}</span>
        </label>
        <label class="slider-row">
          <span class="s-lbl">Simplify</span>
          <input type="range" min="0.5" max="8.0" step="0.5" bind:value={simplifyTol} />
          <span class="s-val num">{simplifyTol.toFixed(1)}</span>
        </label>
      </div>

      <div class="checks-action-row">
        <div class="check-group">
          <label class="toggle-chip" class:on={smoothCorners}>
            <input type="checkbox" bind:checked={smoothCorners} />
            <span>Smooth Corners</span>
          </label>
          <label class="toggle-chip" class:on={invert}>
            <input type="checkbox" bind:checked={invert} />
            <span>Invert</span>
          </label>
        </div>

        <button
          type="button"
          class="btn-inline accent primary-trace"
          disabled={busy || !imageBytes?.length}
          onclick={handleTraceSingle}
        >
          <Scan size={12} />Trace Into Active Glyph
        </button>
      </div>
    </div>
  </div>

  <!-- Collapsible Glyph Sheet Batch Slicer -->
  <div class="sub-section">
    <button
      type="button"
      class="collapse-head"
      aria-expanded={sheetOpen}
      onclick={() => (sheetOpen = !sheetOpen)}
    >
      <span class="collapse-title">
        {#if sheetOpen}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
        <Grid3x3 size={12} />
        <span>Glyph Sheet Batch Slicer</span>
      </span>
      <span class="collapse-hint num">{cols}×{rows} · {chars.length} chars</span>
    </button>

    {#if sheetOpen}
      <div class="sheet-body">
        <label class="num-field">
          <span>Cols</span>
          <input type="number" min="1" max="32" bind:value={cols} />
        </label>
        <label class="num-field">
          <span>Rows</span>
          <input type="number" min="1" max="32" bind:value={rows} />
        </label>
        <label class="chars-field">
          <span>Chars</span>
          <input type="text" bind:value={chars} spellcheck="false" placeholder="ABCDEFGHIJKLMNOPQRSTUVWXYZ" />
        </label>
        <button
          type="button"
          class="btn-inline accent"
          disabled={busy || !imageBytes?.length || !chars.length}
          onclick={handleTraceSheet}
        >
          <Grid3x3 size={12} />Trace Sheet Into Project
        </button>
      </div>
    {/if}
  </div>

  <!-- Skeleton / Stroke Expansion Bar -->
  <div class="expansion-bar">
    <div class="exp-group">
      <div class="exp-head">
        <span class="exp-title"><PenTool size={12} />Outline Stroke</span>
        <label class="toggle-chip compact" class:on={roundCaps}>
          <input type="checkbox" bind:checked={roundCaps} />
          <span>Round Caps</span>
        </label>
        <button
          type="button"
          class="btn-inline"
          disabled={busy}
          onclick={handleOutlineStroke}
        >
          Expand Stroke
        </button>
      </div>
      <label class="slider-row">
        <span class="s-lbl">Width</span>
        <input type="range" min="6" max="120" step="1" bind:value={strokeWidth} />
        <span class="s-val num">{strokeWidth}</span>
      </label>
    </div>

    <div class="exp-divider"></div>

    <div class="exp-group exp-nib">
      <div class="exp-head">
        <span class="exp-title"><Feather size={12} />Calligraphic Nib</span>
        <button
          type="button"
          class="btn-inline accent"
          disabled={busy}
          onclick={handleCalligraphy}
        >
          Sweep Nib
        </button>
      </div>
      <div class="nib-sliders">
        <label class="slider-row">
          <span class="s-lbl">Size</span>
          <input type="range" min="8" max="120" step="1" bind:value={nibSize} />
          <span class="s-val num">{nibSize}</span>
        </label>
        <label class="slider-row">
          <span class="s-lbl">Angle</span>
          <input type="range" min="-90" max="90" step="1" bind:value={nibAngle} />
          <span class="s-val num">{nibAngle}°</span>
        </label>
        <label class="slider-row">
          <span class="s-lbl">Roundness</span>
          <input type="range" min="0.15" max="1.0" step="0.05" bind:value={nibRoundness} />
          <span class="s-val num">{nibRoundness.toFixed(2)}</span>
        </label>
      </div>
    </div>
  </div>
</div>

<style>
  .trace-drawer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--win);
    border: 1px solid var(--line2);
  }

  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .drawer-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-2);
  }
  .mini {
    width: 24px;
    height: 24px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--field);
    color: var(--ink2);
    display: grid;
    place-items: center;
  }
  .mini:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--line2);
  }
  .mini:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .trace-grid {
    display: grid;
    grid-template-columns: 210px minmax(0, 1fr);
    gap: 10px;
    align-items: stretch;
  }
  @media (max-width: 780px) {
    .trace-grid {
      grid-template-columns: 1fr;
    }
  }

  .drop-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sr-input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  .dropzone {
    flex: 1;
    min-height: 92px;
    padding: 10px;
    border: 1.5px dashed var(--line2);
    border-radius: 8px;
    background: var(--field);
    color: var(--ink2);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    text-align: center;
    cursor: pointer;
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }
  .dropzone:hover,
  .dropzone.drag-over {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
  }
  .drop-main {
    font: 600 11px var(--sans);
  }
  .drop-sub {
    font: 500 10px var(--mono);
    color: var(--ink3);
  }

  .preview-card {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--field);
    transition: border-color 0.14s;
  }
  .preview-card.drag-over {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .thumb {
    width: 58px;
    height: 58px;
    flex: none;
    object-fit: contain;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: repeating-conic-gradient(var(--chip) 0% 25%, transparent 0% 50%) 50% / 10px 10px;
  }
  .file-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .file-name {
    font: 600 11px var(--sans);
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file-size {
    font: 500 10px var(--mono);
    color: var(--ink3);
  }
  .file-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
  }
  .file-err {
    font: 500 10.5px var(--sans);
    color: var(--danger);
  }

  .opts-col {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .mode-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .opt-lbl {
    font: 600 11px var(--sans);
    color: var(--ink2);
  }
  .mode-switch {
    display: inline-flex;
    padding: 2px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
    gap: 2px;
  }
  .mode-btn {
    height: 24px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: 600 11px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    transition: all 0.14s;
  }
  .mode-btn:hover {
    color: var(--ink);
  }
  .mode-btn.active {
    background: var(--accent);
    color: #fff;
  }

  .slider-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(175px, 1fr));
    gap: 6px 14px;
  }
  .slider-row {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr) 36px;
    align-items: center;
    gap: 7px;
  }
  .s-lbl {
    font-size: 11px;
    color: var(--ink2);
  }
  .s-val {
    font: 500 11px var(--mono);
    color: var(--ink3);
    text-align: right;
  }
  .slider-row input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    height: 3px;
    border-radius: 3px;
    background: var(--line2);
    outline: none;
    width: 100%;
  }
  .slider-row input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
  }

  .checks-action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .check-group {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .toggle-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border-radius: 7px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    cursor: pointer;
  }
  .toggle-chip input {
    accent-color: var(--accent);
    margin: 0;
  }
  .toggle-chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
  }

  .btn-inline {
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--line2);
    border-radius: 7px;
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .btn-inline:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .btn-inline.accent {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .btn-inline.primary-trace:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn-inline:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .sub-section {
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--side);
    overflow: hidden;
  }
  .collapse-head {
    width: 100%;
    padding: 6px 10px;
    border: 0;
    background: transparent;
    color: var(--ink2);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font: 600 11px var(--sans);
  }
  .collapse-head:hover {
    color: var(--ink);
    background: var(--hov);
  }
  .collapse-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .collapse-hint {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }
  .sheet-body {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-top: 1px solid var(--line);
  }
  .num-field,
  .chars-field {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 7px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
    font: 600 10.5px var(--sans);
    color: var(--ink2);
  }
  .num-field input {
    width: 42px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }
  .chars-field {
    flex: 1;
    min-width: 160px;
  }
  .chars-field input {
    width: 100%;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 11px var(--mono);
  }

  .expansion-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .exp-group {
    flex: 1;
    min-width: 210px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .exp-nib {
    flex: 1.5;
    min-width: 280px;
  }
  .exp-divider {
    width: 1px;
    background: var(--line);
    align-self: stretch;
  }
  @media (max-width: 720px) {
    .exp-divider {
      width: 100%;
      height: 1px;
    }
  }
  .exp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    flex-wrap: wrap;
  }
  .exp-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 11px var(--sans);
    color: var(--ink);
  }
  .nib-sliders {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(145px, 1fr));
    gap: 6px 12px;
  }
</style>
