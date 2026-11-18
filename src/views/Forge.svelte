<script>
  import { onDestroy } from "svelte";
  import { Download, Sparkles, RotateCcw, FlipHorizontal2, Eraser, Check, Contrast } from "@lucide/svelte";
  import {
    MATRIX_COLS,
    MATRIX_ROWS,
    PRESETS,
    SHAPES,
    CHAR_GROUPS,
    buildPresetGlyphs,
    cloneGrid,
    compileTrueTypeFont,
  } from "../lib/forge.js";
  import { installForgedFont } from "../lib/fonts.svelte.js";
  import { settings } from "../lib/settings.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let { sample = "The quick cosmic fox jumps over the lazy dog 0123456789", onInstalled = null } = $props();

  let familyName = $state("Nebula Sans");
  let styleName = $state("Regular");
  let activePreset = $state("cosmic");
  let shape = $state("chamfer");
  let weight = $state(88);
  let contrast = $state(96);
  let slant = $state(0);
  let tracking = $state(80);
  let corner = $state(26);

  let glyphs = $state(buildPresetGlyphs("cosmic"));
  let activeChar = $state("A");
  let testPhrase = $state("");
  let testSize = $state(38);
  let painting = $state(null); // 1 | 0 | null while dragging
  let installing = $state(false);
  let liveFamilyId = $state("ForgePreview");
  let compileError = $state("");
  let compiledBytes = $state(new Uint8Array(0));

  const currentGrid = $derived(glyphs[activeChar] ?? Array.from({ length: MATRIX_ROWS }, () => new Array(MATRIX_COLS).fill(0)));
  const effectivePhrase = $derived(testPhrase.trim() || sample || "COSMIC FONT FOUNDRY 0123456789");

  let activeFontFace = null;
  let compileSeq = 0;

  $effect(() => {
    const payload = {
      familyName,
      styleName,
      weight,
      contrast,
      slant,
      tracking,
      corner,
      shape,
      glyphs,
    };
    const seq = ++compileSeq;
    try {
      const bytes = compileTrueTypeFont(payload);
      compiledBytes = bytes;
      compileError = "";
      const faceName = `ForgeLive_${seq}`;
      const face = new FontFace(faceName, bytes);
      face
        .load()
        .then((loaded) => {
          if (seq !== compileSeq) return;
          if (activeFontFace) document.fonts.delete(activeFontFace);
          activeFontFace = loaded;
          document.fonts.add(loaded);
          liveFamilyId = faceName;
        })
        .catch((err) => {
          if (seq === compileSeq) compileError = String(err);
        });
    } catch (err) {
      compileError = String(err);
    }
  });

  onDestroy(() => {
    if (activeFontFace) document.fonts.delete(activeFontFace);
  });

  function applyPreset(p) {
    activePreset = p.id;
    shape = p.shape;
    weight = p.weight;
    contrast = p.contrast;
    slant = p.slant;
    tracking = p.tracking;
    corner = p.corner;
    glyphs = buildPresetGlyphs(p.id);
  }

  function setCell(r, c, val) {
    const grid = cloneGrid(currentGrid);
    if (grid[r][c] === val) return;
    grid[r][c] = val;
    glyphs = { ...glyphs, [activeChar]: grid };
  }

  function startPaint(r, c) {
    const next = currentGrid[r][c] ? 0 : 1;
    painting = next;
    setCell(r, c, next);
  }

  function enterPaint(r, c) {
    if (painting === null) return;
    setCell(r, c, painting);
  }

  function clearGlyph() {
    const empty = Array.from({ length: MATRIX_ROWS }, () => new Array(MATRIX_COLS).fill(0));
    glyphs = { ...glyphs, [activeChar]: empty };
  }

  function invertGlyph() {
    const inv = currentGrid.map((row) => row.map((v) => (v ? 0 : 1)));
    glyphs = { ...glyphs, [activeChar]: inv };
  }

  function mirrorGlyph() {
    const mir = currentGrid.map((row) => [...row].reverse());
    glyphs = { ...glyphs, [activeChar]: mir };
  }

  function resetCurrentGlyph() {
    const fresh = buildPresetGlyphs(activePreset);
    if (fresh[activeChar]) glyphs = { ...glyphs, [activeChar]: fresh[activeChar] };
  }

  function exportTtf() {
    if (!compiledBytes.length) return;
    const slug = `${familyName.trim() || "cosmic-forge"}-${styleName}`.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    const blob = new Blob([compiledBytes], { type: "font/ttf" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${slug}.ttf`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    notify("ok", t("forge.exported"), `${slug}.ttf`);
  }

  async function installForged() {
    if (!compiledBytes.length || installing) return;
    installing = true;
    try {
      const o = await installForgedFont(familyName, styleName, compiledBytes, settings.installFor === "all");
      if (o && !o.errors?.length && !o.cancelled) {
        notify("ok", t("ok.installed", { n: familyName }), t("ok.availableNow"));
        onInstalled?.();
      }
    } finally {
      installing = false;
    }
  }
</script>

<svelte:window onmouseup={() => (painting = null)} />

<div class="forge">
  <div class="forge-bar">
    <label class="name-box">
      <span class="lbl">{t("forge.family")}</span>
      <input bind:value={familyName} maxlength="40" spellcheck="false" placeholder="Nebula Sans" />
    </label>

    <div class="style-pills">
      {#each ["Regular", "Bold", "Oblique"] as st (st)}
        <button type="button" class="pill" class:active={styleName === st} onclick={() => (styleName = st)}>{st}</button>
      {/each}
    </div>

    <div class="bar-actions">
      <span class="size-badge num">{Math.max(1, Math.round(compiledBytes.length / 1024))} KB · {Object.keys(glyphs).length + 2} glyphs</span>
      <button type="button" class="btn" onclick={exportTtf} disabled={!compiledBytes.length}>
        <Download size={13} strokeWidth={2} />{t("forge.export")}
      </button>
      <button type="button" class="btn primary" onclick={installForged} disabled={installing || !compiledBytes.length}>
        <Check size={13} strokeWidth={2.2} />{installing ? t("forge.installing") : t("forge.install")}
      </button>
    </div>
  </div>

  <div class="workspace">
    <!-- Left Column: Character Picker + 7x9 Blueprint Matrix -->
    <section class="panel editor-col">
      <div class="panel-head">
        <span class="label">{t("forge.matrix")} · “{activeChar}”</span>
        <div class="tool-btns">
          <button type="button" class="mini" title={t("forge.mirror")} onclick={mirrorGlyph}><FlipHorizontal2 size={12} /></button>
          <button type="button" class="mini" title={t("forge.invert")} onclick={invertGlyph}><Contrast size={12} /></button>
          <button type="button" class="mini" title={t("forge.clear")} onclick={clearGlyph}><Eraser size={12} /></button>
          <button type="button" class="mini" title={t("forge.resetGlyph")} onclick={resetCurrentGlyph}><RotateCcw size={12} /></button>
        </div>
      </div>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="matrix-stage" onmouseleave={() => (painting = null)}>
        <div class="matrix" role="grid" aria-label="Glyph blueprint matrix">
          {#each currentGrid as row, r (r)}
            {#each row as cell, c (c)}
              <button
                type="button"
                class="cell"
                class:on={cell === 1}
                class:baseline={r === 8}
                class:xheight={r === 2}
                aria-label="Row {r + 1} Col {c + 1}"
                aria-pressed={cell === 1}
                onmousedown={(e) => {
                  e.preventDefault();
                  startPaint(r, c);
                }}
                onmouseenter={() => enterPaint(r, c)}
              ></button>
            {/each}
          {/each}
        </div>
        <div class="guide-legend">
          <span>{MATRIX_COLS}×{MATRIX_ROWS} EM GRID</span>
          <span>1000 UPM · TTF</span>
        </div>
      </div>

      <div class="char-groups">
        {#each CHAR_GROUPS as grp (grp.id)}
          <div class="char-group">
            <span class="grp-lbl">{grp.label}</span>
            <div class="char-grid">
              {#each grp.chars as ch (ch)}
                <button
                  type="button"
                  class="char-btn"
                  class:active={activeChar === ch}
                  onclick={() => (activeChar = ch)}
                >
                  {ch}
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </section>

    <!-- Right Column: DNA Presets, Module Shapes, Sliders & Live Compiled Specimen -->
    <section class="controls-col">
      <div class="panel">
        <div class="panel-head">
          <span class="label"><Sparkles size={11} /> {t("forge.presets")}</span>
        </div>
        <div class="preset-grid">
          {#each PRESETS as p (p.id)}
            <button type="button" class="preset-btn" class:active={activePreset === p.id} onclick={() => applyPreset(p)}>
              {p.name}
            </button>
          {/each}
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">
          <span class="label">{t("forge.shape")}</span>
        </div>
        <div class="shape-grid">
          {#each SHAPES as s (s.id)}
            <button type="button" class="shape-btn" class:active={shape === s.id} onclick={() => (shape = s.id)}>
              <span class="shape-icon {s.id}"></span>
              <span>{s.label}</span>
            </button>
          {/each}
        </div>

        <div class="sliders">
          <label class="slider-row">
            <span class="s-lbl">{t("forge.weight")}</span>
            <input type="range" min="35" max="110" bind:value={weight} />
            <span class="s-val num">{weight}%</span>
          </label>
          <label class="slider-row">
            <span class="s-lbl">{t("forge.contrast")}</span>
            <input type="range" min="45" max="125" bind:value={contrast} />
            <span class="s-val num">{contrast}%</span>
          </label>
          <label class="slider-row">
            <span class="s-lbl">{t("forge.slant")}</span>
            <input type="range" min="-16" max="20" bind:value={slant} />
            <span class="s-val num">{slant}°</span>
          </label>
          <label class="slider-row">
            <span class="s-lbl">{t("forge.tracking")}</span>
            <input type="range" min="25" max="180" bind:value={tracking} />
            <span class="s-val num">{tracking}</span>
          </label>
          <label class="slider-row">
            <span class="s-lbl">{t("forge.corner")}</span>
            <input type="range" min="0" max="48" bind:value={corner} />
            <span class="s-val num">{corner}%</span>
          </label>
        </div>
      </div>

      <div class="panel specimen-panel">
        <div class="panel-head">
          <span class="label">{t("forge.livePreview")}</span>
          <div class="test-controls">
            <input class="test-input" bind:value={testPhrase} placeholder={t("forge.typeTest")} spellcheck="false" />
            <input class="test-size" type="range" min="18" max="72" bind:value={testSize} aria-label="Preview size" />
            <span class="num size-val">{testSize}px</span>
          </div>
        </div>
        {#if compileError}
          <div class="err-box">{compileError}</div>
        {/if}
        <div
          class="live-stage"
          style:font-family={`"${liveFamilyId}", var(--sans)`}
          style:font-size="{testSize}px"
        >
          {effectivePhrase}
        </div>
        <div class="alphabet-strip" style:font-family={`"${liveFamilyId}", var(--sans)`}>
          ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 !?+-/
        </div>
      </div>
    </section>
  </div>
</div>

<style>
  .forge {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 24px 20px;
    overflow: hidden;
  }
  .forge-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .name-box {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 30px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
  }
  .name-box .lbl { font: 700 10px var(--mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent-2); }
  .name-box input { border: 0; outline: 0; background: transparent; font: 600 13px var(--sans); color: var(--ink); width: 150px; }

  .style-pills { display: flex; gap: 4px; }
  .pill {
    height: 26px; padding: 0 10px; border: 1px solid var(--line); border-radius: 999px;
    background: transparent; color: var(--ink2); font: 600 11.5px var(--sans); transition: all 0.14s;
  }
  .pill:hover { background: var(--hov); color: var(--ink); }
  .pill.active { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }

  .bar-actions { margin-left: auto; display: flex; align-items: center; gap: 8px; }
  .size-badge { font: 500 11px var(--mono); color: var(--ink3); padding-right: 4px; }
  .btn {
    height: 30px; padding: 0 12px; border: 1px solid var(--line2); border-radius: 8px;
    background: var(--field); color: var(--ink); font: 600 12px var(--sans);
    display: inline-flex; align-items: center; gap: 6px; transition: all 0.15s;
  }
  .btn:hover:not(:disabled) { background: var(--chip); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; box-shadow: var(--glow); }
  .btn.primary:hover:not(:disabled) { filter: brightness(1.08); }
  .btn:disabled { opacity: 0.55; cursor: default; }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 330px minmax(0, 1fr);
    gap: 14px;
  }
  .panel {
    background: var(--side);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .editor-col { min-height: 0; overflow-y: auto; }
  .controls-col { min-height: 0; display: flex; flex-direction: column; gap: 12px; overflow-y: auto; }

  .panel-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .panel-head .label { display: inline-flex; align-items: center; gap: 5px; }
  .tool-btns { display: flex; gap: 4px; }
  .mini {
    width: 24px; height: 24px; border: 1px solid var(--line); border-radius: 6px;
    background: var(--field); color: var(--ink2); display: grid; place-items: center;
  }
  .mini:hover { color: var(--ink); border-color: var(--line2); }

  .matrix-stage {
    padding: 12px;
    border-radius: 10px;
    background: var(--win);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .matrix {
    display: grid;
    grid-template-columns: repeat(7, 26px);
    grid-template-rows: repeat(9, 26px);
    gap: 3px;
  }
  .cell {
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--field);
    padding: 0;
    transition: background-color 0.08s, border-color 0.08s, transform 0.08s;
  }
  .cell.xheight { border-top-color: rgba(34, 211, 238, 0.35); }
  .cell.baseline { border-bottom-color: rgba(155, 109, 255, 0.5); }
  .cell:hover { border-color: var(--accent-2); }
  .cell.on {
    background: var(--accent);
    border-color: var(--accent-2);
    box-shadow: 0 0 10px rgba(155, 109, 255, 0.45);
  }
  .guide-legend { width: 100%; display: flex; justify-content: space-between; font: 500 10px var(--mono); color: var(--ink3); }

  .char-groups { display: flex; flex-direction: column; gap: 8px; }
  .char-group { display: flex; flex-direction: column; gap: 4px; }
  .grp-lbl { font: 600 9.5px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink3); }
  .char-grid { display: grid; grid-template-columns: repeat(9, minmax(0, 1fr)); gap: 3px; }
  .char-btn {
    height: 24px; border: 1px solid var(--line); border-radius: 5px;
    background: var(--field); color: var(--ink2); font: 600 11.5px var(--mono);
    display: grid; place-items: center; padding: 0;
  }
  .char-btn:hover { border-color: var(--accent-2); color: var(--ink); }
  .char-btn.active { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }

  .preset-grid { display: flex; flex-wrap: wrap; gap: 6px; }
  .preset-btn {
    height: 28px; padding: 0 11px; border: 1px solid var(--line); border-radius: 8px;
    background: var(--field); color: var(--ink2); font: 600 11.5px var(--sans);
  }
  .preset-btn:hover { border-color: var(--line2); color: var(--ink); }
  .preset-btn.active { background: var(--cyan-soft); border-color: var(--accent-2); color: var(--ink); }

  .shape-grid { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 6px; }
  .shape-btn {
    height: 46px; border: 1px solid var(--line); border-radius: 8px; background: var(--field);
    color: var(--ink2); font: 600 10.5px var(--sans);
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px;
  }
  .shape-btn:hover { border-color: var(--line2); color: var(--ink); }
  .shape-btn.active { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }
  .shape-icon { width: 12px; height: 12px; background: currentColor; display: block; }
  .shape-icon.chamfer { clip-path: polygon(25% 0%, 75% 0%, 100% 25%, 100% 75%, 75% 100%, 25% 100%, 0% 75%, 0% 25%); }
  .shape-icon.rounded { border-radius: 4px; }
  .shape-icon.diamond { transform: rotate(45deg) scale(0.8); }
  .shape-icon.slit { height: 7px; border-radius: 1px; }
  .shape-icon.hollow { background: transparent; border: 2.5px solid currentColor; }

  .sliders { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 8px 16px; padding-top: 4px; }
  .slider-row { display: grid; grid-template-columns: 78px minmax(0, 1fr) 40px; align-items: center; gap: 8px; }
  .s-lbl { font-size: 11.5px; color: var(--ink2); }
  .s-val { font: 500 11px var(--mono); color: var(--ink3); text-align: right; }
  .slider-row input[type="range"] { -webkit-appearance: none; appearance: none; height: 3px; border-radius: 3px; background: var(--line2); outline: none; }
  .slider-row input[type="range"]::-webkit-slider-thumb { -webkit-appearance: none; width: 12px; height: 12px; border-radius: 50%; background: var(--accent); cursor: pointer; }

  .specimen-panel { flex: 1; min-height: 180px; }
  .test-controls { display: flex; align-items: center; gap: 8px; }
  .test-input {
    height: 24px; width: 190px; padding: 0 8px; border: 1px solid var(--line); border-radius: 6px;
    background: var(--field); color: var(--ink); font: 500 11.5px var(--sans);
  }
  .test-size { width: 72px; }
  .size-val { font: 500 11px var(--mono); color: var(--ink3); width: 34px; text-align: right; }
  .live-stage {
    flex: 1;
    min-height: 92px;
    padding: 14px 16px;
    border-radius: 10px;
    background: var(--win);
    border: 1px solid var(--line);
    color: var(--ink);
    line-height: 1.3;
    word-break: break-word;
    overflow-y: auto;
  }
  .alphabet-strip {
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line);
    font-size: 16px;
    color: var(--ink2);
    letter-spacing: 0.04em;
    white-space: nowrap;
    overflow-x: auto;
  }
  .err-box { padding: 8px 10px; border-radius: 8px; background: rgba(251, 113, 133, 0.14); color: var(--danger); font-size: 12px; }
</style>
