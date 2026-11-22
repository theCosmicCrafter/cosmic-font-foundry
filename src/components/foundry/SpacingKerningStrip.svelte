<script>
  import {
    Sparkles,
    SlidersHorizontal,
    ArrowLeftRight,
    Plus,
    Trash,
    Type,
    Search,
    X,
  } from "@lucide/svelte";

  const SAMPLE_PRESETS = [
    "HHxOOx",
    "AV To Wa LT",
    "Hamburgerfontsiv",
    "nnxoo",
  ];

  let {
    project = null,
    liveFamilyId = "ForgePreview",
    activeKey = "",
    busy = false,
    onSelectGlyph = null,
    onUpdateMetrics = null,
    onUpdateKerning = null,
    onAutoSpace = null,
    onAutoKern = null,
  } = $props();

  // ---- Optical Automation State ----
  let targetSidebearing = $state(60);
  let targetOpticalGap = $state(130);

  // ---- Text Spacing Strip State ----
  let testString = $state("HHxOOx AV To Wa LT Hamburgerfontsiv");
  let editingPairIndex = $state(null);
  let editingPairDraft = $state("");
  let scrubbingIndex = $state(null);
  let scrubSession = null;

  // ---- Custom Kerning Pair Form & Filter State ----
  let newLeftChar = $state("A");
  let newRightChar = $state("V");
  let newKernValue = $state(-40);
  let pairFilter = $state("");

  function clampInt(val, min, max) {
    const num = Math.round(Number(val));
    if (!Number.isFinite(num)) return min;
    return Math.min(max, Math.max(min, num));
  }

  function resolveGlyphCodepoint(key, glyph) {
    if (glyph?.codepoint > 0) return glyph.codepoint;
    if (/^\d+$/.test(key)) return Number(key);
    const chars = [...String(key ?? "")];
    return chars.length === 1 ? (chars[0].codePointAt(0) ?? 0) : 0;
  }

  function formatCpHex(cp) {
    return (cp ?? 0).toString(16).toUpperCase().padStart(4, "0");
  }

  function formatCpChar(cp) {
    if (!cp || cp <= 0) return "?";
    if (cp === 32) return "␣";
    try {
      return String.fromCodePoint(cp);
    } catch {
      return "?";
    }
  }

  function formatKernValue(val) {
    const n = Math.round(Number(val) || 0);
    return n > 0 ? `+${n}` : `${n}`;
  }

  const glyphByCodepoint = $derived.by(() => {
    const map = new Map();
    const glyphs = project?.glyphs;
    if (!glyphs) return map;
    for (const [key, glyph] of Object.entries(glyphs)) {
      if (!glyph) continue;
      const cp = resolveGlyphCodepoint(key, glyph);
      if (cp > 0 && !map.has(cp)) {
        map.set(cp, { key, glyph });
      }
    }
    return map;
  });

  const kerningList = $derived(Array.isArray(project?.kerning) ? project.kerning : []);

  const kerningMap = $derived.by(() => {
    const map = new Map();
    for (const pair of kerningList) {
      if (!pair) continue;
      map.set(`${pair.left}:${pair.right}`, pair.value ?? 0);
    }
    return map;
  });

  const stripCells = $derived.by(() => {
    const chars = [...testString];
    return chars.map((ch, idx) => {
      const cp = ch.codePointAt(0) ?? 0;
      const entry = cp > 0 ? (glyphByCodepoint.get(cp) ?? null) : null;
      const nextCh = idx < chars.length - 1 ? chars[idx + 1] : null;
      const nextCp = nextCh ? (nextCh.codePointAt(0) ?? 0) : 0;
      const hasPair = Boolean(nextCh && cp > 0 && nextCp > 0);
      const pairValue = hasPair ? (kerningMap.get(`${cp}:${nextCp}`) ?? 0) : 0;
      return {
        idx,
        ch,
        cp,
        isSpace: ch === " ",
        key: entry?.key ?? "",
        glyph: entry?.glyph ?? null,
        isActive: Boolean(entry?.key && entry.key === activeKey),
        hasPair,
        nextCp,
        pairValue,
      };
    });
  });

  const selectedGlyph = $derived(activeKey ? (project?.glyphs?.[activeKey] ?? null) : null);
  const selectedCp = $derived(selectedGlyph ? resolveGlyphCodepoint(activeKey, selectedGlyph) : 0);
  const selectedChar = $derived(
    selectedCp > 0 ? formatCpChar(selectedCp) : (selectedGlyph?.name || "?"),
  );

  const filteredPairs = $derived.by(() => {
    const q = pairFilter.trim().toLowerCase().replace(/^u\+/, "");
    return kerningList
      .map((pair, index) => {
        const leftChar = formatCpChar(pair.left);
        const rightChar = formatCpChar(pair.right);
        const leftHex = formatCpHex(pair.left);
        const rightHex = formatCpHex(pair.right);
        return { pair, index, leftChar, rightChar, leftHex, rightHex };
      })
      .filter((item) => {
        if (!q) return true;
        return (
          item.leftChar.toLowerCase().includes(q) ||
          item.rightChar.toLowerCase().includes(q) ||
          `${item.leftChar}${item.rightChar}`.toLowerCase().includes(q) ||
          item.leftHex.toLowerCase().includes(q) ||
          item.rightHex.toLowerCase().includes(q)
        );
      });
  });

  // ---- Optical Automation Handlers ----
  async function handleAutoSpace() {
    if (busy || !project || !onAutoSpace) return;
    const sb = clampInt(targetSidebearing, 20, 160);
    targetSidebearing = sb;
    await onAutoSpace(sb);
  }

  async function handleAutoKern() {
    if (busy || !project || !onAutoKern) return;
    const gap = clampInt(targetOpticalGap, 60, 240);
    targetOpticalGap = gap;
    await onAutoKern(gap, 12);
  }

  // ---- Glyph Metrics Handlers ----
  function updateSelectedMetric(field, rawVal) {
    if (!project || !selectedGlyph || !activeKey || !onUpdateMetrics) return;
    const val = Math.round(Number(rawVal));
    if (!Number.isFinite(val)) return;
    const nextAdvance = field === "advance_width" ? val : (selectedGlyph.advance_width ?? 600);
    const nextLsb = field === "lsb" ? val : (selectedGlyph.lsb ?? 0);
    onUpdateMetrics(activeKey, { advance_width: nextAdvance, lsb: nextLsb });
  }

  // ---- Kerning Mutation Helpers ----
  function setKerningPair(leftCp, rightCp, value, { removeZero = false } = {}) {
    if (!project || !onUpdateKerning || leftCp <= 0 || rightCp <= 0) return;
    const val = clampInt(value, -2000, 2000);
    const idx = kerningList.findIndex((p) => p.left === leftCp && p.right === rightCp);
    let next;
    if (removeZero && val === 0) {
      if (idx === -1) return;
      next = kerningList.filter((_, i) => i !== idx);
    } else if (idx >= 0) {
      if (kerningList[idx].value === val) return;
      next = kerningList.map((p, i) => (i === idx ? { ...p, value: val } : p));
    } else {
      next = [...kerningList, { left: leftCp, right: rightCp, value: val }];
    }
    onUpdateKerning(next);
  }

  function updatePairAt(index, rawValue) {
    if (!project || !onUpdateKerning) return;
    const val = Math.round(Number(rawValue));
    if (!Number.isFinite(val)) return;
    const clamped = clampInt(val, -2000, 2000);
    const next = kerningList.map((p, i) => (i === index ? { ...p, value: clamped } : p));
    onUpdateKerning(next);
  }

  function deletePairAt(index) {
    if (!project || !onUpdateKerning) return;
    const next = kerningList.filter((_, i) => i !== index);
    onUpdateKerning(next);
  }

  function addCustomPair(e) {
    e?.preventDefault?.();
    if (!project || !onUpdateKerning || busy) return;
    const lChars = [...newLeftChar];
    const rChars = [...newRightChar];
    const cpL = lChars[0]?.codePointAt(0) ?? 0;
    const cpR = rChars[0]?.codePointAt(0) ?? 0;
    if (cpL <= 0 || cpR <= 0) return;
    const val = Math.round(Number(newKernValue));
    if (!Number.isFinite(val)) return;
    setKerningPair(cpL, cpR, val, { removeZero: false });
  }

  // ---- Strip Pill Scrub & Inline Edit ----
  function focusSelect(node) {
    node.focus();
    node.select();
  }

  function openPairEditor(idx, currentVal) {
    editingPairIndex = idx;
    editingPairDraft = String(currentVal ?? 0);
  }

  function commitPairEditor(cp1, cp2) {
    if (editingPairIndex === null) return;
    const parsed = Math.round(Number(editingPairDraft));
    if (Number.isFinite(parsed)) {
      setKerningPair(cp1, cp2, parsed, { removeZero: true });
    }
    editingPairIndex = null;
    editingPairDraft = "";
  }

  function handlePairDraftInput(cp1, cp2, rawStr) {
    editingPairDraft = rawStr;
    if (rawStr.trim() === "" || rawStr.trim() === "-") return;
    const parsed = Math.round(Number(rawStr));
    if (Number.isFinite(parsed)) {
      setKerningPair(cp1, cp2, parsed, { removeZero: false });
    }
  }

  function handlePairInputKeydown(e, cp1, cp2) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitPairEditor(cp1, cp2);
    } else if (e.key === "Escape") {
      e.preventDefault();
      editingPairIndex = null;
      editingPairDraft = "";
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const step = e.shiftKey ? 10 : 1;
      const base = Math.round(Number(editingPairDraft) || 0);
      const next = e.key === "ArrowUp" ? base + step : base - step;
      editingPairDraft = String(next);
      setKerningPair(cp1, cp2, next, { removeZero: false });
    }
  }

  function onPillPointerDown(e, idx, cp1, cp2, currentVal) {
    if (e.button !== 0 || busy || !project || cp1 <= 0 || cp2 <= 0) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    scrubSession = {
      index: idx,
      cp1,
      cp2,
      startX: e.clientX,
      startVal: currentVal ?? 0,
      moved: false,
    };
  }

  function onPillPointerMove(e, idx) {
    if (!scrubSession || scrubSession.index !== idx) return;
    const dx = e.clientX - scrubSession.startX;
    if (!scrubSession.moved && Math.abs(dx) >= 4) {
      scrubSession.moved = true;
      scrubbingIndex = idx;
    }
    if (scrubSession.moved) {
      const rate = e.shiftKey ? 0.5 : 2;
      const nextVal = clampInt(scrubSession.startVal + dx * rate, -500, 500);
      setKerningPair(scrubSession.cp1, scrubSession.cp2, nextVal, { removeZero: false });
    }
  }

  function onPillPointerUp(e, idx) {
    if (!scrubSession || scrubSession.index !== idx) return;
    const { cp1, cp2, startVal, moved } = scrubSession;
    scrubSession = null;
    scrubbingIndex = null;
    if (moved) {
      const current = kerningMap.get(`${cp1}:${cp2}`) ?? 0;
      if (current === 0) {
        setKerningPair(cp1, cp2, 0, { removeZero: true });
      }
    } else {
      openPairEditor(idx, startVal);
    }
  }

  function onPillPointerCancel(idx) {
    if (scrubSession?.index === idx) {
      scrubSession = null;
      scrubbingIndex = null;
    }
  }

  function selectCharByCp(cp) {
    const entry = glyphByCodepoint.get(cp);
    if (entry?.key) onSelectGlyph?.(entry.key);
  }
