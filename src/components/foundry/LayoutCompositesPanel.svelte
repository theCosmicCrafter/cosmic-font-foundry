<script>
  import {
    Sparkles,
    Languages,
    Link2,
    Combine,
    Plus,
    Trash,
    Pencil,
    Check,
    ArrowRight,
    ArrowLeftRight,
    ChevronDown,
    ChevronRight,
  } from "@lucide/svelte";

  const LATIN_EXT_CHARS = [
    ..."ÁÀÂÄÃÅÉÈÊËÍÌÎÏÑÓÒÔÖÕÚÙÛÜÝáàâäãåéèêëíìîïñóòôöõúùûüýÿÇç",
  ];

  const LIGATURE_PRESETS = ["fi", "fl", "ff", "->", "!=", "==="];

  const SYMBOL_TOKENS = {
    "-": "hyphen",
    ">": "greater",
    "<": "less",
    "=": "equal",
    "!": "exclam",
    "&": "ampersand",
    "|": "bar",
    "+": "plus",
    "*": "asterisk",
    "/": "slash",
    "\\": "backslash",
    ":": "colon",
    ";": "semicolon",
    ".": "period",
    ",": "comma",
    "~": "asciitilde",
    "?": "question",
    "@": "at",
    "#": "numbersign",
    "$": "dollar",
    "%": "percent",
    "^": "asciicircum",
    "_": "underscore",
  };

  let {
    project = null,
    busy = false,
    onGenerateComposites = null,
    onApplyLinkedMetrics = null,
    onUpdateLigatures = null,
    onSelectGlyph = null,
  } = $props();

  // Section collapse state (all open by default)
  let openComposites = $state(true);
  let openLinked = $state(true);
  let openLigatures = $state(true);

  // Custom linked-metric rule form
  let linkTarget = $state("");
  let linkLsbRef = $state("");
  let linkRsbRef = $state("");
  let linkMirror = $state(false);

  // Custom ligature form
  let customLigSeq = $state("");

  function findGlyphEntry(ch) {
    const glyphs = project?.glyphs;
    if (!glyphs || !ch) return null;
    if (glyphs[ch]) return { key: ch, glyph: glyphs[ch] };
    const cp = ch.codePointAt(0);
    if (cp == null) return null;
    const cpKey = String(cp);
    if (glyphs[cpKey]) return { key: cpKey, glyph: glyphs[cpKey] };
    for (const [key, g] of Object.entries(glyphs)) {
      if (g?.codepoint === cp) return { key, glyph: g };
    }
    return null;
  }

  // ---- 1. Latin-Extended Composites ----
  const compositeItems = $derived.by(() => {
    const _glyphs = project?.glyphs;
    return LATIN_EXT_CHARS.map((ch) => {
      const entry = findGlyphEntry(ch);
      const present = Boolean(entry);
      const hasContours = Boolean(entry?.glyph?.contours?.length);
      return {
        ch,
        key: entry?.key ?? ch,
        present,
        hasContours,
      };
    });
  });

  const presentCompositeCount = $derived(
    compositeItems.filter((item) => item.present).length,
  );
  const missingCompositeCount = $derived(
    compositeItems.length - presentCompositeCount,
  );

  async function handleGenerateComposites() {
    if (busy || !project || !onGenerateComposites) return;
    await onGenerateComposites();
  }

  // ---- 2. Linked Sidebearings ----
  const linkedRules = $derived(
    Array.isArray(project?.linked_metrics) ? project.linked_metrics : [],
  );

  function normRule(rule) {
    return {
      target: String(rule?.target ?? rule?.target_char ?? rule?.glyph ?? "").trim(),
      lsb_ref: rule?.lsb_ref ?? rule?.lsb ?? null,
      rsb_ref: rule?.rsb_ref ?? rule?.rsb ?? null,
      mirror: Boolean(rule?.mirror ?? rule?.mirror_lsb_rsb ?? false),
    };
  }

  function mergeLinkedRules(incoming) {
    const byTarget = new Map();
    for (const raw of linkedRules) {
      const r = normRule(raw);
      if (r.target) byTarget.set(r.target, r);
    }
    for (const raw of incoming) {
      const r = normRule(raw);
      if (r.target) byTarget.set(r.target, r);
    }
    return Array.from(byTarget.values());
  }

  const canAddCustomLink = $derived.by(() => {
    const t = linkTarget.trim();
    const lsb = linkLsbRef.trim();
    const rsb = linkRsbRef.trim();
    return Boolean(t && (lsb || rsb || linkMirror));
  });

  async function applyLinkedPreset(kind) {
    if (busy || !project || !onApplyLinkedMetrics) return;
    const incoming =
      kind === "H"
        ? ["I", "E", "F", "B", "D", "L"].map((target) => ({
            target,
            lsb_ref: "H",
            rsb_ref: null,
            mirror: false,
          }))
        : [
            { target: "C", lsb_ref: "O", rsb_ref: null, mirror: false },
            { target: "G", lsb_ref: "O", rsb_ref: null, mirror: false },
            { target: "Q", lsb_ref: "O", rsb_ref: "O", mirror: false },
          ];
    await onApplyLinkedMetrics(mergeLinkedRules(incoming));
  }

  async function handleAddCustomLink(e) {
    e?.preventDefault();
    if (busy || !project || !onApplyLinkedMetrics || !canAddCustomLink) return;
    const newRule = {
      target: linkTarget.trim(),
      lsb_ref: linkLsbRef.trim() || null,
      rsb_ref: linkRsbRef.trim() || null,
      mirror: Boolean(linkMirror),
    };
    const nextRules = mergeLinkedRules([newRule]);
    linkTarget = "";
    linkLsbRef = "";
    linkRsbRef = "";
    linkMirror = false;
    await onApplyLinkedMetrics(nextRules);
  }

  async function handleDeleteLinkedRule(target) {
    if (busy || !project || !onApplyLinkedMetrics) return;
    const nextRules = linkedRules
      .map(normRule)
      .filter((r) => r.target !== target);
    await onApplyLinkedMetrics(nextRules);
  }

  async function handleApplyAllLinked() {
    if (busy || !project || !onApplyLinkedMetrics || !linkedRules.length) return;
    await onApplyLinkedMetrics(linkedRules.map(normRule));
  }

  // ---- 3. OpenType Ligatures ----
  const ligatureRules = $derived(
    Array.isArray(project?.ligatures) ? project.ligatures : [],
  );

  function buildLigatureKey(seq) {
    const chars = [...seq];
    if (chars.every((c) => /^[a-zA-Z0-9]$/.test(c))) {
      return chars.join("_");
    }
    const tokens = chars.map((c) => {
      if (/^[a-zA-Z0-9]$/.test(c)) return c;
      if (SYMBOL_TOKENS[c]) return SYMBOL_TOKENS[c];
      const cp = c.codePointAt(0) ?? 0;
      return `u${cp.toString(16).toUpperCase().padStart(4, "0")}`;
    });
    return `lig_${tokens.join("_")}`;
  }

  function ruleSequence(rule) {
    if (Array.isArray(rule?.components) && rule.components.length) {
      return rule.components
        .map((cp) => {
          try {
            return String.fromCodePoint(Number(cp));
          } catch {
            return "";
          }
        })
        .join("");
    }
    return rule?.name ?? rule?.glyph_key ?? "";
  }

  function hasLigatureForSeq(seq) {
    const glyphKey = buildLigatureKey(seq);
    const cps = [...seq].map((c) => c.codePointAt(0));
    return ligatureRules.some((r) => {
      if (r?.glyph_key === glyphKey) return true;
      if (!Array.isArray(r?.components) || r.components.length !== cps.length) {
        return false;
      }
      return r.components.every((cp, idx) => Number(cp) === cps[idx]);
    });
  }

  function cloneContourWithOffsetX(contour, dx) {
    return {
      closed: Boolean(contour?.closed ?? true),
      anchors: (contour?.anchors ?? []).map((a) => ({
        x: Number(a.x ?? 0) + dx,
        y: Number(a.y ?? 0),
        h_in: [Number(a.h_in?.[0] ?? 0), Number(a.h_in?.[1] ?? 0)],
        h_out: [Number(a.h_out?.[0] ?? 0), Number(a.h_out?.[1] ?? 0)],
        smooth: Boolean(a.smooth),
      })),
    };
  }

  function buildLigatureGlyphDraft(seq, glyphKey) {
    const upm = project?.metrics?.upm || 1000;
    const chars = [...seq];
    const entries = chars.map((ch) => findGlyphEntry(ch));
    const allFound = entries.every((entry) => Boolean(entry?.glyph));

    let contours = [];
    let advanceWidth = Math.round(upm * 0.75);
    let lsb = 40;

    if (allFound) {
      let cursorX = 0;
      const merged = [];
      for (const entry of entries) {
        const g = entry.glyph;
        for (const c of g.contours ?? []) {
          merged.push(cloneContourWithOffsetX(c, cursorX));
        }
        const adv = Number(g.advance_width);
        cursorX += Number.isFinite(adv) && adv > 0 ? adv : Math.round(upm * 0.55);
      }
      if (merged.length > 0) {
        contours = merged;
        advanceWidth = Math.max(Math.round(cursorX), 200);
        const firstLsb = Number(entries[0]?.glyph?.lsb);
        lsb = Number.isFinite(firstLsb) ? Math.round(firstLsb) : 40;
      }
    }

    return {
      key: glyphKey,
      glyph: {
        codepoint: 0,
        name: glyphKey,
        advance_width: advanceWidth,
        lsb,
        contours,
      },
    };
  }

  function addLigature(rawSeq) {
    if (busy || !project || !onUpdateLigatures) return;
    const seq = rawSeq.trim();
    const chars = [...seq];
    if (chars.length < 2) return;

    const components = chars.map((c) => c.codePointAt(0));
    const glyph_key = buildLigatureKey(seq);
    if (hasLigatureForSeq(seq)) return;

    const nextRule = {
      components,
      glyph_key,
      name: glyph_key,
    };
    const nextLigatures = [...ligatureRules, nextRule];
    const optionalNewGlyphEntry = project?.glyphs?.[glyph_key]
      ? null
      : buildLigatureGlyphDraft(seq, glyph_key);

    onUpdateLigatures(nextLigatures, optionalNewGlyphEntry);
  }

  function handleCustomLigSubmit(e) {
    e?.preventDefault();
    const seq = customLigSeq.trim();
    if ([...seq].length < 2 || hasLigatureForSeq(seq)) return;
    addLigature(seq);
    customLigSeq = "";
  }

  function handleDeleteLigature(glyphKey) {
    if (busy || !project || !onUpdateLigatures) return;
    const filtered = ligatureRules.filter((r) => r?.glyph_key !== glyphKey);
    onUpdateLigatures(filtered, null);
  }

  const canAddCustomLig = $derived.by(() => {
    const seq = customLigSeq.trim();
    return [...seq].length >= 2 && !hasLigatureForSeq(seq);
  });
