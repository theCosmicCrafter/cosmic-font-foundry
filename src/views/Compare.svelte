<script>
  import { ChevronLeft, X, ArrowLeftRight, Copy, Check, Archive } from "@lucide/svelte";
  import SegmentedControl from "../components/SegmentedControl.svelte";
  import { fontStyle, invokeCmd } from "../lib/fonts.svelte.js";
  import { inTauri } from "../lib/settings.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let { families, sample, size, onback, onremove } = $props();

  let mode = $state("stacked"); // "stacked" | "columns" | "pairing"
  let headingIdx = $state(0);
  let bodyIdx = $state(1);
  let headingCopy = $state(t("cmp.headingSample"));
  let bodyCopy = $state(t("cmp.bodySample"));
  let headingSize = $state(42);
  let bodySize = $state(16);
  let scaleRatio = $state(1.25);
  let exportingKit = $state(false);
  let suggestedPairings = $state([]);

  $effect(() => {
    const hName = headingFont?.name;
    suggestedPairings = [];
    if (!hName || !inTauri) return;
    invokeCmd("suggest_font_pairings", { family: hName, limit: 5 })
      .then((list) => {
        if (headingFont?.name === hName) suggestedPairings = list ?? [];
      })
      .catch(() => {});
  });

  async function exportDesignKit() {
    if (!inTauri || exportingKit || !families.length) return;
    exportingKit = true;
    try {
      const hName = headingFont?.name || families[0].name;
      const bName = bodyFont?.name || hName;
      const res = await invokeCmd("export_design_kit_zip", {
        headingFamily: hName,
        bodyFamily: bName,
        basePx: bodySize,
        ratio: scaleRatio,
      });
      if (res) {
        notify("ok", t("cmp.exportedKit"), `${res.font_files} files · ${res.zip_path}`);
      }
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      exportingKit = false;
    }
  }

  $effect(() => {
    if (headingIdx >= families.length) headingIdx = 0;
    if (bodyIdx >= families.length) bodyIdx = Math.min(1, families.length - 1);
  });

  const headingFont = $derived(families[headingIdx] ?? families[0]);
  const bodyFont = $derived(families[bodyIdx] ?? families[Math.min(1, families.length - 1)]);
  const headingStyle = $derived(headingFont ? fontStyle(headingFont) : null);
  const bodyStyle = $derived(bodyFont ? fontStyle(bodyFont) : null);

  function swapPair() {
    const h = headingIdx;
    headingIdx = bodyIdx;
    bodyIdx = h;
  }

  let copiedTokens = $state(false);
  async function copyDesignTokens() {
    const hName = headingFont?.name || families[0]?.name || "sans-serif";
    const bName = bodyFont?.name || families[1]?.name || hName;
    const css = [
      "/* Cosmic Font Foundry — Typography Design Tokens */",
      ":root {",
      `  --font-heading: "${hName}", system-ui, sans-serif;`,
      `  --font-body: "${bName}", system-ui, sans-serif;`,
      `  --size-heading: ${headingSize}px;`,
      `  --size-body: ${bodySize}px;`,
      "}",
      "",
      "/* Tailwind CSS v4 @theme */",
      "@theme {",
      `  --font-heading: "${hName}", system-ui, sans-serif;`,
      `  --font-body: "${bName}", system-ui, sans-serif;`,
      "}",
    ].join("\n");
    try {
      await navigator.clipboard.writeText(css);
      copiedTokens = true;
      setTimeout(() => (copiedTokens = false), 1800);
      notify("ok", t("cmp.copiedTokens"), `${hName} + ${bName}`);
    } catch {
      notify("err", t("err.one"), "");
    }
  }
</script>

