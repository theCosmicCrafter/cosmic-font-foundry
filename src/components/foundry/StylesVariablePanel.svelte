<script>
  import {
    WandSparkles,
    SlidersHorizontal,
    Download,
    GitBranch,
    Layers,
    Sparkles,
    Check,
  } from "@lucide/svelte";

  const STYLE_PRESETS = [
    {
      kind: "bold",
      label: "Bold",
      min: 10,
      max: 48,
      step: 1,
      defaultAmount: 24,
      unit: "u",
      summary: "Offset outer contours to thicken stems",
    },
    {
      kind: "light",
      label: "Light",
      min: 6,
      max: 32,
      step: 1,
      defaultAmount: 16,
      unit: "u",
      summary: "Inset contours to thin stroke weight",
    },
    {
      kind: "condensed",
      label: "Condensed",
      min: 0.65,
      max: 0.92,
      step: 0.01,
      defaultAmount: 0.78,
      unit: "×",
      summary: "Compress horizontal width & advance",
    },
    {
      kind: "extended",
      label: "Extended",
      min: 1.08,
      max: 1.45,
      step: 0.01,
      defaultAmount: 1.24,
      unit: "×",
      summary: "Expand horizontal counters & spacing",
    },
    {
      kind: "small_caps",
      label: "SmallCaps",
      min: 0.65,
      max: 0.85,
      step: 0.01,
      defaultAmount: 0.74,
      unit: "×",
      summary: "Synthesize scaled small-cap lowercase",
    },
    {
      kind: "oblique",
      label: "Oblique",
      min: -18,
      max: 20,
      step: 0.5,
      defaultAmount: 10,
      unit: "°",
      summary: "Shear contours along italic slant angle",
    },
    {
      kind: "inline",
      label: "Inline",
      min: 8,
      max: 32,
      step: 1,
      defaultAmount: 18,
      unit: "u",
      summary: "Carve hollow inner inline stroke",
    },
  ];

  let {
    project = null,
    busy = false,
    onGenerateStyle = null,
    onExportVariableTtf = null,
  } = $props();

  let selectedKind = $state("bold");
  let amounts = $state({
    bold: 24,
    light: 16,
    condensed: 0.78,
    extended: 1.24,
    small_caps: 0.74,
    oblique: 10,
    inline: 18,
  });

  let minWght = $state(200);
  let maxWght = $state(800);
  let lightOffset = $state(-16);
  let boldOffset = $state(28);
  let includeNamedStops = $state(true);

  const glyphCount = $derived(
    project?.glyphs ? Object.keys(project.glyphs).length : 0,
  );

  const axisStops = $derived.by(() => {
    const min = clampInt(minWght, 100, 399);
    const max = clampInt(maxWght, 401, 900);
    const stops = [
      { wght: min, label: "Light", isDefault: false },
      { wght: 400, label: "Regular", isDefault: true },
    ];
    if (includeNamedStops) {
      if (max > 500) stops.push({ wght: 500, label: "Medium", isDefault: false });
      if (max > 600) stops.push({ wght: 600, label: "SemiBold", isDefault: false });
    }
    stops.push({ wght: max, label: "Bold", isDefault: false });
    return stops;
  });

  function clampInt(val, min, max) {
    const n = Math.round(Number(val));
    if (!Number.isFinite(n)) return min;
    return Math.min(max, Math.max(min, n));
  }

  function clampNum(val, min, max) {
    const n = Number(val);
    if (!Number.isFinite(n)) return min;
    return Math.min(max, Math.max(min, n));
  }

  function formatAmount(preset, val) {
    const n = Number(val);
    if (!Number.isFinite(n)) return "";
    if (preset.step < 1) {
      return `${n.toFixed(preset.step === 0.5 ? 1 : 2)}${preset.unit}`;
    }
    return `${Math.round(n)}${preset.unit}`;
  }

  function specimenStyle(kind, val) {
    const n = Number(val) || 0;
    switch (kind) {
      case "bold": {
        const w = Math.round(600 + ((n - 10) / 38) * 300);
        return `font-weight: ${w}; letter-spacing: -0.02em;`;
      }
      case "light": {
        const op = (0.95 - ((n - 6) / 26) * 0.3).toFixed(2);
        return `font-weight: 300; opacity: ${op};`;
      }
      case "condensed":
        return `transform: scaleX(${n.toFixed(2)}); font-weight: 600;`;
      case "extended":
        return `transform: scaleX(${Math.min(n, 1.35).toFixed(2)}); font-weight: 600;`;
      case "small_caps":
        return `font-variant-caps: all-small-caps; font-weight: 600; letter-spacing: 0.04em;`;
      case "oblique":
        return `transform: skewX(${(-n).toFixed(1)}deg); font-weight: 600;`;
      case "inline":
        return `font-weight: 800; color: transparent; -webkit-text-stroke: 1.25px var(--accent);`;
      default:
        return "";
    }
  }

  function updateAmount(preset, rawVal) {
    const next = clampNum(rawVal, preset.min, preset.max);
    amounts = {
      ...amounts,
      [preset.kind]: preset.step === 0.01
        ? Number(next.toFixed(2))
        : preset.step === 0.5
          ? Math.round(next * 2) / 2
          : Math.round(next),
    };
  }

  async function handleGenerate(preset) {
    if (busy || !project || !onGenerateStyle) return;
    selectedKind = preset.kind;
    const val = clampNum(amounts[preset.kind], preset.min, preset.max);
    await onGenerateStyle(preset.kind, val);
  }

  async function handleExportVariable() {
    if (busy || !project || !onExportVariableTtf) return;
    const cleanMin = clampInt(minWght, 100, 399);
    const cleanMax = clampInt(maxWght, 401, 900);
    const cleanLight = -Math.abs(clampInt(lightOffset, -48, -2));
    const cleanBold = Math.abs(clampInt(boldOffset, 4, 64));
    minWght = cleanMin;
    maxWght = cleanMax;
    lightOffset = cleanLight;
    boldOffset = cleanBold;
    await onExportVariableTtf({
      minWght: cleanMin,
      maxWght: cleanMax,
      lightOffset: cleanLight,
      boldOffset: cleanBold,
      includeNamedStops,
    });
  }