</script>

<div class="layout-composites-panel">
  <!-- 1. 1-Click Latin-Extended Auto-Composites -->
  <section class="lc-section">
    <button
      type="button"
      class="lc-head"
      onclick={() => (openComposites = !openComposites)}
      aria-expanded={openComposites}
    >
      <span class="lc-title">
        {#if openComposites}
          <ChevronDown size={13} />
        {:else}
          <ChevronRight size={13} />
        {/if}
        <Languages size={13} />
        <span>Latin-Ext Auto-Composites</span>
      </span>
      <span class="lc-badge num">
        {presentCompositeCount}/{compositeItems.length} present
        {#if missingCompositeCount > 0}
          · {missingCompositeCount} missing
        {/if}
      </span>
    </button>

    {#if openComposites}
      <div class="lc-body">
        <div class="lc-action-row">
          <span class="lc-hint">
            Build accented uppercase &amp; lowercase from base glyphs + diacritics.
          </span>
          <button
            type="button"
            class="btn-inline primary"
            disabled={busy || !project || !onGenerateComposites}
            onclick={handleGenerateComposites}
          >
            <Sparkles size={12} />
            <span>Generate Accented Latin-Ext</span>
          </button>
        </div>

        <div class="comp-grid" role="group" aria-label="Latin-Extended accented glyphs">
          {#each compositeItems as item (item.ch)}
            <button
              type="button"
              class="comp-cell"
              class:present={item.present}
              class:empty-outline={item.present && !item.hasContours}
              disabled={!item.present || !onSelectGlyph}
              title={item.present
                ? `Jump to ${item.ch} (U+${item.ch.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")})`
                : `${item.ch} — not in font yet`}
              onclick={() => onSelectGlyph?.(item.key)}
            >
              {item.ch}
            </button>
          {/each}
        </div>
      </div>
    {/if}
  </section>

  <!-- 2. Linked Sidebearings -->
  <section class="lc-section">
    <button
      type="button"
      class="lc-head"
      onclick={() => (openLinked = !openLinked)}
      aria-expanded={openLinked}
    >
      <span class="lc-title">
        {#if openLinked}
          <ChevronDown size={13} />
        {:else}
          <ChevronRight size={13} />
        {/if}
        <Link2 size={13} />
        <span>Linked Sidebearings</span>
      </span>
      <span class="lc-badge num">{linkedRules.length} rules</span>
    </button>

    {#if openLinked}
      <div class="lc-body">
        <div class="preset-row">
          <button
            type="button"
            class="btn-inline"
            disabled={busy || !project || !onApplyLinkedMetrics}
            onclick={() => applyLinkedPreset("H")}
          >
            <Link2 size={11} />
            <span>Link H → I, E, F, B, D, L</span>
          </button>
          <button
            type="button"
            class="btn-inline"
            disabled={busy || !project || !onApplyLinkedMetrics}
            onclick={() => applyLinkedPreset("O")}
          >
            <Link2 size={11} />
            <span>Link O → C, G, Q</span>
          </button>
        </div>

        <form class="link-form" onsubmit={handleAddCustomLink}>
          <label class="field-chip">
            <span>Target</span>
            <input
              type="text"
              bind:value={linkTarget}
              placeholder="I"
              spellcheck="false"
              aria-label="Target char"
            />
          </label>
          <label class="field-chip">
            <span>LSB ref</span>
            <input
              type="text"
              bind:value={linkLsbRef}
              placeholder="H"
              spellcheck="false"
              aria-label="LSB ref"
            />
          </label>
          <label class="field-chip">
            <span>RSB ref</span>
            <input
              type="text"
              bind:value={linkRsbRef}
              placeholder="H"
              spellcheck="false"
              aria-label="RSB ref"
            />
          </label>
          <label class="toggle-chip" class:on={linkMirror}>
            <input type="checkbox" bind:checked={linkMirror} />
            <ArrowLeftRight size={11} />
            <span>Mirror LSB↔RSB</span>
          </label>
          <button
            type="submit"
            class="btn-inline accent"
            disabled={busy || !project || !onApplyLinkedMetrics || !canAddCustomLink}
          >
            <Plus size={12} />
            <span>Add &amp; Apply</span>
          </button>
        </form>

        {#if linkedRules.length > 0}
          <div class="rule-list">
            {#each linkedRules as rawRule, idx (`${rawRule.target ?? rawRule.target_char ?? rawRule.glyph ?? ""}_${idx}`)}
              {@const rule = normRule(rawRule)}
              {@const targetEntry = findGlyphEntry(rule.target)}
              <div class="rule-row">
                <button
                  type="button"
                  class="rule-glyph-pill"
                  disabled={!targetEntry || !onSelectGlyph}
                  title={targetEntry ? `Edit ${rule.target}` : rule.target}
                  onclick={() => targetEntry && onSelectGlyph?.(targetEntry.key)}
                >
                  {rule.target}
                </button>
                <div class="rule-meta num">
                  {#if rule.lsb_ref}
                    <span class="meta-tag">LSB ← {rule.lsb_ref}</span>
                  {/if}
                  {#if rule.rsb_ref}
                    <span class="meta-tag">RSB ← {rule.rsb_ref}</span>
                  {/if}
                  {#if rule.mirror}
                    <span class="meta-tag mirror">
                      <ArrowLeftRight size={10} />
                      Mirror
                    </span>
                  {/if}
                </div>
                <button
                  type="button"
                  class="icon-btn danger"
                  disabled={busy || !onApplyLinkedMetrics}
                  title="Remove linked metric rule"
                  aria-label={`Remove linked metric rule for ${rule.target}`}
                  onclick={() => handleDeleteLinkedRule(rule.target)}
                >
                  <Trash size={11} />
                </button>
              </div>
            {/each}
          </div>

          <div class="lc-footer-end">
            <button
              type="button"
              class="btn-inline primary"
              disabled={busy || !project || !onApplyLinkedMetrics}
              onclick={handleApplyAllLinked}
            >
              <Check size={12} />
              <span>Apply Linked Metrics</span>
            </button>
          </div>
        {:else}
          <div class="empty-msg">
            No linked sidebearings yet. Use a preset or add a custom rule above.
          </div>
        {/if}
      </div>
    {/if}
  </section>

  <!-- 3. OpenType Ligatures (GSUB liga) -->
  <section class="lc-section">
    <button
      type="button"
      class="lc-head"
      onclick={() => (openLigatures = !openLigatures)}
      aria-expanded={openLigatures}
    >
      <span class="lc-title">
        {#if openLigatures}
          <ChevronDown size={13} />
        {:else}
          <ChevronRight size={13} />
        {/if}
        <Combine size={13} />
        <span>OpenType Ligatures (GSUB liga)</span>
      </span>
      <span class="lc-badge num">{ligatureRules.length} active</span>
    </button>

    {#if openLigatures}
      <div class="lc-body">
        <div class="lig-controls">
          <div class="preset-row">
            {#each LIGATURE_PRESETS as seq (seq)}
              {@const exists = hasLigatureForSeq(seq)}
              <button
                type="button"
                class="btn-inline mono"
                class:accent={exists}
                disabled={busy || !project || !onUpdateLigatures || exists}
                title={exists ? `${seq} already added` : `Add ${seq} ligature`}
                onclick={() => addLigature(seq)}
              >
                {#if exists}
                  <Check size={10} />
                {:else}
                  <Plus size={10} />
                {/if}
                <span>{seq}</span>
              </button>
            {/each}
          </div>

          <form class="lig-form" onsubmit={handleCustomLigSubmit}>
            <label class="field-chip wide">
              <span>Seq</span>
              <input
                type="text"
                bind:value={customLigSeq}
                placeholder="ffi"
                spellcheck="false"
                aria-label="Custom ligature sequence"
              />
            </label>
            <button
              type="submit"
              class="btn-inline accent"
              disabled={busy || !project || !onUpdateLigatures || !canAddCustomLig}
            >
              <Plus size={12} />
              <span>Add Ligature</span>
            </button>
          </form>
        </div>

        {#if ligatureRules.length > 0}
          <div class="rule-list">
            {#each ligatureRules as rule, idx (`${rule.glyph_key ?? ""}_${idx}`)}
              {@const seq = ruleSequence(rule)}
              {@const glyphObj = project?.glyphs?.[rule.glyph_key]}
              <div class="rule-row">
                <span class="lig-seq mono">{seq}</span>
                <span class="arrow-muted"><ArrowRight size={11} /></span>
                <span class="meta-tag mono">{rule.glyph_key}</span>
                <span class="lig-contours num">
                  {glyphObj?.contours?.length ?? 0} contours
                </span>
                <div class="rule-actions">
                  <button
                    type="button"
                    class="btn-inline"
                    disabled={!glyphObj || !onSelectGlyph}
                    onclick={() => onSelectGlyph?.(rule.glyph_key)}
                  >
                    <Pencil size={11} />
                    <span>Edit Glyph</span>
                  </button>
                  <button
                    type="button"
                    class="icon-btn danger"
                    disabled={busy || !onUpdateLigatures}
                    title="Delete ligature"
                    aria-label={`Delete ligature ${seq}`}
                    onclick={() => handleDeleteLigature(rule.glyph_key)}
                  >
                    <Trash size={11} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="empty-msg">
            No ligatures defined. Quick-add standard pairs or coding ligatures above.
          </div>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .layout-composites-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .lc-section {
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--win);
    overflow: hidden;
  }

  .lc-head {
    width: 100%;
    padding: 7px 10px;
    border: 0;
    background: transparent;
    color: var(--ink2);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font: 600 11.5px var(--sans);
  }
  .lc-head:hover {
    background: var(--hov);
    color: var(--ink);
  }

  .lc-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--ink);
  }

  .lc-badge {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }

  .lc-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 9px 10px;
    border-top: 1px solid var(--line);
    background: var(--side);
  }

  .lc-action-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .lc-hint {
    font-size: 11px;
    color: var(--ink3);
  }

  .comp-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(26px, 1fr));
    gap: 3px;
  }

  .comp-cell {
    height: 26px;
    padding: 0;
    border-radius: 5px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink3);
    font: 600 12px var(--sans);
    display: grid;
    place-items: center;
    opacity: 0.42;
    cursor: default;
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }
  .comp-cell.present {
    opacity: 1;
    color: var(--ink);
    border-color: var(--line2);
    cursor: pointer;
  }
  .comp-cell.present.empty-outline {
    color: var(--ink2);
    border-style: dashed;
  }
  .comp-cell.present:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
  }

  .preset-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }

  .link-form,
  .lig-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .lig-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .field-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 7px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line2);
    font: 600 10.5px var(--sans);
    color: var(--ink2);
  }
  .field-chip input {
    width: 34px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 11.5px var(--mono);
    text-align: center;
  }
  .field-chip.wide input {
    width: 64px;
    text-align: left;
  }

  .toggle-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 10.5px var(--sans);
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
  .btn-inline.mono {
    font-family: var(--mono);
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
  .btn-inline.primary:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn-inline:disabled {
    opacity: 0.48;
    cursor: default;
  }

  .rule-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
    padding-right: 2px;
  }

  .rule-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 7px;
    border-radius: 7px;
    background: var(--field);
    border: 1px solid var(--line);
    min-width: 0;
  }

  .rule-glyph-pill {
    min-width: 24px;
    height: 22px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--line2);
    background: var(--win);
    color: var(--ink);
    font: 700 11.5px var(--mono);
    display: inline-grid;
    place-items: center;
  }
  .rule-glyph-pill:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .rule-glyph-pill:disabled {
    cursor: default;
  }

  .rule-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }

  .meta-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--chip);
    color: var(--ink2);
    font: 500 10.5px var(--mono);
  }
  .meta-tag.mirror {
    background: var(--cyan-soft);
    color: var(--accent-2);
  }

  .lig-seq {
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    color: var(--ink);
    font: 700 11.5px var(--mono);
  }

  .lig-contours {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
    margin-left: auto;
  }

  .rule-actions {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .icon-btn {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--win);
    color: var(--ink3);
    display: grid;
    place-items: center;
    margin-left: auto;
  }
  .rule-actions .icon-btn {
    margin-left: 0;
  }
  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
    border-color: var(--danger);
    background: rgba(251, 113, 133, 0.12);
  }
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .lc-footer-end {
    display: flex;
    justify-content: flex-end;
  }

  .empty-msg {
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--win);
    border: 1px dashed var(--line);
    color: var(--ink3);
    font-size: 11px;
  }

  .arrow-muted {
    display: inline-flex;
    align-items: center;
    color: var(--ink3);
    flex: none;
  }
</style>