</script>

<div class="spacing-kerning-strip">
  <!-- 1. Optical Automation Bar -->
  <div class="auto-bar">
    <div class="auto-group">
      <label class="auto-ctrl" for="sk-target-sb">
        <span class="ctrl-lbl">Target Sidebearing</span>
        <input
          id="sk-target-sb"
          type="range"
          min="20"
          max="160"
          step="2"
          bind:value={targetSidebearing}
          disabled={busy || !project}
        />
        <input
          class="ctrl-num num"
          type="number"
          min="20"
          max="160"
          step="2"
          aria-label="Target Sidebearing value"
          bind:value={targetSidebearing}
          disabled={busy || !project}
        />
      </label>
      <button
        type="button"
        class="btn-inline accent"
        disabled={busy || !project || !onAutoSpace}
        onclick={handleAutoSpace}
      >
        <Sparkles size={12} />
        <span>Optical Auto-Space All</span>
      </button>
    </div>

    <div class="auto-sep" aria-hidden="true"></div>

    <div class="auto-group">
      <label class="auto-ctrl" for="sk-target-gap">
        <span class="ctrl-lbl">Target Optical Gap</span>
        <input
          id="sk-target-gap"
          type="range"
          min="60"
          max="240"
          step="2"
          bind:value={targetOpticalGap}
          disabled={busy || !project}
        />
        <input
          class="ctrl-num num"
          type="number"
          min="60"
          max="240"
          step="2"
          aria-label="Target Optical Gap value"
          bind:value={targetOpticalGap}
          disabled={busy || !project}
        />
      </label>
      <button
        type="button"
        class="btn-inline accent"
        disabled={busy || !project || !onAutoKern}
        onclick={handleAutoKern}
      >
        <ArrowLeftRight size={12} />
        <span>Optical Auto-Kern Pairs</span>
      </button>
    </div>
  </div>

  <!-- 2. In-Context Text Spacing Strip -->
  <div class="strip-section">
    <div class="strip-toolbar">
      <label class="strip-input-wrap">
        <Type size={12} />
        <input
          class="strip-input"
          type="text"
          bind:value={testString}
          placeholder="Type spacing string…"
          aria-label="Test spacing string"
          spellcheck="false"
        />
        {#if testString}
          <button
            type="button"
            class="clear-btn"
            title="Clear string"
            aria-label="Clear string"
            onclick={() => {
              testString = "";
              editingPairIndex = null;
            }}
          >
            <X size={11} />
          </button>
        {/if}
      </label>

      <div class="preset-chips" role="group" aria-label="Sample spacing presets">
        {#each SAMPLE_PRESETS as preset (preset)}
          <button
            type="button"
            class="preset-chip"
            class:active={testString === preset}
            onclick={() => {
              testString = preset;
              editingPairIndex = null;
            }}
          >
            {preset}
          </button>
        {/each}
      </div>
    </div>

    {#if stripCells.length === 0}
      <div class="empty-strip">Enter characters above to inspect sidebearings and kerning pairs in context.</div>
    {:else}
      <div class="glyph-strip" role="list" aria-label="In-context glyph spacing strip">
        {#each stripCells as cell (cell.idx)}
          <div class="strip-item" role="listitem">
            <button
              type="button"
              class="glyph-cell"
              class:active={cell.isActive}
              class:missing={!cell.glyph}
              disabled={!cell.glyph}
              title={cell.glyph
                ? `${cell.ch} · U+${formatCpHex(cell.cp)} · LSB ${cell.glyph.lsb ?? 0} · ADV ${cell.glyph.advance_width ?? 0}`
                : `${cell.ch} · U+${formatCpHex(cell.cp)} (not in project)`}
              onclick={() => cell.key && onSelectGlyph?.(cell.key)}
            >
              <span
                class="cell-glyph"
                style:font-family={`"${liveFamilyId}", var(--sans)`}
              >
                {#if cell.isSpace}
                  <span class="space-mark">␣</span>
                {:else}
                  {cell.ch}
                {/if}
              </span>
              <span class="cell-metrics num">
                <span>LSB {cell.glyph ? (cell.glyph.lsb ?? 0) : "—"}</span>
                <span>ADV {cell.glyph ? (cell.glyph.advance_width ?? 0) : "—"}</span>
              </span>
            </button>

            {#if cell.hasPair}
              {#if editingPairIndex === cell.idx}
                <input
                  use:focusSelect
                  class="kern-pill-input num"
                  type="text"
                  inputmode="numeric"
                  aria-label={`Kerning for ${cell.ch} and next character`}
                  value={editingPairDraft}
                  oninput={(e) => handlePairDraftInput(cell.cp, cell.nextCp, e.currentTarget.value)}
                  onkeydown={(e) => handlePairInputKeydown(e, cell.cp, cell.nextCp)}
                  onblur={() => commitPairEditor(cell.cp, cell.nextCp)}
                />
              {:else}
                <button
                  type="button"
                  class="kern-pill num"
                  class:nonzero={cell.pairValue !== 0}
                  class:negative={cell.pairValue < 0}
                  class:positive={cell.pairValue > 0}
                  class:scrubbing={scrubbingIndex === cell.idx}
                  disabled={busy || !project}
                  title="Drag horizontally to scrub kerning, or click to type a value"
                  aria-label={`Kern pair value ${cell.pairValue}`}
                  onpointerdown={(e) => onPillPointerDown(e, cell.idx, cell.cp, cell.nextCp, cell.pairValue)}
                  onpointermove={(e) => onPillPointerMove(e, cell.idx)}
                  onpointerup={(e) => onPillPointerUp(e, cell.idx)}
                  onpointercancel={() => onPillPointerCancel(cell.idx)}
                  onclick={(e) => {
                    if (e.detail === 0) openPairEditor(cell.idx, cell.pairValue);
                  }}
                >
                  {formatKernValue(cell.pairValue)}
                </button>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- 3 & 4. Selected Glyph Metrics + Kerning Pairs Table -->
  <div class="lower-grid">
    <!-- 3. Selected Glyph Metrics Controls -->
    <div class="sub-panel metrics-panel">
      <div class="sub-head">
        <span class="label">
          <SlidersHorizontal size={11} />
          <span>Selected Glyph Metrics</span>
        </span>
        {#if selectedGlyph}
          <span class="glyph-badge num">
            <span class="badge-char" style:font-family={`"${liveFamilyId}", var(--sans)`}>
              {selectedChar}
            </span>
            <span>U+{formatCpHex(selectedCp)}</span>
          </span>
        {/if}
      </div>

      {#if selectedGlyph}
        <div class="metric-sliders">
          <div class="metric-row">
            <label class="m-lbl" for="sk-lsb-range">LSB</label>
            <input
              id="sk-lsb-range"
              type="range"
              min="-100"
              max="250"
              step="1"
              value={clampInt(selectedGlyph.lsb ?? 0, -100, 250)}
              disabled={busy || !onUpdateMetrics}
              oninput={(e) => updateSelectedMetric("lsb", e.currentTarget.value)}
            />
            <input
              class="m-num num"
              type="number"
              step="1"
              aria-label="Selected glyph LSB"
              value={selectedGlyph.lsb ?? 0}
              disabled={busy || !onUpdateMetrics}
              onchange={(e) => updateSelectedMetric("lsb", e.currentTarget.value)}
            />
          </div>

          <div class="metric-row">
            <label class="m-lbl" for="sk-adv-range">Advance Width</label>
            <input
              id="sk-adv-range"
              type="range"
              min="100"
              max="1200"
              step="1"
              value={clampInt(selectedGlyph.advance_width ?? 600, 100, 1200)}
              disabled={busy || !onUpdateMetrics}
              oninput={(e) => updateSelectedMetric("advance_width", e.currentTarget.value)}
            />
            <input
              class="m-num num"
              type="number"
              step="1"
              aria-label="Selected glyph Advance Width"
              value={selectedGlyph.advance_width ?? 600}
              disabled={busy || !onUpdateMetrics}
              onchange={(e) => updateSelectedMetric("advance_width", e.currentTarget.value)}
            />
          </div>
        </div>
      {:else}
        <div class="empty-note">Select a glyph from the strip or glyph list to adjust LSB and Advance Width.</div>
      {/if}
    </div>

    <!-- 4. Kerning Pairs Table -->
    <div class="sub-panel kern-panel">
      <div class="sub-head">
        <span class="label">
          <ArrowLeftRight size={11} />
          <span>Kerning Pairs · {kerningList.length}</span>
        </span>
        <label class="pair-search">
          <Search size={11} />
          <input
            type="text"
            bind:value={pairFilter}
            placeholder="Filter…"
            aria-label="Filter kerning pairs"
            spellcheck="false"
          />
        </label>
      </div>

      <form class="add-pair-form" onsubmit={addCustomPair}>
        <label class="pair-char-field">
          <span>Left</span>
          <input
            type="text"
            maxlength="2"
            bind:value={newLeftChar}
            aria-label="Left character"
            spellcheck="false"
            disabled={busy || !project}
          />
        </label>
        <label class="pair-char-field">
          <span>Right</span>
          <input
            type="text"
            maxlength="2"
            bind:value={newRightChar}
            aria-label="Right character"
            spellcheck="false"
            disabled={busy || !project}
          />
        </label>
        <label class="pair-val-field">
          <span>Value</span>
          <input
            class="num"
            type="number"
            step="1"
            bind:value={newKernValue}
            aria-label="Kerning value"
            disabled={busy || !project}
          />
        </label>
        <button
          type="submit"
          class="btn-inline accent"
          disabled={busy || !project || !newLeftChar.length || !newRightChar.length || !onUpdateKerning}
        >
          <Plus size={12} />
          <span>Add Pair</span>
        </button>
      </form>

      {#if filteredPairs.length === 0}
        <div class="empty-note">
          {kerningList.length === 0
            ? "No kerning pairs yet. Drag a pill in the strip, run Optical Auto-Kern, or add a pair above."
            : "No kerning pairs match your filter."}
        </div>
      {:else}
        <div class="pairs-list" role="list" aria-label="Kerning pairs list">
          {#each filteredPairs as item (`${item.pair.left}:${item.pair.right}:${item.index}`)}
            <div class="pair-row" role="listitem">
              <div class="pair-glyphs">
                <button
                  type="button"
                  class="pair-glyph-btn"
                  disabled={!glyphByCodepoint.has(item.pair.left)}
                  title={`Select U+${item.leftHex}`}
                  onclick={() => selectCharByCp(item.pair.left)}
                >
                  <span class="pg-ch">{item.leftChar}</span>
                  <span class="pg-hex num">{item.leftHex}</span>
                </button>
                <span class="pair-arrow" aria-hidden="true">→</span>
                <button
                  type="button"
                  class="pair-glyph-btn"
                  disabled={!glyphByCodepoint.has(item.pair.right)}
                  title={`Select U+${item.rightHex}`}
                  onclick={() => selectCharByCp(item.pair.right)}
                >
                  <span class="pg-ch">{item.rightChar}</span>
                  <span class="pg-hex num">{item.rightHex}</span>
                </button>
                <span
                  class="pair-sample"
                  style:font-family={`"${liveFamilyId}", var(--sans)`}
                  title="Live pair preview"
                >
                  {item.leftChar}{item.rightChar}
                </span>
              </div>

              <div class="pair-actions">
                <input
                  class="pair-edit-num num"
                  type="number"
                  step="1"
                  aria-label={`Kerning value for ${item.leftChar}${item.rightChar}`}
                  value={item.pair.value}
                  disabled={busy || !onUpdateKerning}
                  onchange={(e) => updatePairAt(item.index, e.currentTarget.value)}
                />
                <button
                  type="button"
                  class="del-btn"
                  title="Delete kerning pair"
                  aria-label={`Delete pair ${item.leftChar}${item.rightChar}`}
                  disabled={busy || !onUpdateKerning}
                  onclick={() => deletePairAt(item.index)}
                >
                  <Trash size={12} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .spacing-kerning-strip {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ---- 1. Optical Automation Bar ---- */
  .auto-bar {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    align-items: center;
    gap: 8px 14px;
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--win);
    border: 1px solid var(--line);
  }
  .auto-group {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .auto-ctrl {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
  }
  .ctrl-lbl {
    font: 600 11px var(--sans);
    color: var(--ink2);
    white-space: nowrap;
  }
  .auto-ctrl input[type="range"],
  .metric-row input[type="range"] {
    flex: 1;
    min-width: 60px;
    -webkit-appearance: none;
    appearance: none;
    height: 3px;
    border-radius: 3px;
    background: var(--line2);
    outline: none;
  }
  .auto-ctrl input[type="range"]::-webkit-slider-thumb,
  .metric-row input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
  }
  .ctrl-num {
    width: 52px;
    height: 24px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }
  .auto-sep {
    display: none;
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
    transition: border-color 0.14s, color 0.14s, background-color 0.14s;
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
  .btn-inline:disabled {
    opacity: 0.45;
    cursor: default;
  }

  /* ---- 2. In-Context Text Spacing Strip ---- */
  .strip-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border-radius: 9px;
    background: var(--win);
    border: 1px solid var(--line);
  }
  .strip-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .strip-input-wrap {
    flex: 1;
    min-width: 210px;
    height: 28px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--field);
    border: 1px solid var(--line2);
    color: var(--ink3);
  }
  .strip-input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 500 12px var(--sans);
  }
  .clear-btn {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ink3);
    display: grid;
    place-items: center;
  }
  .clear-btn:hover {
    background: var(--hov);
    color: var(--ink);
  }
  .preset-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .preset-chip {
    height: 24px;
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    transition: all 0.14s;
  }
  .preset-chip:hover {
    border-color: var(--line2);
    color: var(--ink);
  }
  .preset-chip.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--ink);
  }

  .glyph-strip {
    display: flex;
    align-items: stretch;
    gap: 4px;
    overflow-x: auto;
    padding: 6px 4px;
    border-radius: 8px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .strip-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .glyph-cell {
    min-width: 58px;
    padding: 6px 7px 5px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    transition: border-color 0.12s, background-color 0.12s;
  }
  .glyph-cell:hover:not(:disabled) {
    border-color: var(--accent-2);
    background: var(--hov);
  }
  .glyph-cell.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .glyph-cell.missing {
    opacity: 0.45;
    cursor: default;
  }
  .cell-glyph {
    font-size: 32px;
    line-height: 1.1;
    min-height: 36px;
    display: grid;
    place-items: center;
    color: var(--ink);
  }
  .space-mark {
    font: 500 14px var(--mono);
    color: var(--ink3);
  }
  .cell-metrics {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    font: 500 9.5px/1.15 var(--mono);
    color: var(--ink3);
    white-space: nowrap;
  }

  .kern-pill {
    height: 22px;
    min-width: 32px;
    padding: 0 6px;
    border-radius: 999px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink3);
    font: 600 10.5px var(--mono);
    cursor: ew-resize;
    touch-action: none;
    user-select: none;
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }
  .kern-pill:hover:not(:disabled) {
    border-color: var(--accent-2);
    color: var(--ink);
  }
  .kern-pill.nonzero {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .kern-pill.positive {
    border-color: var(--ok);
    background: var(--cyan-soft);
    color: var(--ok);
  }
  .kern-pill.scrubbing {
    box-shadow: var(--glow);
    color: var(--ink);
  }
  .kern-pill:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .kern-pill-input {
    width: 48px;
    height: 22px;
    padding: 0 5px;
    border-radius: 999px;
    border: 1px solid var(--accent);
    background: var(--field);
    color: var(--ink);
    font: 600 10.5px var(--mono);
    text-align: center;
  }

  .empty-strip {
    padding: 14px 10px;
    text-align: center;
    font-size: 11.5px;
    color: var(--ink3);
  }

  /* ---- 3 & 4. Lower Panels ---- */
  .lower-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(290px, 1fr));
    gap: 10px;
  }
  .sub-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border-radius: 9px;
    background: var(--win);
    border: 1px solid var(--line);
    min-width: 0;
  }
  .sub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sub-head .label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .glyph-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px;
    border-radius: 6px;
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    color: var(--ink);
    font: 600 10.5px var(--mono);
  }
  .badge-char {
    font-size: 13px;
    line-height: 1;
  }

  .metric-sliders {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 2px;
  }
  .metric-row {
    display: grid;
    grid-template-columns: 92px minmax(0, 1fr) 62px;
    align-items: center;
    gap: 8px;
  }
  .m-lbl {
    font: 600 11px var(--sans);
    color: var(--ink2);
    white-space: nowrap;
  }
  .m-num {
    width: 100%;
    height: 24px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }

  /* ---- 4. Kerning Pairs Table ---- */
  .pair-search {
    height: 22px;
    width: 120px;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 7px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink3);
  }
  .pair-search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 500 11px var(--sans);
  }

  .add-pair-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .pair-char-field,
  .pair-val-field {
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
  .pair-char-field input {
    width: 26px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 12px var(--mono);
    text-align: center;
  }
  .pair-val-field input {
    width: 52px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }

  .pairs-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 190px;
    overflow-y: auto;
    padding-right: 2px;
  }
  .pair-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 7px;
    border-radius: 7px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .pair-glyphs {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }
  .pair-glyph-btn {
    height: 22px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .pair-glyph-btn:hover:not(:disabled) {
    border-color: var(--accent-2);
  }
  .pair-glyph-btn:disabled {
    opacity: 0.65;
    cursor: default;
  }
  .pg-ch {
    font: 600 11.5px var(--sans);
  }
  .pg-hex {
    font: 500 9.5px var(--mono);
    color: var(--ink3);
  }
  .pair-arrow {
    font-size: 11px;
    color: var(--ink3);
  }
  .pair-sample {
    font-size: 15px;
    line-height: 1;
    padding-left: 4px;
    color: var(--ink2);
  }
  .pair-actions {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  .pair-edit-num {
    width: 58px;
    height: 22px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }
  .del-btn {
    width: 22px;
    height: 22px;
    padding: 0;
    border-radius: 5px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink3);
    display: grid;
    place-items: center;
  }
  .del-btn:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }
  .del-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .empty-note {
    padding: 10px 6px;
    font-size: 11.5px;
    color: var(--ink3);
  }
</style>
