<script>
  import { ChevronLeft, FolderOpen, Copy, RotateCcw, ShieldAlert, Plus, X, Download, Check } from "@lucide/svelte";
  import HeartIcon from "../components/HeartIcon.svelte";
  import { fontStyle, fontCharacters, revealInExplorer, quarantineCorruptFont } from "../lib/fonts.svelte.js";
  import { tagsForFamily, addTagToFamily, removeTagFromFamily } from "../lib/library.svelte.js";
  import { loadGoogleFullSpecimen, discover, installGoogleFont } from "../lib/discover.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let {
    f = null,
    googleFont = null,
    sample,
    fav,
    locked = false,
    installed = true,
    backLabel,
    onback,
    ontoggle = null,
    onfav,
    onInstallGoogle = null,
  } = $props();

  const isGoogle = $derived(!f && Boolean(googleFont));
  const familyName = $derived(f ? f.name : (googleFont?.name ?? ""));
  const category = $derived(f ? f.category : (googleFont?.category ?? "sans"));
  const isOther = $derived(category === "other");
  const isVariable = $derived(f ? f.is_variable : Boolean(googleFont?.is_variable));
  const rawAxes = $derived(f ? (f.axes ?? []) : (googleFont?.axes ?? []));
  const rawFeatures = $derived(f ? (f.opentype_features ?? []) : ["liga", "dlig", "smcp", "onum", "tnum"]);
  const scripts = $derived(f ? (f.scripts ?? []) : (googleFont?.scripts ?? []));
  const styleCount = $derived(f ? f.styles.length : (googleFont?.styles ?? 0));

  const previewStyle = $derived(
    f
      ? fontStyle(f)
      : {
          family: `"${familyName}", var(--sans)`,
          weight: 400,
          italic: false,
        },
  );

  const metaLine = $derived(
    isGoogle
      ? `${styleCount} ${styleCount === 1 ? t("style") : t("styles")} · ${t("src." + (googleFont?.source ?? "google"))}${installed ? ` · ${t("g.installed")}` : ""}`
      : `${styleCount} ${styleCount === 1 ? t("style") : t("styles")} · ${t("origin." + f.origin)}`,
  );

  const charSets = $derived(
    isOther
      ? [
          { title: t("sample.title"), chars: sample },
          { title: t("numbers.only"), chars: "0123456789" },
        ]
      : [
          { title: t("uppercase"), chars: "ABCDEFGHIJKLMNOPQRSTUVWXYZ" },
          { title: t("lowercase"), chars: "abcdefghijklmnopqrstuvwxyz" },
          { title: t("numbers"), chars: "0123456789 ?!@#€$%&()[]{}" },
        ],
  );

  // ---- Variable axes state ----
  let axisValues = $state({});
  $effect(() => {
    const init = {};
    for (const ax of rawAxes) {
      init[ax.tag] = ax.default ?? ax.min;
    }
    axisValues = init;
  });

  function resetAxes() {
    const next = {};
    for (const ax of rawAxes) next[ax.tag] = ax.default ?? ax.min;
    axisValues = next;
  }

  const variationCss = $derived(
    rawAxes.length
      ? rawAxes.map((ax) => `"${ax.tag}" ${Number(axisValues[ax.tag] ?? ax.default ?? ax.min)}`).join(", ")
      : "normal",
  );

  // ---- OpenType features state ----
  let activeFeatures = $state(new Set(["liga", "kern"]));
  $effect(() => {
    const defaults = new Set();
    for (const feat of rawFeatures) {
      if (feat === "liga" || feat === "kern" || feat === "calt") defaults.add(feat);
    }
    activeFeatures = defaults;
  });

  function toggleFeature(tag) {
    const next = new Set(activeFeatures);
    if (next.has(tag)) next.delete(tag);
    else next.add(tag);
    activeFeatures = next;
  }

  const featureList = $derived.by(() => {
    const merged = new Set(rawFeatures);
    for (const common of ["liga", "dlig", "smcp", "onum", "tnum", "zero"]) merged.add(common);
    return [...merged].slice(0, 18);
  });

  const featureCss = $derived(
    featureList
      .map((tag) => `"${tag}" ${activeFeatures.has(tag) ? 1 : 0}`)
      .join(", ") || "normal",
  );

  // ---- Load Google full specimen stylesheet ----
  $effect(() => {
    if (isGoogle && googleFont) loadGoogleFullSpecimen(googleFont);
  });

  // ---- Local glyph map ----
  let glyphs = $state([]);
  $effect(() => {
    if (!f?.file) {
      glyphs = [];
      return;
    }
    const path = f.file;
    glyphs = [];
    fontCharacters(path)
      .then((codes) => {
        if (f?.file === path) glyphs = codes.map((c) => String.fromCodePoint(c));
      })
      .catch(() => {});
  });

  // ---- Tags ----
  let newTag = $state("");
  const familyTags = $derived(familyName ? tagsForFamily(familyName) : []);
  function submitTag(e) {
    if (e.key !== "Enter") return;
    e.preventDefault();
    if (!newTag.trim()) return;
    addTagToFamily(familyName, newTag);
    newTag = "";
  }

  async function copyText(str, label = str) {
    try {
      await navigator.clipboard.writeText(str);
    } catch {
      const a = document.createElement("textarea");
      a.value = str;
      document.body.appendChild(a);
      a.select();
      document.execCommand("copy");
      a.remove();
    }
    notify("ok", t("ok.copied", { c: label }), t("ok.pasteAnywhere"));
  }

  function copyCssSnippet() {
    const lines = [`font-family: "${familyName}", sans-serif;`];
    if (rawAxes.length) lines.push(`font-variation-settings: ${variationCss};`);
    const enabled = [...activeFeatures].filter((x) => x !== "liga" && x !== "kern");
    if (enabled.length) {
      lines.push(`font-feature-settings: ${enabled.map((x) => `"${x}" 1`).join(", ")};`);
    }
    copyText(lines.join("\n"), "CSS");
  }

  const googleDownloadPct = $derived(isGoogle ? discover.downloads[familyName] : null);
