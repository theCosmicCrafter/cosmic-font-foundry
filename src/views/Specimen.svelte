<script>
  import { ChevronLeft, FolderOpen, Copy, RotateCcw, ShieldAlert, Plus, X, Download, Check, Sun, Moon, WandSparkles, Scissors, Eye, EyeOff, ExternalLink } from "@lucide/svelte";
  import HeartIcon from "../components/HeartIcon.svelte";
  import { fontStyle, fontCharacters, revealInExplorer, quarantineCorruptFont, installFamiliesToWindows, invokeCmd } from "../lib/fonts.svelte.js";
  import { inTauri, settings } from "../lib/settings.svelte.js";
  import { tagsForFamily, addTagToFamily, removeTagFromFamily } from "../lib/library.svelte.js";
  import { loadGoogleFullSpecimen, discover, installGoogleFont, openFontSourceUrl } from "../lib/discover.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let {
    f = null,
    googleFont = null,
    sample,
    fmt = { bold: false, italic: false, underline: false, strike: false, uppercase: false },
    fav,
    locked = false,
    installed = true,
    backLabel,
    onback,
    ontoggle = null,
    onfav,
    onInstallGoogle = null,
    onopenfamily = null,
    onfork = null,
  } = $props();

  let specBold = $state(false);
  let specItalic = $state(false);
  let specUnderline = $state(false);
  let specStrike = $state(false);
  let specUpper = $state(false);
  let specSmallCaps = $state(false);
  let surface = $state("auto");
  let installingWin = $state(false);
  let similarFonts = $state([]);

  $effect(() => {
    const name = f?.name;
    similarFonts = [];
    if (!name || !inTauri) return;
    invokeCmd("find_similar_fonts", { family: name, limit: 8 })
      .then((list) => {
        if (f?.name === name) similarFonts = list ?? [];
      })
      .catch(() => {});
  });

  async function installToWindows() {
    if (!f || installingWin) return;
    installingWin = true;
    try {
      await installFamiliesToWindows([f.name], settings.installFor === "all");
    } finally {
      installingWin = false;
    }
  }

  $effect(() => {
    specBold = Boolean(fmt?.bold);
    specItalic = Boolean(fmt?.italic);
    specUnderline = Boolean(fmt?.underline);
    specStrike = Boolean(fmt?.strike);
    specUpper = Boolean(fmt?.uppercase);
  });

  const specDec = $derived(
    [specUnderline && "underline", specStrike && "line-through"].filter(Boolean).join(" ") || "none",
  );

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
  const BLOCK_ORDER = [
    "All",
    "Basic Latin",
    "Latin Extended",
    "Punctuation & Symbols",
    "Arrows & Math",
    "Greek & Cyrillic",
    "Arabic & Hebrew",
    "Other & CJK",
  ];

  function classifyBlock(cp) {
    if (cp <= 0x007f) return "Basic Latin";
    if (cp <= 0x024f) return "Latin Extended";
    if (cp >= 0x0370 && cp <= 0x052f) return "Greek & Cyrillic";
    if (cp >= 0x0590 && cp <= 0x06ff) return "Arabic & Hebrew";
    if ((cp >= 0x2000 && cp <= 0x20cf) || (cp >= 0x2400 && cp <= 0x27bf)) return "Punctuation & Symbols";
    if (cp >= 0x2190 && cp <= 0x23ff) return "Arrows & Math";
    return "Other & CJK";
  }

  let glyphs = $state([]);
  let glyphBlock = $state("All");
  let glyphQuery = $state("");
  let glyphLimit = $state(240);

  // ---- Vector outline inspector & subset export ----
  let inspectedGlyph = $state(null);
  let inspectedChar = $state("");
  let inspectedCode = $state(null);
  let inspectingGlyph = $state(false);
  let showOutlineNodes = $state(true);
  let inspectSeq = 0;

  let subsetOpen = $state(false);
  let subsetText = $state("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 .,:;!?");
  let subsetting = $state(false);

  $effect(() => {
    glyphBlock = "All";
    glyphQuery = "";
    glyphLimit = 240;
    inspectedGlyph = null;
    inspectedChar = "";
    inspectedCode = null;
    subsetOpen = false;
    if (!f?.file) {
      glyphs = [];
      return;
    }
    const path = f.file;
    glyphs = [];
    fontCharacters(path)
      .then((codes) => {
        if (f?.file === path) {
          glyphs = codes.map((code) => ({
            char: String.fromCodePoint(code),
            code,
            hex: code.toString(16).toUpperCase().padStart(4, "0"),
            block: classifyBlock(code),
          }));
        }
      })
      .catch(() => {});
  });

  const glyphBlocks = $derived.by(() => {
    const counts = { All: glyphs.length };
    for (const g of glyphs) counts[g.block] = (counts[g.block] ?? 0) + 1;
    return BLOCK_ORDER.filter((b) => (counts[b] ?? 0) > 0).map((name) => ({ name, count: counts[name] }));
  });

  const filteredGlyphs = $derived.by(() => {
    const q = glyphQuery.trim().toUpperCase().replace(/^U\+/, "");
    return glyphs.filter((g) => {
      if (glyphBlock !== "All" && g.block !== glyphBlock) return false;
      if (!q) return true;
      return g.char.toUpperCase() === q || g.hex.includes(q);
    });
  });

  const shownGlyphs = $derived(filteredGlyphs.slice(0, glyphLimit));

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

  function handleFork() {
    if (!f || !onfork) return;
    if (f.license_tier && f.license_tier !== "free_commercial") {
      notify("warn", t("spec.forkNonFreeWarn"), f.license || "");
    }
    onfork(f);
  }

  async function runSubsetExport() {
    if (!inTauri || !f?.file || subsetting || !subsetText.trim()) return;
    subsetting = true;
    try {
      const axes = Object.entries(axisValues).map(([k, v]) => [k, Number(v)]);
      const savedPath = await invokeCmd("foundry_subset_font", {
        path: f.file,
        text: subsetText,
        axes,
      });
      if (savedPath) {
        notify("ok", t("spec.subsetSuccess"), savedPath);
        subsetOpen = false;
      }
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      subsetting = false;
    }
  }

  async function onGlyphClick(g) {
    copyText(g.char);
    if (!inTauri || !f?.file) return;
    const seq = ++inspectSeq;
    const path = f.file;
    inspectedCode = g.code;
    inspectedChar = g.char;
    inspectingGlyph = true;
    try {
      const info = await invokeCmd("foundry_inspect_glyph", { path, codepoint: g.code });
      if (seq === inspectSeq && f?.file === path) {
        inspectedGlyph = info;
      }
    } catch (e) {
      if (seq === inspectSeq) {
        inspectedGlyph = null;
        notify("err", t("err.one"), String(e));
      }
    } finally {
      if (seq === inspectSeq) inspectingGlyph = false;
    }
  }

  function detectAbsoluteHandles(contours) {
    for (const c of contours ?? []) {
      for (const a of c.anchors ?? []) {
        if (Math.abs(a.x) > 1 || Math.abs(a.y) > 1) {
          if ((a.h_in?.[0] === 0 && a.h_in?.[1] === 0) || (a.h_out?.[0] === 0 && a.h_out?.[1] === 0)) {
            return false;
          }
          if (Math.abs((a.h_in?.[0] ?? 0) - a.x) < 0.01 && Math.abs((a.h_in?.[1] ?? 0) - a.y) < 0.01) {
            return true;
          }
        }
      }
    }
    return false;
  }

  function handleTipSvg(anchor, handle, ascender, absHandles) {
    const hx = absHandles ? (handle?.[0] ?? anchor.x) : anchor.x + (handle?.[0] ?? 0);
    const hy = absHandles ? (handle?.[1] ?? anchor.y) : anchor.y + (handle?.[1] ?? 0);
    const hasHandle = Math.abs(hx - anchor.x) > 0.05 || Math.abs(hy - anchor.y) > 0.05;
    return { x: hx, y: ascender - hy, hasHandle };
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
        style:font-weight={specBold ? 700 : (axisValues.wght ?? previewStyle.weight)}
        style:font-style={specItalic || previewStyle.italic ? "italic" : "normal"}
        style:text-decoration={specDec}
        style:text-transform={specUpper ? "uppercase" : "none"}
        style:font-variant-caps={specSmallCaps ? "small-caps" : "normal"}
        style:font-variation-settings={variationCss}
        style:font-feature-settings={featureCss}>{familyName}</span
      >
      {#if isGoogle}
        <span class="google-note">{googleFont?.description || t("g.previewNote")}</span>
      {/if}
    </div>

    <button type="button" class="chip" onclick={onfav}>
      <span class="heart" class:fav><HeartIcon filled={fav} size={13} /></span>{fav ? t("fav.is") : t("fav.add")}
    </button>

    {#if f && f.file && onfork}
      <button type="button" class="chip fork-btn" onclick={handleFork}>
        <WandSparkles size={13} strokeWidth={2} />{t("spec.forkFoundry")}
      </button>
    {/if}

    {#if isGoogle}
      {#if googleFont?.homepage}
        <button type="button" class="chip" onclick={() => openFontSourceUrl(googleFont.homepage)} title={t("g.openOriginTip")}>
          <ExternalLink size={13} strokeWidth={2} />{t("g.openOrigin")}
        </button>
      {/if}
      {#if googleDownloadPct != null}
        <span class="chip fixed num">{Math.round(googleDownloadPct)} %</span>
      {:else if installed}
        <span class="chip ok"><Check size={13} strokeWidth={2} />{t("g.installed")}</span>
      {:else if googleFont?.download_mode === "origin_link" && googleFont?.homepage}
        <button type="button" class="chip primary" onclick={() => openFontSourceUrl(googleFont.homepage)} title={t("g.openOriginTip")}>
          <ExternalLink size={13} strokeWidth={2} />{t("g.openOrigin")}
        </button>
      {:else}
        <button type="button" class="chip primary" onclick={() => onInstallGoogle?.(familyName)}>
          <Download size={13} strokeWidth={2} />{t("g.install")}
        </button>
      {/if}
    {:else if locked}
      <span class="chip fixed">{t("alwaysOn")}</span>
    {:else}
      {#if f?.origin === "dir"}
        <button type="button" class="chip" disabled={installingWin} onclick={installToWindows}>
          <Download size={13} strokeWidth={2} />{t("spec.installWin")}
        </button>
      {/if}
      <button type="button" class="chip" class:primary={!f.active} onclick={ontoggle}>{f.active ? t("deactivate") : t("activate")}</button>
    {/if}
  </div>

  <!-- Formatting Sampler Bar -->
  <div class="fmt-bar">
    <span class="label">{t("spec.formatSampler")}</span>
    <div class="fmt-pills">
      <button type="button" class="fmt-btn bold" class:on={specBold} onclick={() => (specBold = !specBold)}>{t("fmt.bold")}</button>
      <button type="button" class="fmt-btn italic" class:on={specItalic} onclick={() => (specItalic = !specItalic)}>{t("fmt.italic")}</button>
      <button type="button" class="fmt-btn underline" class:on={specUnderline} onclick={() => (specUnderline = !specUnderline)}>{t("fmt.underline")}</button>
      <button type="button" class="fmt-btn strike" class:on={specStrike} onclick={() => (specStrike = !specStrike)}>{t("fmt.strike")}</button>
      <button type="button" class="fmt-btn" class:on={specUpper} onclick={() => (specUpper = !specUpper)}>{t("fmt.upper")}</button>
      <button type="button" class="fmt-btn" class:on={specSmallCaps} onclick={() => (specSmallCaps = !specSmallCaps)}>{t("fmt.smallCaps")}</button>
    </div>
    <div class="surface-pills">
      <button type="button" class="fmt-btn" class:on={surface === "light"} onclick={() => (surface = surface === "light" ? "auto" : "light")} title={t("spec.surfaceLight")}>
        <Sun size={11} />
      </button>
      <button type="button" class="fmt-btn" class:on={surface === "dark"} onclick={() => (surface = surface === "dark" ? "auto" : "dark")} title={t("spec.surfaceDark")}>
        <Moon size={11} />
      </button>
    </div>
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
          <div class="panel-actions">
            {#if inTauri && f?.file}
              <button
                type="button"
                class="mini-btn"
                class:accent={subsetOpen}
                onclick={() => (subsetOpen = !subsetOpen)}
              >
                <Scissors size={11} />{t("spec.subsetFont")}
              </button>
            {/if}
            {#if !rawAxes.length}
              <button type="button" class="mini-btn accent" onclick={copyCssSnippet}><Copy size={11} />{t("spec.copyCss")}</button>
            {/if}
          </div>
        </div>
        {#if subsetOpen && inTauri && f?.file}
          <div class="subset-popover">
            <input
              type="text"
              class="subset-input"
              bind:value={subsetText}
              placeholder={t("spec.subsetPlaceholder")}
              spellcheck="false"
            />
            <div class="subset-foot">
              {#if rawAxes.length}
                <span class="subset-axes num">
                  {rawAxes.map((ax) => `${ax.tag}:${Number(axisValues[ax.tag] ?? ax.default ?? ax.min)}`).join(" · ")}
                </span>
              {/if}
              <button type="button" class="mini-btn accent" disabled={subsetting || !subsetText.trim()} onclick={runSubsetExport}>
                <Download size={11} />{t("spec.subsetExport")}
              </button>
            </div>
          </div>
        {/if}
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

    {#if (f && (f.foundry || f.designer || f.license || f.license_tier)) || (googleFont && (googleFont.designers?.length || googleFont.license || googleFont.homepage))}
      <div class="meta-box wide">
        <span class="label">
          {t("spec.credits")}
          {#if f?.license_tier}
            <span class="lic-pill" class:safe={f.license_tier === "free_commercial"} class:warn={f.license_tier === "personal_only"}>
              {t("lic." + f.license_tier)}
            </span>
          {/if}
        </span>
        <div class="credits">
          {#if f?.foundry}<span><b>{t("spec.foundry")}:</b> {f.foundry}</span>{/if}
          {#if f?.designer || googleFont?.designers?.length}
            <span><b>{t("spec.designer")}:</b> {f?.designer || googleFont.designers.join(", ")}</span>
          {/if}
          {#if f?.license || googleFont?.license}
            <span>
              <b>{t("spec.license")}:</b> {f?.license || googleFont.license}
              {#if f?.license_url}
                <button type="button" class="copy-url" onclick={() => copyText(f.license_url, f.license_url)}>({t("copy")} URL)</button>
              {/if}
            </span>
          {/if}
        </div>
      </div>
    {/if}

    {#if similarFonts.length}
      <div class="meta-box wide">
        <span class="label">{t("spec.similar")}</span>
        <div class="badges">
          {#each similarFonts as sim (sim.name)}
            <button
              type="button"
              class="sim-chip"
              title={sim.reason}
              onclick={() => onopenfamily?.(sim.name)}
            >
              <span>{sim.name}</span>
              <span class="sim-score num">{sim.score}%</span>
            </button>
          {/each}
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

  <!-- Standard Formatting Matrix -->
  <div class="block format-matrix">
    <span class="label">{t("spec.formatMatrix")}</span>
    <div class="matrix-grid">
      {#each [
        { lbl: t("fmt.regular"), w: 400, it: false, dec: "none", tr: "none", ls: "normal" },
        { lbl: t("fmt.bold"), w: 700, it: false, dec: "none", tr: "none", ls: "normal" },
        { lbl: t("fmt.italic"), w: 400, it: true, dec: "none", tr: "none", ls: "normal" },
        { lbl: t("fmt.boldItalic"), w: 700, it: true, dec: "none", tr: "none", ls: "normal" },
        { lbl: t("fmt.underline"), w: 400, it: false, dec: "underline", tr: "none", ls: "normal" },
        { lbl: t("fmt.strike"), w: 400, it: false, dec: "line-through", tr: "none", ls: "normal" },
        { lbl: t("fmt.boldUnderline"), w: 700, it: false, dec: "underline", tr: "none", ls: "normal" },
        { lbl: t("fmt.upperTracked"), w: 600, it: false, dec: "none", tr: "uppercase", ls: "0.08em" },
      ] as row (row.lbl)}
        <div class="matrix-card">
          <span class="matrix-lbl">{row.lbl}</span>
          <span
            class="matrix-sample"
            style:font-family={previewStyle.family}
            style:font-weight={row.w}
            style:font-style={row.it ? "italic" : "normal"}
            style:text-decoration={row.dec}
            style:text-transform={row.tr}
            style:letter-spacing={row.ls}
            style:font-feature-settings={featureCss}>{sample}</span
          >
        </div>
      {/each}
    </div>
  </div>

  <!-- Waterfall -->
  <div class="waterfall" class:surface-light={surface === "light"} class:surface-dark={surface === "dark"}>
    {#each [12, 18, 24, 36, 48, 72] as n (n)}
      <div class="step">
        <span class="n num">{n}</span>
        <span
          class="line"
          style:font-family={previewStyle.family}
          style:font-weight={specBold ? 700 : (axisValues.wght ?? previewStyle.weight)}
          style:font-style={specItalic || previewStyle.italic ? "italic" : "normal"}
          style:text-decoration={specDec}
          style:text-transform={specUpper ? "uppercase" : "none"}
          style:font-variant-caps={specSmallCaps ? "small-caps" : "normal"}
          style:font-variation-settings={variationCss}
          style:font-feature-settings={featureCss}
          style:font-size="{n}px">{sample}</span
        >
      </div>
    {/each}
  </div>

  <!-- Editorial Article Preview -->
  <section class="editorial-preview" class:surface-light={surface === "light"} class:surface-dark={surface === "dark"}>
    <span class="label">{t("spec.editorial")}</span>
    <h3
      class="ed-h1"
      style:font-family={previewStyle.family}
      style:font-variation-settings={variationCss}
      style:font-feature-settings={featureCss}>{t("spec.edHeadline")}</h3>
    <p
      class="ed-h2"
      style:font-family={previewStyle.family}
      style:font-variation-settings={variationCss}
      style:font-feature-settings={featureCss}>{t("spec.edSubhead")}</p>
    <div
      class="ed-cols"
      style:font-family={previewStyle.family}
      style:font-weight={specBold ? 700 : (axisValues.wght ?? previewStyle.weight)}
      style:font-style={specItalic || previewStyle.italic ? "italic" : "normal"}
      style:font-variation-settings={variationCss}
      style:font-feature-settings={featureCss}>
      <p>{t("spec.edCol1")}</p>
      <p>{t("spec.edCol2")}</p>
    </div>
  </section>

  <!-- Character Sets -->
  <div class="sets">
    {#each charSets as cs (cs.title)}
      <div class="set">
        <span class="label">{cs.title}</span>
        <span
          class="chars"
          style:font-family={previewStyle.family}
          style:font-weight={specBold ? 700 : (axisValues.wght ?? previewStyle.weight)}
          style:font-style={specItalic || previewStyle.italic ? "italic" : "normal"}
          style:text-decoration={specDec}
          style:text-transform={specUpper ? "uppercase" : "none"}
          style:font-variant-caps={specSmallCaps ? "small-caps" : "normal"}
          style:font-variation-settings={variationCss}
          style:font-feature-settings={featureCss}>{cs.chars}</span
        >
      </div>
    {/each}
  </div>

  <!-- Glyph Map -->
  {#if glyphs.length}
    <div class="block">
      <div class="glyph-head">
        <span class="label">{t("allChars")} · {filteredGlyphs.length}</span>
        <input
          class="glyph-search"
          bind:value={glyphQuery}
          placeholder={t("spec.glyphSearch")}
          spellcheck="false"
        />
      </div>
      {#if glyphBlocks.length > 2}
        <div class="glyph-blocks">
          {#each glyphBlocks as blk (blk.name)}
            <button
              type="button"
              class="blk-pill"
              class:active={glyphBlock === blk.name}
              onclick={() => ((glyphBlock = blk.name), (glyphLimit = 240))}
            >
              {blk.name} <span class="num">{blk.count}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if inTauri && f?.file && (inspectingGlyph || inspectedGlyph)}
        <div class="outline-inspector">
          <div class="outline-head">
            <span class="label">
              {t("spec.outlineInspector")} · “{inspectedChar}”
              {#if inspectedGlyph}
                <span class="outline-gname num">U+{inspectedCode?.toString(16).toUpperCase().padStart(4, "0")} · {inspectedGlyph.glyph_name}</span>
              {/if}
            </span>
            <div class="outline-actions">
              {#if inspectedGlyph}
                <button
                  type="button"
                  class="mini-btn"
                  class:accent={showOutlineNodes}
                  onclick={() => (showOutlineNodes = !showOutlineNodes)}
                >
                  {#if showOutlineNodes}<Eye size={11} />{:else}<EyeOff size={11} />{/if}
                  {t("forge.showNodes")}
                </button>
                <button type="button" class="mini-btn" onclick={() => copyText(inspectedChar)}>
                  <Copy size={11} />{t("copy")}
                </button>
                <button type="button" class="mini-btn accent" onclick={() => copyText(inspectedGlyph.svg_path, "SVG <path>")}>
                  <Copy size={11} />{t("spec.copySvg")}
                </button>
              {/if}
              <button
                type="button"
                class="mini-btn"
                aria-label={t("close")}
                onclick={() => ((inspectedGlyph = null), (inspectedCode = null))}
              >
                <X size={11} />
              </button>
            </div>
          </div>

          {#if inspectedGlyph}
            {@const info = inspectedGlyph}
            {@const vb = info.view_box ?? [0, 0, info.upm || 1000, info.upm || 1000]}
            {@const absHandles = detectAbsoluteHandles(info.contours)}
            {@const sw = Math.max(1, (info.upm || 1000) * 0.003)}
            {@const nodeR = Math.max(2.5, (info.upm || 1000) * 0.01)}
            <div class="outline-body">
              <div class="outline-svg-wrap">
                <svg class="outline-svg" viewBox={vb.join(" ")} preserveAspectRatio="xMidYMid meet">
                  <!-- Origin (x=0) & Advance Width lines -->
                  <line x1="0" y1={vb[1]} x2="0" y2={vb[1] + vb[3]} class="guide-origin" stroke-width={sw} />
                  <line x1={info.advance_width} y1={vb[1]} x2={info.advance_width} y2={vb[1] + vb[3]} class="guide-adv" stroke-width={sw} />
                  <!-- Baseline (y = info.ascender) -->
                  <line x1={vb[0]} y1={info.ascender} x2={vb[0] + vb[2]} y2={info.ascender} class="guide-base" stroke-width={sw * 1.2} />
                  <!-- Glyph outline -->
                  <path d={info.svg_path} class="glyph-path" />
                  <!-- Anchor dots & handles -->
                  {#if showOutlineNodes && info.contours?.length}
                    {#each info.contours as contour}
                      {#each contour.anchors as pt}
                        {@const ax = pt.x}
                        {@const ay = info.ascender - pt.y}
                        {@const hin = handleTipSvg(pt, pt.h_in, info.ascender, absHandles)}
                        {@const hout = handleTipSvg(pt, pt.h_out, info.ascender, absHandles)}
                        {#if hin.hasHandle}
                          <line x1={ax} y1={ay} x2={hin.x} y2={hin.y} class="handle-line" stroke-width={sw} />
                          <circle cx={hin.x} cy={hin.y} r={nodeR * 0.7} class="handle-dot" />
                        {/if}
                        {#if hout.hasHandle}
                          <line x1={ax} y1={ay} x2={hout.x} y2={hout.y} class="handle-line" stroke-width={sw} />
                          <circle cx={hout.x} cy={hout.y} r={nodeR * 0.7} class="handle-dot" />
                        {/if}
                        {#if pt.smooth}
                          <circle cx={ax} cy={ay} r={nodeR} class="anchor-smooth" />
                        {:else}
                          <rect x={ax - nodeR} y={ay - nodeR} width={nodeR * 2} height={nodeR * 2} class="anchor-corner" />
                        {/if}
                      {/each}
                    {/each}
                  {/if}
                </svg>
              </div>
              <div class="outline-metrics">
                <span class="m-pill"><b>{t("spec.upm")}</b> <span class="num">{info.upm}</span></span>
                <span class="m-pill"><b>{t("spec.advance")}</b> <span class="num">{info.advance_width}</span></span>
                <span class="m-pill"><b>{t("spec.lsb")}</b> <span class="num">{info.lsb}</span></span>
                <span class="m-pill"><b>{t("spec.contours")}</b> <span class="num">{info.contour_count}</span></span>
                <span class="m-pill"><b>{t("spec.points")}</b> <span class="num">{info.point_count}</span></span>
              </div>
            </div>
          {/if}
        </div>
      {/if}
      <div class="glyphs">
        {#each shownGlyphs as g (g.code)}
          <button
            type="button"
            class="glyph"
            class:selected={inspectedCode === g.code}
            title={`${t("copy")} U+${g.hex}`}
            style:font-family={previewStyle.family}
            style:font-weight={axisValues.wght ?? previewStyle.weight}
            style:font-style={previewStyle.italic ? "italic" : "normal"}
            style:font-variation-settings={variationCss}
            style:font-feature-settings={featureCss}
            onclick={() => onGlyphClick(g)}>{g.char}</button
          >
        {/each}
      </div>
      {#if filteredGlyphs.length > shownGlyphs.length}
        <div class="glyph-more">
          <button type="button" class="blk-pill" onclick={() => (glyphLimit += 240)}>
            {t("spec.showMore", { n: Math.min(240, filteredGlyphs.length - shownGlyphs.length) })}
          </button>
          <button type="button" class="blk-pill" onclick={() => (glyphLimit = filteredGlyphs.length)}>
            {t("spec.showAll", { n: filteredGlyphs.length })}
          </button>
        </div>
      {/if}
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

  .fmt-bar { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; padding: 10px 14px; border-radius: 12px; background: var(--side); border: 1px solid var(--line); }
  .fmt-pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .fmt-btn { height: 26px; padding: 0 11px; border: 1px solid var(--line2); border-radius: 7px; background: var(--field); color: var(--ink2); font: 600 11.5px var(--sans); transition: all 0.12s; }
  .fmt-btn.bold { font-weight: 800; }
  .fmt-btn.italic { font-style: italic; }
  .fmt-btn.underline { text-decoration: underline; }
  .fmt-btn.strike { text-decoration: line-through; }
  .fmt-btn:hover { border-color: var(--accent-2); color: var(--ink); }
  .fmt-btn.on { background: var(--accent); border-color: var(--accent); color: #fff; box-shadow: var(--glow); }
  .surface-pills { margin-left: auto; display: flex; gap: 4px; }
  .surface-pills .fmt-btn { display: inline-flex; align-items: center; justify-content: center; width: 28px; padding: 0; }
  .surface-light { background: #fbf8f3 !important; color: #1a1615 !important; border-radius: 12px; padding: 14px 18px; border: 1px solid rgba(26, 22, 21, 0.12); }
  .surface-light .n, .surface-light .label { color: #6e625c !important; }
  .surface-dark { background: #100e14 !important; color: #f4efe8 !important; border-radius: 12px; padding: 14px 18px; border: 1px solid rgba(255, 255, 255, 0.1); }
  .surface-dark .n, .surface-dark .label { color: #a79c95 !important; }
  .editorial-preview { display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; border-radius: 12px; background: var(--side); border: 1px solid var(--line); }
  .ed-h1 { margin: 0; font-size: 28px; line-height: 1.2; font-weight: 700; }
  .ed-h2 { margin: 0; font-size: 16px; line-height: 1.4; opacity: 0.8; }
  .ed-cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 18px; font-size: 14.5px; line-height: 1.65; }
  .ed-cols p { margin: 0; }
  .lic-pill { margin-left: 8px; padding: 2px 7px; border-radius: 999px; font: 600 10px var(--sans); background: var(--chip); color: var(--ink2); text-transform: none; letter-spacing: 0; }
  .lic-pill.safe { background: rgba(45, 164, 78, 0.16); color: #2da44e; }
  .lic-pill.warn { background: rgba(214, 56, 81, 0.16); color: var(--danger); }
  .sim-chip { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 999px; border: 1px solid var(--line2); background: var(--field); color: var(--ink); font: 500 12px var(--sans); transition: border-color 0.12s, background-color 0.12s; }
  .sim-chip:hover { border-color: var(--accent); background: var(--accent-soft); }
  .sim-score { font-size: 10.5px; color: var(--accent); }

  .matrix-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 10px; }
  .matrix-card { padding: 10px 14px; border-radius: 10px; background: var(--field); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .matrix-lbl { font: 600 10px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--accent-2); }
  .matrix-sample { font-size: 20px; line-height: 1.35; color: var(--ink); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .waterfall { display: flex; flex-direction: column; }
  .step { display: grid; grid-template-columns: 40px minmax(0, 1fr); align-items: baseline; gap: 12px; padding: 4px 0; }
  .step .n { font-size: 11px; color: var(--ink3); }
  .line { line-height: 1.38; white-space: nowrap; overflow-x: clip; overflow-y: visible; text-overflow: ellipsis; }
  .waterfall .line { white-space: normal; overflow: visible; overflow-wrap: anywhere; line-height: 1.3; padding: 0.12em 0; margin: 0; }
  .sets { display: flex; flex-direction: column; gap: 18px; }
  .set { display: flex; flex-direction: column; gap: 4px; }
  .chars { font-size: 30px; line-height: 1.5; letter-spacing: 0.02em; word-break: break-all; }
  .block { display: flex; flex-direction: column; gap: 8px; }
  .glyph-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .glyph-search {
    height: 26px; width: 210px; padding: 0 9px; border: 1px solid var(--line2); border-radius: 7px;
    background: var(--field); color: var(--ink); font: 500 11.5px var(--sans);
  }
  .glyph-blocks, .glyph-more { display: flex; flex-wrap: wrap; gap: 5px; }
  .blk-pill {
    height: 24px; padding: 0 9px; border: 1px solid var(--line); border-radius: 999px;
    background: var(--field); color: var(--ink2); font: 600 11px var(--sans);
    display: inline-flex; align-items: center; gap: 5px; transition: all 0.12s;
  }
  .blk-pill:hover { border-color: var(--line2); color: var(--ink); }
  .blk-pill.active { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }
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
  .glyph.selected { background: var(--accent-soft); box-shadow: inset 0 0 0 1.5px var(--accent); color: var(--ink); }
  .fork-btn { background: var(--cyan-soft); color: var(--accent-2); }
  .fork-btn:hover { background: var(--accent-2); color: #090b12; }
  .subset-popover {
    display: flex; flex-direction: column; gap: 8px;
    padding: 10px; border-radius: 9px; background: var(--win); border: 1px solid var(--line2);
  }
  .subset-input {
    height: 28px; padding: 0 9px; border-radius: 7px; border: 1px solid var(--line2);
    background: var(--field); color: var(--ink); font: 500 12px var(--mono);
  }
  .subset-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .subset-axes { font-size: 11px; color: var(--ink3); }
  .outline-inspector {
    display: flex; flex-direction: column; gap: 10px;
    padding: 12px 14px; border-radius: 12px; background: var(--side); border: 1px solid var(--line2);
  }
  .outline-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .outline-gname { margin-left: 6px; font-size: 11px; color: var(--accent-2); }
  .outline-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .outline-body { display: flex; flex-wrap: wrap; align-items: center; gap: 16px; }
  .outline-svg-wrap {
    width: 190px; height: 170px; border-radius: 10px; background: var(--win); border: 1px solid var(--line);
    display: grid; place-items: center; padding: 8px; flex: none;
  }
  .outline-svg { width: 100%; height: 100%; overflow: visible; color: var(--ink); }
  .guide-origin { stroke: rgba(56, 189, 248, 0.45); stroke-dasharray: 18 14; }
  .guide-adv { stroke: rgba(56, 189, 248, 0.3); stroke-dasharray: 18 14; }
  .guide-base { stroke: rgba(242, 107, 33, 0.55); }
  .glyph-path { fill: currentColor; fill-opacity: 0.2; stroke: var(--ink); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .handle-line { stroke: var(--accent-2); opacity: 0.75; }
  .handle-dot { fill: var(--accent-2); }
  .anchor-smooth { fill: var(--accent); stroke: var(--win); stroke-width: 1.2; vector-effect: non-scaling-stroke; }
  .anchor-corner { fill: #fbbf24; stroke: var(--win); stroke-width: 1.2; vector-effect: non-scaling-stroke; }
  .outline-metrics { display: flex; flex-wrap: wrap; gap: 8px; flex: 1; }
  .m-pill {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 5px 10px; border-radius: 8px; background: var(--field); border: 1px solid var(--line);
    font-size: 11.5px; color: var(--ink2);
  }
  .m-pill b { font-weight: 600; color: var(--ink3); font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.04em; }
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
