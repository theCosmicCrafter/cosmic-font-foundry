<script>
  import { onMount, onDestroy } from "svelte";
  import {
    Sparkles,
    Bot,
    Cpu,
    Play,
    Square,
    RefreshCw,
    Check,
    Copy,
    X,
    Layers,
    WandSparkles,
    Download,
    CheckCircle2,
    AlertCircle,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import { invokeCmd } from "../../lib/fonts.svelte.js";
  import { inTauri } from "../../lib/settings.svelte.js";

  const BUILTIN_DEFAULT_ENDPOINT = "http://127.0.0.1:17421";
  const OLLAMA_DEFAULT_ENDPOINT = "http://127.0.0.1:11434";
  const OPENAI_DEFAULT_ENDPOINT = "http://127.0.0.1:30000";
  const DEFAULT_MODEL = "hf.co/wahidmounir/VecGlypher-27b-it-Q4_K_M-GGUF";
  const PULL_COMMAND = `ollama run ${DEFAULT_MODEL}`;
  const MAX_EXEMPLARS = 8;
  const MAX_EXEMPLAR_CHIPS = 24;
  const SEED_PRIORITY_CHARS = ["H", "O", "A", "n", "o"];

  const DEFAULT_STYLE_PROMPT =
    "humanist sans-serif, semibold 600 weight, low stroke contrast, normal proportions, smooth clean vector curves, calm and businesslike";

  const STYLE_PRESETS = [
    {
      label: "Humanist Sans 600",
      prompt: DEFAULT_STYLE_PROMPT,
    },
    {
      label: "Art-Deco Luxury Serif",
      prompt:
        "didone serif, regular 400 weight, high stroke contrast, condensed narrow proportions, crisp sharp corners, elegant luxury editorial",
    },
    {
      label: "Brutalist Stencil",
      prompt:
        "industrial stencil display, black 900 weight, uniform monolinear strokes, chunky brutalist geometric forms, bold expressive headline",
    },
    {
      label: "Vintage Rounded",
      prompt:
        "grotesque sans-serif, bold 700 weight, low stroke contrast, soft rounded terminals, playful warm and friendly, vintage retro signage",
    },
    {
      label: "Coding Mono",
      prompt:
        "geometric monospace, medium 500 weight, uniform monolinear strokes, tall x-height proportions, crisp sharp corners, technical futuristic precision",
    },
    {
      label: "Contemporary Ink-Trap",
      prompt:
        "grotesque sans-serif, bold 700 weight, moderate stroke contrast, wide extended proportions, ink-trap notched corners, bold expressive headline",
    },
    {
      label: "Classic Book Serif",
      prompt:
        "old-style serif, regular 400 weight, moderate stroke contrast, normal proportions, teardrop ball terminals, calligraphic broad-nib modulation",
    },
  ];

  // Beginner-friendly guided font trait categories.
  // "single" swaps any existing selection from the same category; "multi" toggles independently.
  const TRAIT_CATEGORIES = [
    {
      id: "style",
      label: "Font Style",
      mode: "single",
      hint: "Overall typeface family — pick one base style",
      items: [
        { label: "Humanist Sans", token: "humanist sans-serif", tip: "Clean, natural modern letters without feet — warm and readable" },
        { label: "Geometric Sans", token: "geometric sans-serif", tip: "Built from crisp circles and straight lines — modern and minimal" },
        { label: "Grotesque Sans", token: "grotesque sans-serif", tip: "Classic neo-grotesque UI and editorial sans like Helvetica or Inter" },
        { label: "Classic Book Serif", token: "old-style serif", tip: "Traditional literary letters with organic angled serifs" },
        { label: "Editorial Serif", token: "transitional serif", tip: "Crisp, balanced magazine and headline serif" },
        { label: "High-Fashion Didone", token: "didone serif", tip: "Luxury editorial style with razor-thin hairlines and bold stems" },
        { label: "Slab Serif", token: "slab serif", tip: "Sturdy, blocky rectangular serifs — bold and mechanical" },
        { label: "Coding Monospace", token: "geometric monospace", tip: "Fixed-width technical letters for code and terminals" },
        { label: "Calligraphic Script", token: "calligraphic script", tip: "Flowing pen- or brush-inspired letterforms" },
        { label: "Industrial Stencil", token: "industrial stencil display", tip: "Cut-out stencil bridges across strokes" },
      ],
    },
    {
      id: "weight",
      label: "Weight (Thickness)",
      mode: "single",
      hint: "How thin or heavy the strokes look — pick one",
      items: [
        { label: "Hairline (100)", token: "hairline 100 weight", tip: "Ultra-delicate needle-thin strokes" },
        { label: "Light (300)", token: "light 300 weight", tip: "Airy, slender strokes" },
        { label: "Regular (400)", token: "regular 400 weight", tip: "Balanced everyday reading thickness" },
        { label: "Medium (500)", token: "medium 500 weight", tip: "Slightly firmer than Regular for crisp UI text" },
        { label: "SemiBold (600)", token: "semibold 600 weight", tip: "Strong, confident subheadings" },
        { label: "Bold (700)", token: "bold 700 weight", tip: "Thick, punchy headlines" },
        { label: "Black / Heavy (900)", token: "black 900 weight", tip: "Massive, ultra-chunky poster weight" },
      ],
    },
    {
      id: "contrast",
      label: "Stroke Contrast",
      mode: "single",
      hint: "Difference between thick stems and thin crossbars — pick one",
      items: [
        { label: "Monoline (Even)", token: "uniform monolinear strokes", tip: "Every part of the letter has the same even thickness" },
        { label: "Low Contrast", token: "low stroke contrast", tip: "Subtle, smooth thickness transitions" },
        { label: "Medium Contrast", token: "moderate stroke contrast", tip: "Classic balanced thick-and-thin strokes" },
        { label: "High Contrast", token: "high stroke contrast", tip: "Dramatic thick verticals paired with delicate hairlines" },
        { label: "Extreme Modulation", token: "extreme stroke modulation", tip: "Showpiece poster contrast between ultra-thick and hairline strokes" },
      ],
    },
    {
      id: "proportions",
      label: "Width & Spacing",
      mode: "single",
      hint: "How narrow, wide, or tall letters sit — pick one",
      items: [
        { label: "Condensed (Narrow)", token: "condensed narrow proportions", tip: "Tall, space-saving narrow letters" },
        { label: "Normal Width", token: "normal proportions", tip: "Balanced standard letter width" },
        { label: "Wide / Extended", token: "wide extended proportions", tip: "Stretched, cinematic wide stance" },
        { label: "Tall Lowercase", token: "tall x-height proportions", tip: "Large, open lowercase bowls for high legibility" },
      ],
    },
    {
      id: "terminals",
      label: "Corners & Terminals",
      mode: "multi",
      hint: "How letter tips and corners are finished — click one or more",
      items: [
        { label: "Sharp Corners", token: "crisp sharp corners", tip: "Clean, crisp 90-degree edges" },
        { label: "Rounded Ends", token: "soft rounded terminals", tip: "Friendly, pill-smooth stroke endings" },
        { label: "Ink Traps", token: "ink-trap notched corners", tip: "Contemporary notched inner corners for crisp display type" },
        { label: "Angled Cuts", token: "sheared angled terminals", tip: "Dynamic diagonal stroke endings" },
        { label: "Ball Terminals", token: "teardrop ball terminals", tip: "Classic circular droplets on letter tips" },
        { label: "Chamfered / Beveled", token: "chamfered beveled corners", tip: "Faceted 45-degree industrial cut corners" },
      ],
    },
    {
      id: "texture",
      label: "Texture & Structure",
      mode: "multi",
      hint: "Surface feel and construction style — click one or more",
      items: [
        { label: "Clean & Smooth", token: "smooth clean vector curves", tip: "Polished, razor-clean studio outlines" },
        { label: "Broad-Nib Pen", token: "calligraphic broad-nib modulation", tip: "Angled pen-ribbon stroke structure" },
        { label: "Hand-Inked / Organic", token: "organic hand-inked texture", tip: "Warm, human hand-crafted edge character" },
        { label: "Brutalist / Modular", token: "chunky brutalist geometric forms", tip: "Bold architectural blocks and stark geometry" },
        { label: "Chiseled / Engraved", token: "chiseled flared capitals", tip: "Stone-carved monumental flair at stem ends" },
      ],
    },
    {
      id: "mood",
      label: "Mood & Vibe",
      mode: "multi",
      hint: "Personality and use case — click one or more",
      items: [
        { label: "Calm & Business", token: "calm and businesslike", tip: "Trustworthy, clean UI or corporate voice" },
        { label: "Luxury & Editorial", token: "elegant luxury editorial", tip: "Refined magazine, fashion, or boutique branding" },
        { label: "Playful & Friendly", token: "playful warm and friendly", tip: "Approachable, upbeat, inviting personality" },
        { label: "Tech & Futuristic", token: "technical futuristic precision", tip: "Engineered sci-fi and developer aesthetic" },
        { label: "Bold Poster", token: "bold expressive headline", tip: "High-impact album cover or poster energy" },
        { label: "Vintage / Retro", token: "vintage retro signage", tip: "Nostalgic 70s/80s print and signage charm" },
      ],
    },
  ];

  const UPPER_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const LOWER_CHARS = "abcdefghijklmnopqrstuvwxyz";
  const DIGIT_CHARS = "0123456789";
  const ALNUM_CHARS = UPPER_CHARS + LOWER_CHARS + DIGIT_CHARS;
  const BASIC_LATIN_CHARS = (() => {
    const seen = new Set(ALNUM_CHARS.split(""));
    let extra = "";
    for (let cp = 0x21; cp <= 0x7e; cp++) {
      const ch = String.fromCharCode(cp);
      if (!seen.has(ch)) extra += ch;
    }
    return ALNUM_CHARS + extra;
  })();

  let {
    project = null,
    activeKey = "65",
    activeChar = "A",
    upm = 1000,
    busy = false,
    onApplyGlyph = null,
    onApplyProject = null,
    onclose = null,
  } = $props();

  // Backend config & status
  let provider = $state("builtin");
  let endpoint = $state(BUILTIN_DEFAULT_ENDPOINT);
  let model = $state(DEFAULT_MODEL);
  let backendStatus = $state(null);
  let checkingBackend = $state(false);
  let updatingRuntime = $state(false);
  let copiedPull = $state(false);
  let copyTimer = null;

  // Model download / pull tracking
  let pulling = $state(false);
  let pullProgress = $state(null);
  let pullSuccess = $state(false);
  let unlistenPull = null;

  const isPrimeVecGlypherModel = $derived.by(() => {
    const target = (model || "").trim();
    return Boolean(target) && /vecglypher/i.test(target);
  });

  const isModelInstalled = $derived.by(() => {
    if (!isPrimeVecGlypherModel || !backendStatus?.models?.length) return false;
    const target = (model || "").trim().toLowerCase();
    return backendStatus.models.some((m) => {
      const lower = m.toLowerCase();
      return (
        lower.includes("vecglypher") &&
        (lower === target || lower.includes(target) || target.includes(lower))
      );
    });
  });

  const primeModelOptions = $derived.by(() => {
    const set = new Set([DEFAULT_MODEL]);
    for (const m of backendStatus?.models ?? []) {
      if (/vecglypher/i.test(m)) set.add(m);
    }
    return [...set];
  });

  function formatBytes(bytes) {
    if (!bytes || bytes <= 0) return "0 B";
    const k = 1024;
    const dm = 2;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i];
  }

  async function startPullModel() {
    if (!inTauri || pulling) return false;
    if (!isPrimeVecGlypherModel) {
      model = DEFAULT_MODEL;
    }
    pulling = true;
    pullSuccess = false;
    pullProgress = {
      status:
        provider === "builtin"
          ? "Initializing built-in GPU engine & model download…"
          : "Contacting Ollama…",
      completed: 0,
      total: 0,
      percent: 0,
      done: false,
      error: null,
    };
    errorMsg = "";
    try {
      await invokeCmd("foundry_ai_pull_model", {
        endpoint,
        model: model.trim(),
        provider,
      });
      await checkBackend();
      return Boolean(isModelInstalled && !pullProgress?.error);
    } catch (err) {
      pulling = false;
      errorMsg = String(err);
      pullProgress = {
        status: "Download failed.",
        completed: 0,
        total: 0,
        percent: 0,
        done: true,
        error: String(err),
      };
      return false;
    }
  }

  async function cancelPullModel() {
    if (!inTauri || !pulling) return;
    try {
      await invokeCmd("foundry_ai_cancel_pull");
    } catch (err) {
      console.warn("Cancel pull failed:", err);
    } finally {
      pulling = false;
    }
  }

  // Style & conditioning
  let stylePrompt = $state(DEFAULT_STYLE_PROMPT);
  let selectedExemplarKeys = $state([]);
  let temperature = $state(0.0);

  // Guided Trait Picker & two-way prompt assembly
  let traitsOpen = $state(true);
  let activeCategoryFilter = $state("all");

  function parsePromptPhrases(raw) {
    return (raw || "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  }

  const activeTokenSet = $derived.by(() => {
    return new Set(parsePromptPhrases(stylePrompt).map((p) => p.toLowerCase()));
  });

  const selectedTraitCount = $derived.by(() => {
    let count = 0;
    for (const cat of TRAIT_CATEGORIES) {
      for (const item of cat.items) {
        if (activeTokenSet.has(item.token.toLowerCase())) count++;
      }
    }
    return count;
  });

  const visibleTraitCategories = $derived.by(() => {
    return activeCategoryFilter === "all"
      ? TRAIT_CATEGORIES
      : TRAIT_CATEGORIES.filter((c) => c.id === activeCategoryFilter);
  });

  function categorySelectedCount(cat) {
    let n = 0;
    for (const item of cat.items) {
      if (activeTokenSet.has(item.token.toLowerCase())) n++;
    }
    return n;
  }

  function toggleTrait(category, item) {
    const phrases = parsePromptPhrases(stylePrompt);
    const targetLower = item.token.toLowerCase();
    const already = phrases.some((p) => p.toLowerCase() === targetLower);
    if (already) {
      stylePrompt = phrases.filter((p) => p.toLowerCase() !== targetLower).join(", ");
      return;
    }
    if (category.mode === "single") {
      const siblingSet = new Set(category.items.map((x) => x.token.toLowerCase()));
      let replaced = false;
      const next = [];
      for (const p of phrases) {
        if (siblingSet.has(p.toLowerCase())) {
          if (!replaced) {
            next.push(item.token);
            replaced = true;
          }
        } else {
          next.push(p);
        }
      }
      if (!replaced) next.push(item.token);
      stylePrompt = next.join(", ");
    } else {
      stylePrompt = [...phrases, item.token].join(", ");
    }
  }

  function clearAllTraits() {
    stylePrompt = "";
  }

  // Scope & foundry post-processing
  let scope = $state("single");
  let autoSpaceKern = $state(true);
  let autoComposites = $state(true);
  let autoFixQa = $state(true);

  // Execution & live streaming
  let generating = $state(false);
  let cancelling = $state(false);
  let progress = $state(null);
  let errorMsg = $state("");
  let statusMsg = $state("");
  let unlistenProgress = null;

  function glyphChar(key, glyph) {
    const cp = Number(glyph?.codepoint ?? key);
    if (Number.isFinite(cp) && cp > 0) {
      try {
        return String.fromCodePoint(cp);
      } catch {
        // Fallback to glyph name or key
      }
    }
    return glyph?.name || key;
  }

  function isNonEmptyGlyph(glyph) {
    return Boolean(glyph?.contours?.length > 0);
  }

  const nonEmptyGlyphEntries = $derived.by(() => {
    const glyphs = project?.glyphs;
    if (!glyphs) return [];
    const list = [];
    for (const [key, glyph] of Object.entries(glyphs)) {
      if (!isNonEmptyGlyph(glyph)) continue;
      const cp = Number(glyph?.codepoint ?? key);
      const ch = glyphChar(key, glyph);
      if (!ch || ch === " " || cp === 32) continue;
      list.push({
        key,
        glyph,
        char: ch,
        codepoint: Number.isFinite(cp) ? cp : 0,
      });
    }
    list.sort((a, b) => a.codepoint - b.codepoint || a.key.localeCompare(b.key));
    return list;
  });

  // Keep up to 24 chips visible, ensuring any already-selected keys remain visible
  const exemplarChips = $derived.by(() => {
    if (nonEmptyGlyphEntries.length <= MAX_EXEMPLAR_CHIPS) {
      return nonEmptyGlyphEntries;
    }
    const selectedSet = new Set(selectedExemplarKeys);
    const prioritized = nonEmptyGlyphEntries.filter((item) => selectedSet.has(item.key));
    const remaining = nonEmptyGlyphEntries.filter((item) => !selectedSet.has(item.key));
    return [...prioritized, ...remaining].slice(0, MAX_EXEMPLAR_CHIPS);
  });

  const selectedExemplars = $derived.by(() => {
    const glyphs = project?.glyphs;
    if (!glyphs) return [];
    return selectedExemplarKeys
      .map((key) => glyphs[key])
      .filter((glyph) => isNonEmptyGlyph(glyph));
  });

  const canSeedDefaults = $derived.by(() => {
    const seedSet = new Set(SEED_PRIORITY_CHARS);
    return nonEmptyGlyphEntries.some((entry) => seedSet.has(entry.char));
  });

  const missingBasicLatinChars = $derived.by(() => {
    const existing = new Set();
    const glyphs = project?.glyphs;
    if (glyphs) {
      for (const [key, glyph] of Object.entries(glyphs)) {
        if (!isNonEmptyGlyph(glyph)) continue;
        const cp = Number(glyph?.codepoint ?? key);
        if (Number.isFinite(cp) && cp > 0) existing.add(cp);
      }
    }
    let out = "";
    for (const ch of BASIC_LATIN_CHARS) {
      const cp = ch.codePointAt(0);
      if (!existing.has(cp)) out += ch;
    }
    return out;
  });

  const targetChars = $derived.by(() => {
    switch (scope) {
      case "single":
        return activeChar || "";
      case "upper":
        return UPPER_CHARS;
      case "lower":
        return LOWER_CHARS;
      case "digits":
        return DIGIT_CHARS;
      case "alnum":
        return ALNUM_CHARS;
      case "missing":
        return missingBasicLatinChars;
      default:
        return "";
    }
  });

  const progressPct = $derived.by(() => {
    if (!progress?.total) return 0;
    return Math.min(100, Math.max(0, Math.round((progress.current / progress.total) * 100)));
  });

  const isDisabled = $derived(busy || generating || pulling);

  // Prune exemplar keys if the underlying project changed and removed glyphs
  $effect(() => {
    const glyphs = project?.glyphs;
    if (!glyphs) {
      if (selectedExemplarKeys.length > 0) selectedExemplarKeys = [];
      return;
    }
    const valid = selectedExemplarKeys.filter((k) => isNonEmptyGlyph(glyphs[k]));
    if (valid.length !== selectedExemplarKeys.length) {
      selectedExemplarKeys = valid;
    }
  });

  function setProvider(next) {
    if (next === provider) return;
    provider = next;
    if (next === "builtin") {
      endpoint = BUILTIN_DEFAULT_ENDPOINT;
    } else if (next === "ollama") {
      endpoint = OLLAMA_DEFAULT_ENDPOINT;
    } else if (
      next === "openai" &&
      (!endpoint.trim() ||
        endpoint.trim() === OLLAMA_DEFAULT_ENDPOINT ||
        endpoint.trim() === BUILTIN_DEFAULT_ENDPOINT)
    ) {
      endpoint = OPENAI_DEFAULT_ENDPOINT;
    }
    backendStatus = null;
    pullSuccess = false;
    checkBackend();
  }

  async function checkBackend() {
    if (checkingBackend) return;
    checkingBackend = true;
    errorMsg = "";
    try {
      const status = await invokeCmd("foundry_ai_check_backend", {
        endpoint: endpoint.trim(),
        provider,
      });
      backendStatus = status;
      if (status?.models?.length && !status.models.includes(model)) {
        const installedVecGlypher = status.models.find((m) => /vecglypher/i.test(m));
        if (installedVecGlypher) {
          model = installedVecGlypher;
        }
      }
    } catch (e) {
      backendStatus = {
        ok: false,
        models: [],
        has_vecglypher: false,
        recommended_pull: PULL_COMMAND,
        message: String(e),
      };
    } finally {
      checkingBackend = false;
    }
  }

  async function applyRuntimeConfig(patch) {
    if (!inTauri || updatingRuntime) return;
    updatingRuntime = true;
    errorMsg = "";
    try {
      backendStatus = await invokeCmd("foundry_ai_set_runtime_config", patch);
    } catch (e) {
      errorMsg = String(e);
    } finally {
      updatingRuntime = false;
    }
  }

  async function stopBuiltinEngine() {
    if (!inTauri || updatingRuntime) return;
    updatingRuntime = true;
    errorMsg = "";
    try {
      backendStatus = await invokeCmd("foundry_ai_stop_builtin");
      statusMsg = "Stopped built-in engine and released GPU/RAM memory.";
    } catch (e) {
      errorMsg = String(e);
    } finally {
      updatingRuntime = false;
    }
  }

  async function pickModelsDir() {
    if (!inTauri || isDisabled || updatingRuntime) return;
    try {
      const picked = await invokeCmd("foundry_ai_choose_models_dir");
      if (picked) {
        await applyRuntimeConfig({ storageDir: picked });
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function pickCustomGguf() {
    if (!inTauri || isDisabled || updatingRuntime) return;
    try {
      const picked = await invokeCmd("foundry_ai_choose_gguf_file");
      if (picked) {
        await applyRuntimeConfig({ customGgufPath: picked });
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function clearCustomGguf() {
    await applyRuntimeConfig({ customGgufPath: "" });
  }

  async function copyPullCommand() {
    try {
      await navigator.clipboard.writeText(PULL_COMMAND);
      copiedPull = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copiedPull = false;
      }, 2000);
    } catch (e) {
      errorMsg = `Clipboard copy failed: ${String(e)}`;
    }
  }

  function toggleExemplar(key) {
    if (selectedExemplarKeys.includes(key)) {
      selectedExemplarKeys = selectedExemplarKeys.filter((k) => k !== key);
    } else if (selectedExemplarKeys.length < MAX_EXEMPLARS) {
      selectedExemplarKeys = [...selectedExemplarKeys, key];
    }
  }

  function seedDefaultExemplars() {
    const picked = [];
    for (const targetChar of SEED_PRIORITY_CHARS) {
      const match = nonEmptyGlyphEntries.find((entry) => entry.char === targetChar);
      if (match && !picked.includes(match.key)) {
        picked.push(match.key);
      }
    }
    selectedExemplarKeys = picked.slice(0, MAX_EXEMPLARS);
  }

  function clearExemplars() {
    selectedExemplarKeys = [];
  }

  async function handleGenerate() {
    if (isDisabled) return;
    const trimmedPrompt = stylePrompt.trim();
    if (!trimmedPrompt) {
      errorMsg = "Pick traits above or describe your font style before generating.";
      return;
    }
    const resolvedPrompt = trimmedPrompt;
    if (scope === "single" && !activeChar) {
      errorMsg = "Select an active glyph to generate.";
      return;
    }
    if (scope !== "single" && (!project || !targetChars.length)) {
      errorMsg = scope === "missing"
        ? "All Basic Latin glyphs already have outlines in this project."
        : "Open or create a foundry project to run batch generation.";
      return;
    }

    if (!isPrimeVecGlypherModel) {
      errorMsg = `Prime model lock: "${model}" is not a VecGlypher model. Switching models will break SVG glyph synthesis — resetting to prime model (${DEFAULT_MODEL}).`;
      model = DEFAULT_MODEL;
      return;
    }

    if (!backendStatus && inTauri) {
      await checkBackend();
    }

    if (provider === "builtin" || provider === "ollama") {
      if (!backendStatus?.ok) {
        errorMsg = `Generation halted: ${provider === "builtin" ? "built-in engine" : "local Ollama backend"} is unavailable (${backendStatus?.message || endpoint}).`;
        return;
      }
      if (!isModelInstalled) {
        if (pulling) {
          errorMsg = `Generation halted: waiting for prime model download (${pullProgress?.percent?.toFixed(1) ?? 0}%) to finish.`;
          return;
        }
        statusMsg =
          provider === "builtin"
            ? "Auto-provisioning built-in llama.cpp GPU engine + VecGlypher-27b-it — generation will run automatically once complete…"
            : `Prime model not yet downloaded. Halting generation and downloading ${model} now — generation will resume automatically once download completes…`;
        const pulledOk = await startPullModel();
        if (!pulledOk || !isModelInstalled) {
          statusMsg = "";
          errorMsg =
            errorMsg ||
            "Generation halted: prime model download did not complete. No fallback model is permitted.";
          return;
        }
      }
    }

    generating = true;
    cancelling = false;
    errorMsg = "";
    statusMsg = "";
    progress = scope === "single"
      ? { current: 0, total: 1, character: activeChar, status: "Running inference…" }
      : { current: 0, total: targetChars.length, character: targetChars[0] || "", status: "Starting batch…" };

    try {
      if (scope === "single") {
        const glyph = await invokeCmd("foundry_ai_generate_glyph", {
          req: {
            endpoint: endpoint.trim(),
            provider,
            model: model.trim(),
            style_prompt: resolvedPrompt,
            character: activeChar,
            exemplar_glyphs: selectedExemplars,
            temperature: Number(temperature),
            upm: Number(upm) || 1000,
          },
        });
        if (glyph) {
          await onApplyGlyph?.(glyph);
          statusMsg = `Generated “${activeChar}” (${glyph.contours?.length ?? 0} contours).`;
        }
      } else {
        const beforeCount = nonEmptyGlyphEntries.length;
        const nextProj = await invokeCmd("foundry_ai_generate_font_batch", {
          req: {
            project,
            endpoint: endpoint.trim(),
            provider,
            model: model.trim(),
            style_prompt: resolvedPrompt,
            chars: targetChars,
            exemplar_keys: selectedExemplarKeys,
            temperature: Number(temperature),
            auto_space_kern: autoSpaceKern,
            auto_composites: autoComposites,
            auto_fix_qa: autoFixQa,
          },
        });
        if (nextProj) {
          await onApplyProject?.(nextProj);
          const afterCount = Object.values(nextProj.glyphs ?? {}).filter((g) => isNonEmptyGlyph(g)).length;
          const added = Math.max(0, afterCount - beforeCount);
          statusMsg = cancelling
            ? `Batch stopped — ${afterCount} non-empty glyph(s) in project.`
            : added > 0
              ? `Batch complete — added ${added} glyph(s) (${afterCount} non-empty in project).`
              : `Batch complete — updated target glyph(s) (${afterCount} non-empty in project).`;
        }
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      generating = false;
      cancelling = false;
      progress = null;
    }
  }

  async function handleCancel() {
    if (!generating || cancelling) return;
    cancelling = true;
    try {
      await invokeCmd("foundry_ai_cancel_batch");
    } catch (e) {
      errorMsg = String(e);
    }
  }

  onMount(async () => {
    if (inTauri) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenProgress = await listen("ai-glyph-progress", ({ payload }) => {
          if (!payload) return;
          progress = payload;
        });
        unlistenPull = await listen("ai-pull-progress", ({ payload }) => {
          if (!payload) return;
          pullProgress = payload;
          if (payload.done) {
            pulling = false;
            if (!payload.error) {
              pullSuccess = true;
              checkBackend();
            }
          }
        });
        checkBackend();
      } catch {
        // Ignore outside Tauri runtime
      }
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
    unlistenPull?.();
    if (copyTimer) clearTimeout(copyTimer);
  });
</script>

<div class="ai-drawer" data-active-key={activeKey}>
  <div class="drawer-head">
    <div class="drawer-title">
      <WandSparkles size={13} />
      <span class="label">VecGlypher AI Glyph Forge</span>
      <span class="badge-chip">
        <Cpu size={10} />
        <span>Auto GPU / CPU</span>
      </span>
    </div>
    {#if onclose}
      <button
        type="button"
        class="mini"
        title="Close"
        aria-label="Close AI glyph generator drawer"
        onclick={onclose}
      >
        <X size={12} />
      </button>
    {/if}
  </div>

  <!-- 1. C++ / CUDA Inference Backend Bar -->
  <section class="panel-block" aria-label="Inference backend">
    <div class="block-head">
      <span class="block-title">
        <Bot size={12} />
        <span>Local Inference Backend</span>
      </span>
      <div class="provider-switch" role="radiogroup" aria-label="Inference provider">
        <button
          type="button"
          class="mode-btn"
          class:active={provider === "builtin"}
          role="radio"
          aria-checked={provider === "builtin"}
          disabled={isDisabled}
          onclick={() => setProvider("builtin")}
        >
          Built-in Engine (Auto)
        </button>
        <button
          type="button"
          class="mode-btn"
          class:active={provider === "ollama"}
          role="radio"
          aria-checked={provider === "ollama"}
          disabled={isDisabled}
          onclick={() => setProvider("ollama")}
        >
          Ollama (Local)
        </button>
        <button
          type="button"
          class="mode-btn"
          class:active={provider === "openai"}
          role="radio"
          aria-checked={provider === "openai"}
          disabled={isDisabled}
          onclick={() => setProvider("openai")}
        >
          vLLM / Custom (OpenAI API)
        </button>
      </div>
    </div>

    <!-- Download Progress Tracker / Status Card (Directly above Model Name) -->
    {#if pulling || (pullProgress && !pullProgress.done)}
      <div class="model-download-card active" aria-live="polite">
        <div class="download-card-header">
          <div class="download-card-title">
            <span class="icon-spin spinning"><RefreshCw size={13} /></span>
            <span class="download-label">Downloading Model: <code>{model}</code></span>
          </div>
          <button
            type="button"
            class="btn-inline danger compact"
            onclick={cancelPullModel}
          >
            Cancel Download
          </button>
        </div>
        <div class="download-bar-track">
          <div
            class="download-bar-fill"
            style="width: {Math.max(2, Math.min(100, pullProgress?.percent || 0))}%"
          ></div>
        </div>
        <div class="download-card-meta">
          <span class="download-bytes">
            {#if (pullProgress?.total || 0) > 0}
              <strong>{formatBytes(pullProgress.completed)}</strong> of {formatBytes(pullProgress.total)} ({pullProgress.percent.toFixed(1)}%)
            {:else}
              {provider === "builtin" ? "Preparing built-in engine…" : "Connecting to Ollama…"}
            {/if}
          </span>
          <span class="download-phase">{pullProgress?.status || "downloading…"}</span>
        </div>
      </div>
    {:else if pullSuccess}
      <div class="model-download-card success" aria-live="polite">
        <div class="download-card-header">
          <div class="download-card-title ok">
            <CheckCircle2 size={13} />
            <span class="download-label">Download Complete! <code>{model}</code> installed and verified.</span>
          </div>
          <button
            type="button"
            class="btn-inline compact"
            onclick={() => (pullSuccess = false)}
          >
            Dismiss
          </button>
        </div>
        <span class="download-subtext">
          {provider === "builtin"
            ? "Built-in llama.cpp GPU engine and VecGlypher-27b-it are installed and active. Ready to forge vector glyphs!"
            : "VecGlypher is active in local Ollama on your GPU. Ready to forge vector glyphs!"}
        </span>
      </div>
    {:else if !isPrimeVecGlypherModel}
      <div class="model-download-card uninstalled">
        <div class="uninstalled-info">
          <div class="uninstalled-title">
            <AlertCircle size={13} />
            <span>Invalid Model — Prime VecGlypher Model Required</span>
          </div>
          <span class="uninstalled-sub">No substitute chat model can emit native VecGlypher SVG paths without breaking output.</span>
        </div>
        <button
          type="button"
          class="btn-inline primary-generate"
          disabled={isDisabled}
          onclick={() => (model = DEFAULT_MODEL)}
        >
          <span>Lock to Prime Model</span>
        </button>
      </div>
    {:else if backendStatus?.ok && !isModelInstalled && (provider === "builtin" || provider === "ollama")}
      <div class="model-download-card uninstalled">
        <div class="uninstalled-info">
          <div class="uninstalled-title">
            <AlertCircle size={13} />
            <span>
              {provider === "builtin"
                ? "Zero-Setup Auto-Install — Built-in GPU Engine + Prime Model"
                : "Prime Model Required — Generation Paused Until Downloaded"}
            </span>
          </div>
          <span class="uninstalled-sub">
            {provider === "builtin"
              ? "Auto-downloads portable llama.cpp Vulkan GPU engine (~37 MB) + VecGlypher-27b-it (~16.6 GB, resumable) · No Ollama needed"
              : "VecGlypher-27b-it (~16.6 GB GGUF) · No alternative model will be substituted"}
          </span>
        </div>
        <button
          type="button"
          class="btn-inline primary-generate"
          disabled={isDisabled || checkingBackend}
          onclick={startPullModel}
        >
          <Download size={12} />
          <span>
            {provider === "builtin"
              ? "Auto-Install Engine + Model"
              : "Download Prime Model (~16.6 GB)"}
          </span>
        </button>
      </div>
    {:else if isModelInstalled}
      <div class="model-installed-badge">
        <Check size={11} />
        <span>Model Installed &amp; Ready on GPU ({backendStatus?.models?.find(m => m.toLowerCase().includes(model.trim().toLowerCase()) || model.trim().toLowerCase().includes(m.toLowerCase())) || model})</span>
      </div>
    {/if}

    <div class="backend-fields">
      <label class="text-field endpoint-field">
        <span class="field-lbl">Endpoint</span>
        <input
          type="text"
          bind:value={endpoint}
          spellcheck="false"
          disabled={isDisabled || provider === "builtin"}
          placeholder={provider === "builtin"
            ? BUILTIN_DEFAULT_ENDPOINT
            : provider === "ollama"
              ? OLLAMA_DEFAULT_ENDPOINT
              : OPENAI_DEFAULT_ENDPOINT}
        />
      </label>

      <label class="text-field model-field">
        <span class="field-lbl">Model</span>
        <input
          type="text"
          bind:value={model}
          list="ai-backend-models"
          spellcheck="false"
          disabled={isDisabled}
          placeholder={DEFAULT_MODEL}
        />
        <datalist id="ai-backend-models">
          {#each primeModelOptions as m (m)}
            <option value={m}></option>
          {/each}
        </datalist>
      </label>

      <div class="backend-actions">
        <button
          type="button"
          class="btn-inline"
          disabled={checkingBackend || isDisabled}
          onclick={checkBackend}
        >
          <span class="icon-spin" class:spinning={checkingBackend}>
            <RefreshCw size={11} />
          </span>
          <span>{checkingBackend ? "Checking…" : "Check Backend"}</span>
        </button>

        <button
          type="button"
          class="btn-inline"
          title={PULL_COMMAND}
          onclick={copyPullCommand}
        >
          {#if copiedPull}
            <Check size={11} />
            <span>Copied</span>
          {:else}
            <Copy size={11} />
            <span>Copy Pull Command</span>
          {/if}
        </button>
      </div>
    </div>

    {#if backendStatus}
      <div class="status-strip" class:ok={backendStatus.ok} class:err={!backendStatus.ok}>
        <span class="status-dot"></span>
        <span class="status-text">
          {backendStatus.message || (backendStatus.ok ? "Backend reachable." : "Backend unreachable.")}
        </span>
        {#if backendStatus.models?.length}
          <span class="status-meta num">{backendStatus.models.length} model(s)</span>
        {/if}
        {#if backendStatus.has_vecglypher}
          <span class="status-tag ok">VecGlypher Ready</span>
        {:else if backendStatus.ok && backendStatus.recommended_pull}
          <span class="status-tag warn" title={backendStatus.recommended_pull}>Pull Recommended</span>
        {/if}
      </div>
    {/if}

    {#if provider === "builtin" && backendStatus}
      {@const activeMode = backendStatus.compute_mode || "auto"}
      <div class="hw-bar">
        <div class="hw-summary-row">
          {#if backendStatus.engine_backend}
            <span class="hw-pill active">{backendStatus.engine_backend}</span>
          {/if}
          {#if backendStatus.active_tier}
            <span class="hw-pill">{backendStatus.active_tier}</span>
          {/if}
          {#if backendStatus.gpu_hold}
            <span class="hw-pill">GPU Hold Active</span>
          {/if}
          <span class="hw-pill">
            {backendStatus.gpu_summary || "GPU / CPU Auto-Fit"}
          </span>
          {#if backendStatus.ram_total_bytes > 0}
            <span class="hw-pill num">
              RAM {formatBytes(backendStatus.ram_avail_bytes)} / {formatBytes(backendStatus.ram_total_bytes)} free
            </span>
          {/if}
          {#if backendStatus.disk_free_bytes > 0}
            <span class="hw-pill num">
              Disk {formatBytes(backendStatus.disk_free_bytes)} free
            </span>
          {/if}
          {#if backendStatus.server_running}
            <span class="hw-pill active">Engine Active</span>
            <button
              type="button"
              class="btn-inline compact danger"
              disabled={isDisabled || updatingRuntime}
              onclick={stopBuiltinEngine}
            >
              Stop / Free VRAM
            </button>
          {/if}
        </div>

        <div class="hw-controls-row">
          <div class="provider-switch" role="radiogroup" aria-label="Acceleration mode">
            <button
              type="button"
              class="mode-btn"
              class:active={activeMode === "auto"}
              role="radio"
              aria-checked={activeMode === "auto"}
              disabled={isDisabled || updatingRuntime}
              onclick={() => applyRuntimeConfig({ computeMode: "auto" })}
            >
              Auto-Fit (GPU + RAM)
            </button>
            <button
              type="button"
              class="mode-btn"
              class:active={activeMode === "gpu"}
              role="radio"
              aria-checked={activeMode === "gpu"}
              disabled={isDisabled || updatingRuntime}
              onclick={() => applyRuntimeConfig({ computeMode: "gpu" })}
            >
              GPU Only
            </button>
            <button
              type="button"
              class="mode-btn"
              class:active={activeMode === "cpu"}
              role="radio"
              aria-checked={activeMode === "cpu"}
              disabled={isDisabled || updatingRuntime}
              onclick={() => applyRuntimeConfig({ computeMode: "cpu" })}
            >
              CPU Only
            </button>
          </div>

          <div class="hw-storage-actions">
            <button
              type="button"
              class="btn-inline compact"
              disabled={isDisabled || updatingRuntime}
              onclick={pickModelsDir}
            >
              Choose Storage Folder…
            </button>
            <button
              type="button"
              class="btn-inline compact"
              disabled={isDisabled || updatingRuntime}
              onclick={pickCustomGguf}
            >
              Use Existing .gguf…
            </button>
            {#if backendStatus.custom_gguf_path}
              <button
                type="button"
                class="btn-inline compact"
                disabled={isDisabled || updatingRuntime}
                onclick={clearCustomGguf}
              >
                Clear Custom .gguf
              </button>
            {/if}
          </div>
        </div>

        <div
          class="hw-path num"
          title={backendStatus.custom_gguf_path || backendStatus.models_dir}
        >
          {backendStatus.custom_gguf_path
            ? `Custom GGUF: ${backendStatus.custom_gguf_path}`
            : `Model Folder: ${backendStatus.models_dir}`}
        </div>
      </div>
    {/if}
  </section>

  <!-- 2. Guided Trait Builder & Style Prompt -->
  <section class="panel-block" aria-label="Style prompt and conditioning">
    <div class="block-head">
      <span class="block-title">
        <Sparkles size={12} />
        <span>Style &amp; Trait Builder</span>
      </span>

      <div class="head-actions">
        <button
          type="button"
          class="trait-toggle"
          class:active={traitsOpen}
          aria-expanded={traitsOpen}
          title="Show or hide beginner-friendly clickable font trait lists"
          onclick={() => (traitsOpen = !traitsOpen)}
        >
          <SlidersHorizontal size={11} />
          <span>Trait Builder</span>
          {#if selectedTraitCount > 0}
            <span class="trait-count num">{selectedTraitCount}</span>
          {/if}
        </button>

        {#if stylePrompt.trim()}
          <button
            type="button"
            class="btn-inline compact"
            title="Clear all style traits and prompt text"
            disabled={isDisabled}
            onclick={clearAllTraits}
          >
            Clear
          </button>
        {/if}

        <div class="temp-switch" role="radiogroup" aria-label="Sampling temperature">
          <button
            type="button"
            class="mode-btn"
            class:active={temperature === 0}
            role="radio"
            aria-checked={temperature === 0}
            disabled={isDisabled}
            onclick={() => (temperature = 0.0)}
          >
            Greedy (0.0 · Exact)
          </button>
          <button
            type="button"
            class="mode-btn"
            class:active={temperature !== 0}
            role="radio"
            aria-checked={temperature !== 0}
            disabled={isDisabled}
            onclick={() => (temperature = 0.7)}
          >
            Sample (0.7)
          </button>
        </div>
      </div>
    </div>

    <div class="preset-row" aria-label="Quick starter style presets">
      {#each STYLE_PRESETS as preset (preset.prompt)}
        <button
          type="button"
          class="preset-pill"
          class:active={stylePrompt.trim() === preset.prompt}
          title={preset.prompt}
          disabled={isDisabled}
          onclick={() => (stylePrompt = preset.prompt)}
        >
          {preset.label}
        </button>
      {/each}
    </div>

    {#if traitsOpen}
      <div class="trait-panel">
        <div class="trait-filter-row" role="tablist" aria-label="Trait categories">
          <button
            type="button"
            role="tab"
            class="trait-tab"
            class:active={activeCategoryFilter === "all"}
            aria-selected={activeCategoryFilter === "all"}
            onclick={() => (activeCategoryFilter = "all")}
          >
            <span>All Traits</span>
            {#if selectedTraitCount > 0}
              <span class="trait-tab-count num">{selectedTraitCount}</span>
            {/if}
          </button>
          {#each TRAIT_CATEGORIES as cat (cat.id)}
            {@const catCount = categorySelectedCount(cat)}
            <button
              type="button"
              role="tab"
              class="trait-tab"
              class:active={activeCategoryFilter === cat.id}
              aria-selected={activeCategoryFilter === cat.id}
              onclick={() => (activeCategoryFilter = cat.id)}
            >
              <span>{cat.label}</span>
              {#if catCount > 0}
                <span class="trait-tab-count num">{catCount}</span>
              {/if}
            </button>
          {/each}
        </div>

        <div
          class="trait-lists"
          class:scroll-all={activeCategoryFilter === "all"}
        >
          {#each visibleTraitCategories as cat (cat.id)}
            <div class="trait-group">
              <div class="trait-group-head">
                <span class="trait-group-title">{cat.label}</span>
                <span class="trait-mode-badge">
                  {cat.mode === "single" ? "Pick 1" : "Pick any"}
                </span>
                <span class="trait-hint">{cat.hint}</span>
              </div>
              <div class="trait-grid" role="group" aria-label={cat.label}>
                {#each cat.items as item (item.token)}
                  {@const isSelected = activeTokenSet.has(item.token.toLowerCase())}
                  <button
                    type="button"
                    class="trait-chip"
                    class:selected={isSelected}
                    aria-pressed={isSelected}
                    title={`${item.tip} (${item.token})`}
                    disabled={isDisabled}
                    onclick={() => toggleTrait(cat, item)}
                  >
                    {item.label}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <div class="prompt-field-head">
      <span>Assembled Style Prompt</span>
      <span class="prompt-field-sub">Click traits above or type custom details</span>
    </div>

    <textarea
      class="prompt-input"
      rows="2"
      bind:value={stylePrompt}
      disabled={isDisabled}
      spellcheck="false"
      placeholder="Click traits above or describe weight, contrast, corners, texture, and mood…"
      onkeydown={(e) => {
        if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
          e.preventDefault();
          handleGenerate();
        }
      }}
    ></textarea>

    <!-- Few-Shot Image-to-Glyph Exemplars -->
    <div class="exemplar-box">
      <div class="exemplar-head">
        <div class="exemplar-title-group">
          <span class="opt-lbl">Few-Shot Exemplars</span>
          <span class="count-pill num">{selectedExemplarKeys.length}/{MAX_EXEMPLARS}</span>
          <span class="cond-mode">
            {selectedExemplarKeys.length > 0
              ? provider === "builtin"
                ? "Vector SVG + Style Conditioning"
                : "Image / SVG + Style Conditioning"
              : "Text-Only Conditioning"}
          </span>
        </div>

        <div class="exemplar-actions">
          <button
            type="button"
            class="btn-inline compact"
            disabled={isDisabled || !canSeedDefaults}
            onclick={seedDefaultExemplars}
          >
            Seed H, O, A, n, o
          </button>
          <button
            type="button"
            class="btn-inline compact"
            disabled={isDisabled || selectedExemplarKeys.length === 0}
            onclick={clearExemplars}
          >
            Clear Seeds (Text-Only)
          </button>
        </div>
      </div>

      {#if exemplarChips.length > 0}
        <div class="chip-grid" role="group" aria-label="Exemplar seed glyphs">
          {#each exemplarChips as item (item.key)}
            {@const isSelected = selectedExemplarKeys.includes(item.key)}
            <button
              type="button"
              class="glyph-chip"
              class:selected={isSelected}
              aria-pressed={isSelected}
              title={`${item.char} (U+${item.codepoint.toString(16).toUpperCase().padStart(4, "0")})`}
              disabled={isDisabled || (!isSelected && selectedExemplarKeys.length >= MAX_EXEMPLARS)}
              onclick={() => toggleExemplar(item.key)}
            >
              {item.char}
            </button>
          {/each}
        </div>
      {:else}
        <div class="empty-hint">
          No non-empty glyphs in project yet — generation will run in Text-Only mode.
        </div>
      {/if}
    </div>
  </section>

  <!-- 3. Generation Scope & Automated Foundry Pipeline -->
  <section class="panel-block" aria-label="Generation scope and pipeline">
    <div class="block-head">
      <span class="block-title">
        <Layers size={12} />
        <span>Generation Scope &amp; Foundry Pipeline</span>
      </span>
      <span class="scope-summary num">
        {#if scope === "single"}
          1 glyph · UPM {upm}
        {:else}
          {targetChars.length} glyph(s)
        {/if}
      </span>
    </div>

    <div class="scope-grid" role="radiogroup" aria-label="Generation scope">
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "single"}
        role="radio"
        aria-checked={scope === "single"}
        disabled={isDisabled}
        onclick={() => (scope = "single")}
      >
        Current Glyph (“{activeChar}”)
      </button>
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "upper"}
        role="radio"
        aria-checked={scope === "upper"}
        disabled={isDisabled}
        onclick={() => (scope = "upper")}
      >
        Uppercase A–Z (26)
      </button>
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "lower"}
        role="radio"
        aria-checked={scope === "lower"}
        disabled={isDisabled}
        onclick={() => (scope = "lower")}
      >
        Lowercase a–z (26)
      </button>
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "digits"}
        role="radio"
        aria-checked={scope === "digits"}
        disabled={isDisabled}
        onclick={() => (scope = "digits")}
      >
        Digits 0–9 (10)
      </button>
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "alnum"}
        role="radio"
        aria-checked={scope === "alnum"}
        disabled={isDisabled}
        onclick={() => (scope = "alnum")}
      >
        Full Alphanumeric (62)
      </button>
      <button
        type="button"
        class="scope-pill"
        class:active={scope === "missing"}
        role="radio"
        aria-checked={scope === "missing"}
        disabled={isDisabled}
        onclick={() => (scope = "missing")}
      >
        Fill Missing Basic Latin ({missingBasicLatinChars.length})
      </button>
    </div>

    {#if scope !== "single"}
      <div class="pipeline-checks">
        <label class="toggle-chip" class:on={autoSpaceKern}>
          <input type="checkbox" bind:checked={autoSpaceKern} disabled={isDisabled} />
          <span>Auto-Space &amp; 24-Band Auto-Kern</span>
        </label>
        <label class="toggle-chip" class:on={autoComposites}>
          <input type="checkbox" bind:checked={autoComposites} disabled={isDisabled} />
          <span>Synthesize 53 Accented Composites</span>
        </label>
        <label class="toggle-chip" class:on={autoFixQa}>
          <input type="checkbox" bind:checked={autoFixQa} disabled={isDisabled} />
          <span>Auto-Fix Winding &amp; Extrema</span>
        </label>
      </div>
    {/if}
  </section>

  <!-- 4. Execution & Live Progress Streaming -->
  {#if generating || progress}
    <div class="progress-panel" role="status" aria-live="polite">
      <div class="progress-head">
        <span class="progress-label">
          {#if progress?.character}
            Generating “{progress.character}” ({progress.current}/{progress.total})…
          {:else}
            Running VecGlypher inference…
          {/if}
        </span>
        {#if progress?.status}
          <span class="progress-status num">{progress.status}</span>
        {/if}
      </div>
      <div class="progress-track">
        <div class="progress-fill" style:width={`${progressPct}%`}></div>
      </div>
    </div>
  {/if}

  {#if errorMsg}
    <div class="feedback-msg error">{errorMsg}</div>
  {:else if statusMsg}
    <div class="feedback-msg ok">{statusMsg}</div>
  {/if}

  <div class="drawer-footer">
    <span class="footer-hint">
      {#if selectedTraitCount > 0}
        {selectedTraitCount} trait{selectedTraitCount === 1 ? "" : "s"} ·
      {/if}
      {#if selectedExemplarKeys.length > 0}
        {selectedExemplarKeys.length} exemplar(s)
      {:else}
        text-only
      {/if} · temp {temperature.toFixed(1)}
    </span>

    <div class="footer-actions">
      {#if generating}
        <button
          type="button"
          class="btn-inline danger"
          disabled={cancelling}
          onclick={handleCancel}
        >
          <Square size={11} />
          <span>{cancelling ? "Stopping…" : "Stop / Cancel"}</span>
        </button>
      {/if}

      <button
        type="button"
        class="btn-inline primary-generate"
        disabled={isDisabled || !stylePrompt.trim() || (scope !== "single" && (!project || !targetChars.length))}
        onclick={handleGenerate}
      >
        <Play size={11} />
        <span>
          {#if pulling}
            Waiting for Prime Model ({pullProgress?.percent?.toFixed(1) ?? "0.0"}%)…
          {:else if generating}
            Generating…
          {:else if scope === "single"}
            Generate “{activeChar}”
          {:else}
            Generate {targetChars.length} Glyph{targetChars.length === 1 ? "" : "s"}
          {/if}
        </span>
      </button>
    </div>
  </div>
</div>

<style>
  .ai-drawer {
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
  .badge-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 18px;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--accent-soft);
    border: 1px solid var(--line2);
    color: var(--accent-2);
    font: 600 10px var(--mono);
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

  .panel-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
  }

  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .block-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 11px var(--sans);
    color: var(--ink);
  }

  .provider-switch,
  .temp-switch {
    display: inline-flex;
    padding: 2px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
    gap: 2px;
    flex-wrap: wrap;
  }
  .mode-btn {
    height: 22px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: 600 10.5px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: background-color 0.14s, color 0.14s;
  }
  .mode-btn:hover:not(:disabled) {
    color: var(--ink);
  }
  .mode-btn.active {
    background: var(--accent);
    color: #fff;
  }
  .mode-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .backend-fields {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .text-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
    min-width: 0;
  }
  .endpoint-field {
    flex: 1 1 190px;
  }
  .model-field {
    flex: 1.6 1 250px;
  }
  .field-lbl {
    font: 600 10px var(--sans);
    color: var(--ink3);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex: none;
  }
  .text-field input {
    width: 100%;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 500 11px var(--mono);
  }
  .backend-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .icon-spin {
    display: inline-flex;
  }
  .icon-spin.spinning {
    animation: ai-spin 0.9s linear infinite;
  }
  @keyframes ai-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .status-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
    font: 500 10.5px var(--sans);
    color: var(--ink2);
  }
  .status-strip.ok {
    border-color: var(--line2);
  }
  .status-strip.err {
    border-color: var(--danger);
    color: var(--danger);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ink3);
    flex: none;
  }
  .status-strip.ok .status-dot {
    background: var(--ok);
  }
  .status-strip.err .status-dot {
    background: var(--danger);
  }
  .status-text {
    flex: 1;
    min-width: 140px;
  }
  .status-meta {
    font: 500 10px var(--mono);
    color: var(--ink3);
  }
  .status-tag {
    padding: 1px 6px;
    border-radius: 4px;
    font: 600 10px var(--mono);
  }
  .status-tag.ok {
    background: var(--accent-soft);
    color: var(--accent-2);
  }
  .status-tag.warn {
    background: var(--chip);
    color: var(--ink2);
  }

  .hw-bar {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
  }
  .hw-summary-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
  }
  .hw-pill {
    padding: 2px 6px;
    border-radius: 5px;
    background: var(--win, var(--side));
    border: 1px solid var(--line);
    color: var(--ink2);
    font: 500 10px var(--sans);
  }
  .hw-pill.active {
    border-color: var(--ok);
    color: var(--ok);
    font-weight: 600;
  }
  .hw-controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
  }
  .hw-storage-actions {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
  }
  .hw-path {
    font: 500 10px var(--mono);
    color: var(--ink3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .trait-toggle {
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink2);
    font: 600 10.5px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }
  .trait-toggle:hover {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .trait-toggle.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-2);
  }
  .trait-count {
    min-width: 15px;
    height: 14px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font: 700 9px var(--mono);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .trait-panel {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
  }
  .trait-filter-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
  }
  .trait-tab {
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--win, var(--side));
    color: var(--ink2);
    font: 600 10px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }
  .trait-tab:hover {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .trait-tab.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .trait-tab-count {
    padding: 0 4px;
    height: 13px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.22);
    font: 700 9px var(--mono);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .trait-tab:not(.active) .trait-tab-count {
    background: var(--accent-soft);
    color: var(--accent-2);
  }

  .trait-lists {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .trait-lists.scroll-all {
    max-height: 240px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .trait-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .trait-group-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .trait-group-title {
    font: 700 10.5px var(--sans);
    color: var(--ink);
  }
  .trait-mode-badge {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--win, var(--side));
    border: 1px solid var(--line);
    color: var(--ink3);
    font: 600 9px var(--mono);
    text-transform: uppercase;
  }
  .trait-hint {
    font: 500 10px var(--sans);
    color: var(--ink3);
  }
  .trait-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .trait-chip {
    height: 23px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--win, var(--side));
    color: var(--ink2);
    font: 600 10.5px var(--sans);
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }
  .trait-chip:hover:not(:disabled) {
    border-color: var(--accent-2);
    color: var(--ink);
  }
  .trait-chip.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .trait-chip:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .prompt-field-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
    font: 600 10.5px var(--sans);
    color: var(--ink2);
  }
  .prompt-field-sub {
    font: 500 10px var(--sans);
    color: var(--ink3);
  }

  .prompt-input {
    width: 100%;
    min-height: 46px;
    resize: vertical;
    padding: 7px 9px;
    border-radius: 7px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 500 11.5px/1.4 var(--sans);
  }
  .prompt-input:focus {
    border-color: var(--accent);
  }

  .preset-row {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .preset-pill {
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink2);
    font: 600 10.5px var(--sans);
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }
  .preset-pill:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .preset-pill.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-2);
  }
  .preset-pill:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .exemplar-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 4px;
    border-top: 1px solid var(--line);
  }
  .exemplar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .exemplar-title-group {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .opt-lbl {
    font: 600 11px var(--sans);
    color: var(--ink2);
  }
  .count-pill {
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--field);
    border: 1px solid var(--line);
    font: 600 10px var(--mono);
    color: var(--ink2);
  }
  .cond-mode {
    font: 500 10.5px var(--sans);
    color: var(--ink3);
  }
  .exemplar-actions {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }

  .chip-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .glyph-chip {
    min-width: 26px;
    height: 24px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 11.5px var(--mono);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }
  .glyph-chip:hover:not(:disabled) {
    border-color: var(--accent-2);
    color: var(--ink);
  }
  .glyph-chip.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .glyph-chip:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .empty-hint {
    font: 500 10.5px var(--sans);
    color: var(--ink3);
  }

  .scope-summary {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }
  .scope-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .scope-pill {
    height: 25px;
    padding: 0 9px;
    border-radius: 7px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }
  .scope-pill:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .scope-pill.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--ink);
  }
  .scope-pill:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .pipeline-checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 2px;
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

  .progress-panel {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
  }
  .progress-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .progress-label {
    font: 600 11px var(--sans);
    color: var(--ink);
  }
  .progress-status {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }
  .progress-track {
    height: 6px;
    border-radius: 999px;
    background: var(--line);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.16s ease-out;
  }

  .feedback-msg {
    padding: 6px 9px;
    border-radius: 7px;
    font: 500 11px var(--sans);
    border: 1px solid var(--line);
  }
  .feedback-msg.error {
    color: var(--danger);
    border-color: var(--danger);
    background: var(--field);
  }
  .feedback-msg.ok {
    color: var(--ok);
    border-color: var(--ok);
    background: var(--field);
  }

  .drawer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .footer-hint {
    font: 500 10.5px var(--mono);
    color: var(--ink3);
  }
  .footer-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }

  .model-download-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-radius: 7px;
    background: var(--field);
    border: 1px solid var(--line2);
  }
  .model-download-card.active {
    border-color: var(--accent);
    background: var(--accent-soft, var(--field));
  }
  .model-download-card.success {
    border-color: var(--ok);
    background: var(--field);
  }
  .model-download-card.uninstalled {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-color: var(--warn, #e5a50a);
    background: var(--field);
  }
  .download-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .download-card-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 600 11px var(--sans);
    color: var(--ink);
  }
  .download-card-title.ok {
    color: var(--ok);
  }
  .download-label code {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--accent-2);
  }
  .download-subtext {
    font: 500 10.5px var(--sans);
    color: var(--ink2);
  }
  .uninstalled-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .uninstalled-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 11px var(--sans);
    color: var(--warn, #e5a50a);
  }
  .uninstalled-sub {
    font: 500 10px var(--mono);
    color: var(--ink3);
  }
  .download-bar-track {
    width: 100%;
    height: 7px;
    border-radius: 999px;
    background: var(--line2);
    overflow: hidden;
  }
  .download-bar-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.2s ease-out;
  }
  .download-card-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font: 500 10.5px var(--mono);
    color: var(--ink2);
  }
  .download-phase {
    color: var(--ink3);
    font-size: 10px;
    text-transform: capitalize;
  }
  .model-installed-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--accent-soft, var(--field));
    border: 1px solid var(--line2);
    color: var(--ok);
    font: 600 10.5px var(--sans);
    width: fit-content;
  }

  .btn-inline {
    height: 26px;
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
  .btn-inline.compact {
    height: 22px;
    padding: 0 7px;
    font-size: 10.5px;
  }
  .btn-inline:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .btn-inline.primary-generate:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn-inline.danger:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }
  .btn-inline:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