</script>

<div class="styles-variable-panel">
  <!-- Section 1: Multi-Style Variant Generator -->
  <section class="sv-section">
    <div class="sv-head">
      <div class="sv-title">
        <WandSparkles size={13} />
        <span>Multi-Style Variant Generator</span>
      </div>
      <span class="sv-badge num">
        {project ? `${glyphCount} glyphs ready` : "No vector project"}
      </span>
    </div>

    <div class="sv-body">
      <p class="sv-hint">
        Synthesize parametric style variants directly from your current vector outlines with full undo support.
      </p>

      <div class="sv-grid">
        {#each STYLE_PRESETS as preset (preset.kind)}
          {@const isSelected = selectedKind === preset.kind}
          {@const currentVal = amounts[preset.kind]}
          <div class="sv-card" class:selected={isSelected}>
            <button
              type="button"
              class="sv-card-select"
              onclick={() => (selectedKind = preset.kind)}
            >
              <span class="sv-card-top">
                <span class="sv-card-name">
                  <Layers size={11} />
                  <span>{preset.label}</span>
                </span>
                <span class="sv-specimen" style={specimenStyle(preset.kind, currentVal)}>
                  {preset.kind === "small_caps" ? "AA" : "Aa"}
                </span>
              </span>
              <span class="sv-card-desc">{preset.summary}</span>
            </button>

            <div class="sv-slider-row">
              <span class="sv-lbl">
                <SlidersHorizontal size={10} />
                <span>Strength</span>
              </span>
              <input
                type="range"
                min={preset.min}
                max={preset.max}
                step={preset.step}
                value={currentVal}
                aria-label={`${preset.label} strength`}
                oninput={(e) => {
                  selectedKind = preset.kind;
                  updateAmount(preset, e.currentTarget.value);
                }}
              />
              <span class="sv-val num">{formatAmount(preset, currentVal)}</span>
            </div>

            <div class="sv-card-foot">
              <span class="sv-kind-tag">{preset.kind}</span>
              <button
                type="button"
                class="btn-inline"
                class:primary={isSelected}
                disabled={busy || !project || !onGenerateStyle}
                onclick={() => handleGenerate(preset)}
              >
                <Sparkles size={11} />
                <span>Generate Style</span>
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Section 2: Variable Font (wght) Compiler -->
  <section class="sv-section">
    <div class="sv-head">
      <div class="sv-title">
        <GitBranch size={13} />
        <span>Variable Font (<code>wght</code>) Compiler</span>
      </div>
      <span class="sv-badge">fvar + gvar .TTF</span>
    </div>

    <div class="sv-body">
      <p class="sv-hint">
        Compile a 3-master OpenType Variable font around the current Regular master (wght 400).
      </p>

      <div class="vf-masters">
        <div class="vf-master-card">
          <div class="vf-master-head">
            <span class="vf-master-role">Min Master</span>
            <span class="vf-master-tag num">wght {minWght}</span>
          </div>
          <label class="vf-field">
            <span>Min Weight</span>
            <input
              type="number"
              min={100}
              max={399}
              step={50}
              bind:value={minWght}
              disabled={busy}
            />
          </label>
          <label class="vf-field">
            <span>Light Offset</span>
            <input
              type="number"
              min={-48}
              max={-2}
              step={1}
              bind:value={lightOffset}
              disabled={busy}
            />
          </label>
        </div>

        <div class="vf-master-card default">
          <div class="vf-master-head">
            <span class="vf-master-role">Default Master</span>
            <span class="vf-master-tag accent num">wght 400</span>
          </div>
          <div class="vf-default-pill">
            <Check size={12} />
            <span>Regular = Current Project</span>
          </div>
          <span class="vf-default-note">
            All outlines serve as the 400 base master for delta interpolation.
          </span>
        </div>

        <div class="vf-master-card">
          <div class="vf-master-head">
            <span class="vf-master-role">Max Master</span>
            <span class="vf-master-tag num">wght {maxWght}</span>
          </div>
          <label class="vf-field">
            <span>Max Weight</span>
            <input
              type="number"
              min={401}
              max={900}
              step={50}
              bind:value={maxWght}
              disabled={busy}
            />
          </label>
          <label class="vf-field">
            <span>Bold Offset</span>
            <input
              type="number"
              min={4}
              max={64}
              step={1}
              bind:value={boldOffset}
              disabled={busy}
            />
          </label>
        </div>
      </div>

      <div class="vf-axis-bar" aria-label="Weight axis stops">
        {#each axisStops as stop (stop.wght)}
          <div class="vf-stop" class:default={stop.isDefault}>
            <span class="vf-stop-num num">{stop.wght}</span>
            <span class="vf-stop-lbl">{stop.label}</span>
          </div>
        {/each}
      </div>

      <div class="vf-footer">
        <label class="toggle-chip" class:on={includeNamedStops}>
          <input
            type="checkbox"
            bind:checked={includeNamedStops}
            disabled={busy}
          />
          <span>Include Medium (500) / SemiBold (600) named instance stops</span>
        </label>

        <button
          type="button"
          class="btn-inline primary"
          disabled={busy || !project || !onExportVariableTtf}
          onclick={handleExportVariable}
        >
          <Download size={12} />
          <span>Export Variable .TTF</span>
        </button>
      </div>
    </div>
  </section>
</div>

<style>
  .styles-variable-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .sv-section {
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--win);
    overflow: hidden;
  }

  .sv-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    background: var(--win);
  }

  .sv-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 600 12px var(--sans);
    color: var(--ink);
  }

  .sv-title code {
    font: 600 11px var(--mono);
    padding: 0 4px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .sv-badge {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }

  .sv-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 10px;
    background: var(--surface, var(--side));
  }

  .sv-hint {
    margin: 0;
    font: 400 11px/1.4 var(--sans);
    color: var(--ink3);
  }

  .sv-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 8px;
  }

  .sv-card {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px 9px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--field);
    transition: border-color 0.14s, background-color 0.14s, box-shadow 0.14s;
  }

  .sv-card:hover {
    border-color: var(--line2);
  }

  .sv-card.selected {
    border-color: var(--accent);
    background: var(--win);
    box-shadow: 0 0 0 1px var(--accent-soft);
  }

  .sv-card-select {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0;
    border: 0;
    background: transparent;
    text-align: left;
    color: inherit;
    width: 100%;
  }

  .sv-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .sv-card-name {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 12px var(--sans);
    color: var(--ink);
  }

  .sv-specimen {
    display: inline-grid;
    place-items: center;
    min-width: 28px;
    height: 22px;
    padding: 0 5px;
    border-radius: 5px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    font: 600 13px/1 var(--sans);
    color: var(--accent);
    transform-origin: center;
  }

  .sv-card-desc {
    font: 400 10.5px/1.3 var(--sans);
    color: var(--ink3);
  }

  .sv-slider-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .sv-lbl {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font: 600 10px var(--sans);
    color: var(--ink2);
    white-space: nowrap;
  }

  .sv-slider-row input[type="range"] {
    flex: 1;
    min-width: 50px;
    -webkit-appearance: none;
    appearance: none;
    height: 3px;
    border-radius: 3px;
    background: var(--line2);
    outline: none;
  }

  .sv-slider-row input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
  }

  .sv-val {
    min-width: 38px;
    font: 600 10.5px var(--mono);
    color: var(--ink);
    text-align: right;
  }

  .sv-card-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding-top: 2px;
  }

  .sv-kind-tag {
    font: 500 9.5px var(--mono);
    color: var(--ink3);
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
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

  .btn-inline.primary:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .btn-inline:disabled {
    opacity: 0.48;
    cursor: default;
  }

  /* ---- Variable Font Compiler ---- */
  .vf-masters {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .vf-master-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 9px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--field);
  }

  .vf-master-card.default {
    border-color: var(--accent);
    background: var(--win);
  }

  .vf-master-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .vf-master-role {
    font: 600 11px var(--sans);
    color: var(--ink);
  }

  .vf-master-tag {
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    font: 600 10px var(--mono);
    color: var(--ink2);
  }

  .vf-master-tag.accent {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }

  .vf-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font: 500 11px var(--sans);
    color: var(--ink2);
  }

  .vf-field input {
    width: 68px;
    height: 24px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid var(--line2);
    background: var(--win);
    color: var(--ink);
    font: 600 11px var(--mono);
    text-align: right;
  }

  .vf-default-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 7px;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--ink);
    font: 600 11px var(--sans);
  }

  .vf-default-note {
    font: 400 10.5px/1.3 var(--sans);
    color: var(--ink3);
  }

  .vf-axis-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 7px;
    border: 1px solid var(--line);
    background: var(--win);
  }

  .vf-stop {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--field);
    font-size: 10.5px;
  }

  .vf-stop.default {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .vf-stop-num {
    font: 700 10.5px var(--mono);
    color: var(--ink);
  }

  .vf-stop-lbl {
    font: 500 10.5px var(--sans);
    color: var(--ink2);
  }

  .vf-footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .toggle-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 24px;
    padding: 2px 8px;
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
</style>