</script>

<div class="spec">
  <button type="button" class="back" onclick={onback}><ChevronLeft size={13} strokeWidth={2.2} />{backLabel}</button>

  <div class="head">
    <div class="title-col">
      <span class="meta">
        {metaLine}
        {#if isVariable}<span class="vf-pill">Variable</span>{/if}
      </span>
      <span
        class="title"
        style:font-family={previewStyle.family}
        style:font-weight={axisValues.wght ?? previewStyle.weight}
        style:font-style={previewStyle.italic ? "italic" : "normal"}
        style:font-variation-settings={variationCss}
        style:font-feature-settings={featureCss}>{familyName}</span
      >
      {#if isGoogle}
        <span class="google-note">{t("g.previewNote")}</span>
      {/if}
    </div>

    <button type="button" class="chip" onclick={onfav}>
      <span class="heart" class:fav><HeartIcon filled={fav} size={13} /></span>{fav ? t("fav.is") : t("fav.add")}
    </button>

    {#if isGoogle}
      {#if googleDownloadPct != null}
        <span class="chip fixed num">{Math.round(googleDownloadPct)} %</span>
      {:else if installed}
        <span class="chip ok"><Check size={13} strokeWidth={2} />{t("g.installed")}</span>
      {:else}
        <button type="button" class="chip primary" onclick={() => onInstallGoogle?.(familyName)}>
          <Download size={13} strokeWidth={2} />{t("g.install")}
        </button>
      {/if}
    {:else if locked}
      <span class="chip fixed">{t("alwaysOn")}</span>
    {:else}
      <button type="button" class="chip" class:primary={!f.active} onclick={ontoggle}>{f.active ? t("deactivate") : t("activate")}</button>
    {/if}
  </div>

  <!-- Variable axes & OpenType inspector -->
  {#if rawAxes.length || featureList.length}
    <div class="inspector-grid">
      {#if rawAxes.length}
        <section class="panel">
          <div class="panel-head">
            <span class="label">{t("spec.variableAxes")}</span>
            <div class="panel-actions">
              <button type="button" class="mini-btn" onclick={resetAxes}><RotateCcw size={11} />{t("spec.resetAxes")}</button>
              <button type="button" class="mini-btn accent" onclick={copyCssSnippet}><Copy size={11} />{t("spec.copyCss")}</button>
            </div>
          </div>
          <div class="axes">
            {#each rawAxes as ax (ax.tag)}
              <label class="axis-row">
                <span class="axis-tag num">{ax.tag}</span>
                <span class="axis-name">{ax.name || ax.tag}</span>
                <input
                  type="range"
                  min={ax.min}
                  max={ax.max}
                  step={ax.max - ax.min <= 10 ? 0.1 : 1}
                  bind:value={axisValues[ax.tag]}
                />
                <span class="axis-val num">{axisValues[ax.tag]}</span>
              </label>
            {/each}
          </div>
        </section>
      {/if}

      <section class="panel">
        <div class="panel-head">
          <span class="label">{t("spec.opentype")}</span>
          {#if !rawAxes.length}
            <button type="button" class="mini-btn accent" onclick={copyCssSnippet}><Copy size={11} />{t("spec.copyCss")}</button>
          {/if}
        </div>
        <div class="features">
          {#each featureList as tag (tag)}
            <button
              type="button"
              class="feat-pill"
              class:on={activeFeatures.has(tag)}
              onclick={() => toggleFeature(tag)}
            >
              <span class="num">{tag}</span>
            </button>
          {/each}
        </div>
      </section>
    </div>
  {/if}

  <!-- Metadata, Scripts, Tags & Diagnostics -->
  <div class="meta-panels">
    {#if scripts.length}
      <div class="meta-box">
        <span class="label">{t("spec.scripts")}</span>
        <div class="badges">
          {#each scripts as sc (sc)}<span class="badge">{sc}</span>{/each}
        </div>
      </div>
    {/if}

    <div class="meta-box">
      <span class="label">{t("spec.tags")}</span>
      <div class="badges">
        {#each familyTags as tag (tag)}
          <span class="tag-pill">
            #{tag}
            <button type="button" aria-label={t("close")} onclick={() => removeTagFromFamily(familyName, tag)}><X size={10} /></button>
          </span>
        {/each}
        <input class="tag-input" bind:value={newTag} onkeydown={submitTag} placeholder={t("spec.addTag")} />
      </div>
    </div>

    {#if (f && (f.foundry || f.designer || f.license)) || (googleFont?.designers?.length)}
      <div class="meta-box wide">
        <span class="label">{t("spec.credits")}</span>
        <div class="credits">
          {#if f?.foundry}<span><b>{t("spec.foundry")}:</b> {f.foundry}</span>{/if}
          {#if f?.designer || googleFont?.designers?.length}
            <span><b>{t("spec.designer")}:</b> {f?.designer || googleFont.designers.join(", ")}</span>
          {/if}
          {#if f?.license}
            <span>
              <b>{t("spec.license")}:</b> {f.license}
              {#if f.license_url}
                <button type="button" class="copy-url" onclick={() => copyText(f.license_url, f.license_url)}>({t("copy")} URL)</button>
              {/if}
            </span>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  {#if f && (f.error || f.duplicates?.length)}
    <section class="diag-box">
      <div class="diag-head">
        <ShieldAlert size={15} strokeWidth={2} />
        <span>{t("spec.duplicates")}</span>
      </div>
      {#if f.error}
        <p class="diag-err">{f.error}</p>
        <button type="button" class="quarantine-btn" onclick={() => quarantineCorruptFont(f.file)}>
          {t("spec.quarantine")}
        </button>
      {/if}
      {#each f.duplicates as d (d.path)}
        <div class="dup-row">
          <span class="dup-kind" class:exact={d.kind === "exact"}>
            {d.kind === "exact" ? t("spec.dupExact") : t("spec.dupConflict")}
          </span>
          <button type="button" class="dup-path" onclick={() => revealInExplorer(d.path)}>{d.path}</button>
          <button type="button" class="mini-btn danger" onclick={() => quarantineCorruptFont(d.path)}>
            {t("spec.quarantine")}
          </button>
        </div>
      {/each}
    </section>
  {/if}

  <!-- Waterfall -->
  <div class="waterfall">
    {#each [12, 18, 24, 36, 48, 72] as n (n)}
      <div class="step">
        <span class="n num">{n}</span>
        <span
          class="line"
          style:font-family={previewStyle.family}
          style:font-weight={axisValues.wght ?? previewStyle.weight}
          style:font-style={previewStyle.italic ? "italic" : "normal"}
          style:font-variation-settings={variationCss}
          style:font-feature-settings={featureCss}
          style:font-size="{n}px">{sample}</span
        >
      </div>
    {/each}
  </div>

  <!-- Character Sets -->
  <div class="sets">
    {#each charSets as cs (cs.title)}
      <div class="set">
        <span class="label">{cs.title}</span>
        <span
          class="chars"
          style:font-family={previewStyle.family}
          style:font-weight={axisValues.wght ?? previewStyle.weight}
          style:font-style={previewStyle.italic ? "italic" : "normal"}
          style:font-variation-settings={variationCss}
          style:font-feature-settings={featureCss}>{cs.chars}</span
        >
      </div>
    {/each}
  </div>

  <!-- Glyph Map -->
  {#if glyphs.length}
    <div class="block">
      <span class="label">{t("allChars")} · {glyphs.length}</span>
      <div class="glyphs">
        {#each glyphs as c, i (i)}
          <button
            type="button"
            class="glyph"
            title={t("copy") + " U+" + c.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}
            style:font-family={previewStyle.family}
            style:font-weight={axisValues.wght ?? previewStyle.weight}
            style:font-style={previewStyle.italic ? "italic" : "normal"}
            style:font-variation-settings={variationCss}
            style:font-feature-settings={featureCss}
            onclick={() => copyText(c)}>{c}</button
          >
        {/each}
      </div>
    </div>
  {/if}

  <!-- Styles -->
  {#if f}
    <div class="block">
      <span class="label">{t("styles.title", { n: f.styles.length })}</span>
      {#each f.styles as s (s.name + s.weight + s.italic)}
        {@const st = fontStyle(f, s)}
        <div class="style-row">
          <span class="n">{s.name}</span>
          <span
            class="line"
            style:font-family={st.family}
            style:font-weight={st.weight}
            style:font-style={st.italic ? "italic" : "normal"}
            style:font-feature-settings={featureCss}
            style:font-size="26px">{sample}</span
          >
        </div>
      {/each}
    </div>

    <div class="paths">
      {#each f.files as file (file)}
        <button type="button" class="file-btn" title={t("path.reveal")} onclick={() => revealInExplorer(file)}>
          <span class="txt">{file}</span>
          <FolderOpen size={13} strokeWidth={1.9} />
        </button>
      {/each}
    </div>
  {:else if googleFont?.variants?.length}
    <div class="block">
      <span class="label">{t("styles.title", { n: googleFont.variants.length })}</span>
      {#each googleFont.variants as v (v)}
        {@const italic = v.endsWith("italic")}
        {@const weight = parseInt(v, 10) || 400}
        <div class="style-row">
          <span class="n">{v}</span>
          <span
            class="line"
            style:font-family={`"${familyName}", var(--sans)`}
            style:font-weight={weight}
            style:font-style={italic ? "italic" : "normal"}
            style:font-feature-settings={featureCss}
            style:font-size="26px">{sample}</span
          >
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .spec { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 40px 70px 24px; display: flex; flex-direction: column; gap: 28px; animation: cf-up 0.25s ease-out; }
  .spec > :global(*) { flex-shrink: 0; }
  .back, .chip {
    height: 28px;
    padding: 0 12px 0 8px;
    border-radius: 999px;
    border: 0;
    background: var(--chip);
    color: var(--ink);
    font: 600 12.5px var(--sans);
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  .back { align-self: flex-start; }
  .chip { height: 30px; padding: 0 12px; gap: 6px; }
  .chip.primary { background: var(--accent); color: #fff; padding: 0 14px; }
  .chip.ok { color: var(--ok); }
  .chip.fixed { color: var(--ink2); }
  .heart { display: flex; }
  .heart.fav { color: var(--accent); }
  .head { display: flex; align-items: flex-end; gap: 12px; }
  .title-col { display: flex; flex-direction: column; gap: 6px; flex: 1; min-width: 0; }
  .meta { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--ink3); }
  .vf-pill { font: 700 10px var(--mono); padding: 1px 6px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); }
  .title { font-size: 64px; line-height: 1.45; padding-top: 0.08em; overflow-wrap: anywhere; }
  .google-note { font-size: 12px; color: var(--ink2); }

  .inspector-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 14px; }
  .panel { background: var(--side); border: 1px solid var(--line); border-radius: 12px; padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; }
  .panel-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .panel-actions { display: flex; gap: 6px; }
  .mini-btn { height: 22px; padding: 0 8px; border: 0; border-radius: 6px; background: var(--chip); color: var(--ink2); font: 600 11px var(--sans); display: inline-flex; align-items: center; gap: 4px; }
  .mini-btn:hover { color: var(--ink); background: var(--line); }
  .mini-btn.accent { background: var(--accent-soft); color: var(--accent); }
  .mini-btn.danger { background: rgba(214, 56, 81, 0.14); color: var(--danger); }
  .axes { display: flex; flex-direction: column; gap: 8px; }
  .axis-row { display: grid; grid-template-columns: 42px 86px 1fr 48px; align-items: center; gap: 8px; font-size: 12px; }
  .axis-tag { font: 600 11px var(--mono); color: var(--accent); }
  .axis-name { color: var(--ink2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .axis-row input[type="range"] { width: 100%; accent-color: var(--accent); }
  .axis-val { text-align: right; font: 500 11.5px var(--mono); color: var(--ink3); }
  .features { display: flex; flex-wrap: wrap; gap: 6px; }
  .feat-pill { height: 26px; padding: 0 10px; border: 1px solid var(--line2); border-radius: 7px; background: transparent; color: var(--ink2); font: 600 11.5px var(--mono); transition: all 0.12s; }
  .feat-pill:hover { border-color: var(--accent); color: var(--ink); }
  .feat-pill.on { background: var(--accent); border-color: var(--accent); color: #fff; }

  .meta-panels { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 12px; }
  .meta-box { background: var(--field); border: 1px solid var(--line); border-radius: 10px; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  .meta-box.wide { grid-column: 1 / -1; }
  .badges { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .badge { padding: 3px 8px; border-radius: 6px; background: var(--chip); font-size: 11.5px; color: var(--ink2); font-weight: 500; }
  .tag-pill { display: inline-flex; align-items: center; gap: 4px; padding: 3px 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font: 600 11.5px var(--sans); }
  .tag-pill button { border: 0; background: transparent; padding: 0; display: flex; color: inherit; opacity: 0.7; }
  .tag-pill button:hover { opacity: 1; }
  .tag-input { border: 1px dashed var(--line2); border-radius: 999px; background: transparent; padding: 3px 10px; font-size: 11.5px; width: 132px; color: var(--ink); }
  .credits { display: flex; flex-wrap: wrap; gap: 8px 20px; font-size: 12.5px; color: var(--ink2); }
  .credits b { color: var(--ink); font-weight: 600; }
  .copy-url { border: 0; background: transparent; color: var(--accent); font-weight: 600; padding: 0 4px; }

  .diag-box { border: 1px solid rgba(196, 125, 23, 0.35); background: rgba(196, 125, 23, 0.08); border-radius: 12px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .diag-head { display: flex; align-items: center; gap: 8px; font: 700 12.5px var(--sans); color: var(--warn); }
  .diag-err { margin: 0; font-size: 12.5px; color: var(--danger); }
  .quarantine-btn { align-self: flex-start; height: 28px; padding: 0 12px; border: 0; border-radius: 8px; background: var(--danger); color: #fff; font: 600 12px var(--sans); }
  .dup-row { display: flex; align-items: center; gap: 10px; font-size: 12px; }
  .dup-kind { font: 600 11px var(--sans); padding: 2px 8px; border-radius: 999px; background: rgba(196, 125, 23, 0.2); color: var(--warn); flex: none; }
  .dup-kind.exact { background: rgba(214, 56, 81, 0.16); color: var(--danger); }
  .dup-path { flex: 1; min-width: 0; border: 0; background: transparent; padding: 0; text-align: left; font: 400 11.5px var(--mono); color: var(--ink2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dup-path:hover { color: var(--ink); text-decoration: underline; }

  .waterfall { display: flex; flex-direction: column; }
  .step { display: grid; grid-template-columns: 40px minmax(0, 1fr); align-items: baseline; gap: 12px; padding: 4px 0; }
  .step .n { font-size: 11px; color: var(--ink3); }
  .line { line-height: 1.38; white-space: nowrap; overflow-x: clip; overflow-y: visible; text-overflow: ellipsis; }
  .waterfall .line { white-space: normal; overflow: visible; overflow-wrap: anywhere; line-height: 1.3; padding: 0.12em 0; margin: 0; }
  .sets { display: flex; flex-direction: column; gap: 18px; }
  .set { display: flex; flex-direction: column; gap: 4px; }
  .chars { font-size: 30px; line-height: 1.5; letter-spacing: 0.02em; word-break: break-all; }
  .block { display: flex; flex-direction: column; gap: 8px; }
  .glyphs { display: grid; grid-template-columns: repeat(auto-fill, minmax(52px, 1fr)); gap: 4px; }
  .glyph {
    height: 56px;
    border: 0;
    border-radius: 8px;
    background: var(--chip);
    color: var(--ink);
    font-size: 26px;
    line-height: 1;
    content-visibility: auto;
    contain-intrinsic-size: 52px 56px;
    transition: background-color 0.12s, color 0.12s, transform 0.12s;
  }
  .glyph:hover { background: var(--accent); color: #fff; transform: scale(1.06); }
  .style-row { display: flex; flex-direction: column; gap: 1px; padding: 6px 0; }
  .style-row .n { font-size: 10.5px; color: var(--ink3); }
  .paths { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding-top: 14px; border-top: 1px solid var(--line); }
  .file-btn {
    max-width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: -8px;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: 400 12px var(--mono);
    text-align: left;
    transition: background-color 0.15s, color 0.15s;
  }
  .file-btn .txt { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-btn :global(svg) { opacity: 0; transition: opacity 0.15s; }
  .file-btn:hover { background: var(--hov); color: var(--ink); }
  .file-btn:hover :global(svg), .file-btn:focus-visible :global(svg) { opacity: 1; }
</style>