<div class="cmp">
  <div class="toolbar">
    <button type="button" class="back" onclick={onback}><ChevronLeft size={13} strokeWidth={2.2} />{t("back")}</button>
    <SegmentedControl
      label={t("compareTitle")}
      options={[
        { value: "stacked", label: t("cmp.stacked") },
        { value: "columns", label: t("cmp.columns") },
        { value: "pairing", label: t("cmp.pairing") },
      ]}
      value={mode}
      onselect={(v) => (mode = v)}
      height={24}
      font="600 11.5px"
    />
    <button type="button" class="tokens-btn" onclick={copyDesignTokens}>
      {#if copiedTokens}
        <Check size={12} strokeWidth={2.2} />{t("agent.copied")}
      {:else}
        <Copy size={12} strokeWidth={2} />{t("cmp.copyTokens")}
      {/if}
    </button>
    {#if inTauri}
      <button type="button" class="tokens-btn" disabled={exportingKit} onclick={exportDesignKit}>
        <Archive size={12} strokeWidth={2} />{t("cmp.exportKit")}
      </button>
    {/if}
  </div>

  {#if mode === "pairing" && headingFont && bodyFont}
    <div class="pair-controls">
      <label class="picker">
        <span class="label">{t("cmp.headingFont")}</span>
        <select bind:value={headingIdx}>
          {#each families as f, idx (f.name)}
            <option value={idx}>{f.name}</option>
          {/each}
        </select>
        <input type="range" min="20" max="76" bind:value={headingSize} />
        <span class="num">{headingSize}px</span>
      </label>

      <button type="button" class="swap" onclick={swapPair} title={t("cmp.swap")}>
        <ArrowLeftRight size={14} strokeWidth={2} />{t("cmp.swap")}
      </button>

      <label class="picker">
        <span class="label">{t("cmp.bodyFont")}</span>
        <select bind:value={bodyIdx}>
          {#each families as f, idx (f.name)}
            <option value={idx}>{f.name}</option>
          {/each}
        </select>
        <input type="range" min="12" max="28" bind:value={bodySize} />
        <span class="num">{bodySize}px</span>
      </label>

      <label class="picker">
        <span class="label">{t("cmp.scaleRatio")}</span>
        <select bind:value={scaleRatio}>
          <option value={1.2}>1.200 Minor Third</option>
          <option value={1.25}>1.250 Major Third</option>
          <option value={1.333}>1.333 Perfect Fourth</option>
          <option value={1.618}>1.618 Golden Ratio</option>
        </select>
      </label>
    </div>

    {#if suggestedPairings.length}
      <div class="pair-suggest">
        <span class="label">{t("cmp.suggestPairing")}:</span>
        {#each suggestedPairings as sp (sp.name)}
          <span class="pair-pill" title={sp.reason}>{sp.name} · {sp.category}</span>
        {/each}
      </div>
    {/if}

    <div class="pair-canvas">
      <textarea
        class="pair-heading"
        rows="2"
        bind:value={headingCopy}
        style:font-family={headingStyle.family}
        style:font-weight={headingStyle.weight}
        style:font-style={headingStyle.italic ? "italic" : "normal"}
        style:font-size="{headingSize}px"
      ></textarea>
      <textarea
        class="pair-body"
        rows="5"
        bind:value={bodyCopy}
        style:font-family={bodyStyle.family}
        style:font-weight={bodyStyle.weight}
        style:font-style={bodyStyle.italic ? "italic" : "normal"}
        style:font-size="{bodySize}px"
      ></textarea>
    </div>
  {:else}
    <div class="items" class:columns={mode === "columns"} style:--cols={families.length}>
      {#each families as f (f.name)}
        {@const st = fontStyle(f)}
        <div class="item">
          <div class="head">
            <span class="name">{f.name}</span>
            <span class="meta">{f.styles.length} {f.styles.length === 1 ? t("style") : t("styles")} · {t("origin." + f.origin)}</span>
            <button type="button" class="remove" title={t("removeCmp")} aria-label={t("removeCmp")} onclick={() => onremove(f.name)}>
              <X size={12} strokeWidth={2} />
            </button>
          </div>
          <span
            class="line"
            style:font-family={st.family}
            style:font-weight={st.weight}
            style:font-style={st.italic ? "italic" : "normal"}
            style:font-size="{mode === 'columns' ? Math.min(Math.max(size, 28), 44) : Math.max(size, 44)}px">{sample}</span
          >
          <span class="paragraph" style:font-family={st.family} style:font-weight={st.weight} style:font-style={st.italic ? "italic" : "normal"}>
            {t("paragraph")}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .cmp { flex: 1; min-height: 0; overflow-y: auto; padding: 0 24px 60px; display: flex; flex-direction: column; gap: 14px; animation: cf-up 0.25s ease-out; }
  .toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .tokens-btn {
    height: 26px; padding: 0 11px; border: 1px solid var(--line2); border-radius: 999px;
    background: var(--field); color: var(--ink); font: 600 11.5px var(--sans);
    display: inline-flex; align-items: center; gap: 5px; transition: border-color 0.15s, background-color 0.15s;
  }
  .tokens-btn:hover { border-color: var(--accent); background: var(--accent-soft); }
  .back {
    flex: none;
    height: 28px;
    padding: 0 13px 0 9px;
    border-radius: 999px;
    border: 0;
    background: var(--chip);
    color: var(--ink);
    font: 600 12.5px var(--sans);
    display: flex;
    align-items: center;
    gap: 4px;
    transition: background-color 0.15s;
  }
  .back:hover { background: var(--line2); }

  .items { display: flex; flex-direction: column; gap: 4px; }
  .items.columns { display: grid; grid-template-columns: repeat(var(--cols, 2), minmax(0, 1fr)); gap: 14px; }
  .item { flex: none; display: flex; flex-direction: column; gap: 6px; padding: 14px 0 18px; border-top: 1px solid var(--line); }
  .items.columns .item { border-top: 0; border: 1px solid var(--line); border-radius: 12px; padding: 14px 16px; background: var(--field); }
  .head { display: flex; align-items: center; gap: 10px; }
  .name { font-size: 12px; font-weight: 600; }
  .meta { font-size: 11.5px; color: var(--ink3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .remove {
    margin-left: auto;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--ink3);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .remove:hover { background: var(--hov); color: var(--ink); }
  .line { line-height: 1.38; white-space: nowrap; overflow-x: clip; overflow-y: visible; text-overflow: ellipsis; }
  .items.columns .line { white-space: normal; overflow-wrap: anywhere; }
  .paragraph { font-size: 15px; line-height: 1.55; color: var(--ink2); max-width: 70ch; }

  .pair-controls { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; padding: 12px 14px; border-radius: 12px; background: var(--side); border: 1px solid var(--line); }
  .picker { display: flex; align-items: center; gap: 8px; flex: 1 1 220px; }
  .picker select { height: 28px; padding: 0 8px; border-radius: 7px; border: 1px solid var(--line2); background: var(--field); color: var(--ink); font-size: 12.5px; }
  .picker input[type="range"] { width: 70px; accent-color: var(--accent); }
  .swap { height: 28px; padding: 0 12px; border: 0; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font: 600 12px var(--sans); display: inline-flex; align-items: center; gap: 6px; }
  .pair-canvas { display: flex; flex-direction: column; gap: 16px; padding: 24px; border-radius: 14px; border: 1px solid var(--line); background: var(--field); }
  .pair-heading, .pair-body { width: 100%; border: 0; background: transparent; resize: vertical; color: var(--ink); line-height: 1.25; }
  .pair-body { line-height: 1.6; color: var(--ink2); }
  .pair-suggest { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 0 2px; }
  .pair-pill { padding: 3px 9px; border-radius: 999px; background: var(--field); border: 1px solid var(--line); font: 500 11.5px var(--sans); color: var(--ink2); }
</style>
