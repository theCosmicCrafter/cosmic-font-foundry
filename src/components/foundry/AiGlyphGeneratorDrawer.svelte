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
    Braces,
    Dices,
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

  const STYLE_PRESETS = [
    {
      label: "Humanist Sans 600",
      prompt: "humanist sans-serif, 600 weight, calm, business, normal style",
    },
    {
      label: "Art-Deco Serif",
      prompt: "high-contrast, narrow, art-deco, luxury editorial serif",
    },
    {
      label: "Brutalist Stencil",
      prompt: "chunky, brutalist, industrial stencil, heavy display",
    },
    {
      label: "Vintage Rounded",
      prompt: "active, cute, vintage, rounded warm display",
    },
    {
      label: "Geometric Mono",
      prompt: "geometric monospace, technical, sharp, coding",
    },
    {
      label: "Wildcard Sampler",
      prompt: "__sans__, __weight__, __mood__, __contrast__, __era__ style",
    },
    {
      label: "Serif Study",
      prompt: "__serif__, __weight__, __texture__, __proportions__, __terminals__",
    },
  ];

  // Prompt wildcard collections — `__name__` expands to one random phrase per run.
  // `{a|b|c}` inline choices are resolved by expandChoices() before token expansion.
  const WILDCARD_COLLECTIONS = {
    weight: [
      "hairline weight",
      "light weight",
      "regular weight",
      "medium weight",
      "semibold weight",
      "bold weight",
      "black weight",
    ],
    contrast: [
      "low stroke contrast",
      "moderate stroke contrast",
      "high stroke contrast",
      "extreme stroke modulation",
    ],
    serif: [
      "old-style serif",
      "transitional serif",
      "didone serif",
      "slab serif",
      "wedge serif",
      "humanist serif",
      "geometric serif",
    ],
    sans: [
      "grotesque sans-serif",
      "neo-grotesque sans-serif",
      "humanist sans-serif",
      "geometric sans-serif",
      "square sans-serif",
    ],
    mood: [
      "calm and businesslike",
      "confident and authoritative",
      "playful and friendly",
      "elegant and luxurious",
      "technical and precise",
      "warm and approachable",
      "bold and expressive",
      "minimal and neutral",
    ],
    texture: [
      "smooth uniform strokes",
      "subtle stroke modulation",
      "ink-trap terminals",
      "brushed stroke texture",
      "calligraphic stroke contrast",
    ],
    era: [
      "contemporary",
      "modern",
      "retro",
      "vintage",
      "futuristic",
      "timeless classic",
    ],
    terminals: [
      "flat terminals",
      "rounded terminals",
      "angled terminals",
      "slab terminals",
      "sheared terminals",
    ],
    proportions: [
      "narrow proportions",
      "normal proportions",
      "wide proportions",
      "condensed proportions",
      "extended proportions",
    ],
    spacing: [
      "tight spacing",
      "normal spacing",
      "generous spacing",
      "open spacing",
    ],
    display: [
      "display size",
      "text size",
      "headline",
      "poster",
      "caption",
    ],
    mono: ["monospace", "proportional"],
  };

  const WILDCARD_NAMES = Object.keys(WILDCARD_COLLECTIONS);

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
  let stylePrompt = $state("humanist sans-serif, 600 weight, calm, clean, normal style");
  let selectedExemplarKeys = $state([]);
  let temperature = $state(0.0);

  // Prompt wildcards & resolved preview
  let promptEl = $state(null);
  let wildOpen = $state(false);
  let previewNonce = $state(0);
  let copiedResolved = $state(false);
  let resolvedCopyTimer = null;

  function pickOption(options, rng) {
    return options[Math.floor(rng() * options.length)];
  }

  // Expands {a|b|c} choice groups inside-out so nested groups resolve correctly.
  function expandChoices(text, rng) {
    let out = text;
    for (let guard = 0; guard < 64; guard++) {
      const m = out.match(/\{([^{}]*)\}/);
      if (!m) break;
      const options = m[1].split("|").map((s) => s.trim()).filter(Boolean);
      const repl = options.length ? pickOption(options, rng) : m[0];
      out = out.slice(0, m.index) + repl + out.slice(m.index + m[0].length);
    }
    return out;
  }

  // Resolves {a|b} choices then __collection__ tokens. Unknown tokens stay literal
  // and are reported so the preview can flag them. Each call rolls independently —
  // one roll per Generate keeps batch glyphs stylistically consistent.
  function resolvePrompt(raw, rng = Math.random) {
    const used = new Set();
    const unknown = new Set();
    let text = expandChoices(raw, rng);
    text = text.replace(/__([A-Za-z0-9_-]+)__/g, (whole, name) => {
      const coll = WILDCARD_COLLECTIONS[name.toLowerCase()];
      if (!coll?.length) {
        unknown.add(name);
        return whole;
      }
      used.add(name.toLowerCase());
      return pickOption(coll, rng);
    });
    text = text
      .replace(/\s*,\s*/g, ", ")
      .replace(/,\s*,+/g, ",")
      .replace(/\s{2,}/g, " ")
      .trim();
    return { text, used: [...used], unknown: [...unknown] };
  }

  const resolvedPreview = $derived.by(() => {
    void previewNonce;
    return resolvePrompt(stylePrompt || "");
  });

  const wildcardTokenCount = $derived.by(() => {
    const m = (stylePrompt || "").match(/__[A-Za-z0-9_-]+__/g);
    return m ? m.length : 0;
  });

  const choiceGroupCount = $derived.by(() => {
    const m = (stylePrompt || "").match(/\{[^{}]*\|[^{}]*\}/g);
    return m ? m.length : 0;
  });

  function insertWildcard(name) {
    const token = `__${name}__`;
    const el = promptEl;
    const start = el?.selectionStart ?? (stylePrompt || "").length;
    const end = el?.selectionEnd ?? start;
    stylePrompt = (stylePrompt || "").slice(0, start) + token + (stylePrompt || "").slice(end);
    previewNonce += 1;
    requestAnimationFrame(() => {
      el?.focus();
      const pos = start + token.length;
      el?.setSelectionRange(pos, pos);
    });
  }

  function rerollPreview() {
    previewNonce += 1;
  }

  async function copyResolvedPrompt() {
    try {
      await navigator.clipboard.writeText(resolvedPreview.text);
      copiedResolved = true;
      if (resolvedCopyTimer) clearTimeout(resolvedCopyTimer);
      resolvedCopyTimer = setTimeout(() => {
        copiedResolved = false;
      }, 1600);
    } catch {
      // Clipboard unavailable — non-fatal
    }
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
      errorMsg = "Enter a VecGlypher style prompt before generating.";
      return;
    }
    const resolvedPrompt = resolvePrompt(trimmedPrompt).text;
    if (!resolvedPrompt) {
      errorMsg = "Prompt resolves to empty after wildcard expansion.";
      return;
    }
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
    if (resolvedCopyTimer) clearTimeout(resolvedCopyTimer);
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
          <span class="hw-pill">
            {backendStatus.gpu_summary || "Vulkan GPU / CPU Auto-Fit"}
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

  <!-- 2. VecGlypher Style Prompt & Conditioning Mode -->
  <section class="panel-block" aria-label="Style prompt and conditioning">
    <div class="block-head">
      <span class="block-title">
        <Sparkles size={12} />
        <span>VecGlypher Style Prompt</span>
      </span>

      <div class="head-actions">
        <button
          type="button"
          class="wild-toggle"
          class:active={wildOpen}
          class:has-tokens={wildcardTokenCount > 0}
          aria-expanded={wildOpen}
          title={"Wildcard collections and {a|b} inline choices — re-rolled on every Generate"}
          onclick={() => (wildOpen = !wildOpen)}
        >
          <Braces size={11} />
          <span>Wildcards</span>
          {#if wildcardTokenCount > 0}
            <span class="wild-count num">{wildcardTokenCount}</span>
          {/if}
        </button>

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

    <textarea
      class="prompt-input"
      rows="2"
      bind:this={promptEl}
      bind:value={stylePrompt}
      disabled={isDisabled}
      spellcheck="false"
      placeholder={"Describe weight, contrast, mood, serif style, proportions… try __sans__ or {old-style|didone}"}
      onkeydown={(e) => {
        if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
          e.preventDefault();
          handleGenerate();
        }
      }}
    ></textarea>

    {#if wildOpen}
      <div class="wild-panel">
        <div class="wild-grid" role="group" aria-label="Wildcard collections">
          {#each WILDCARD_NAMES as name (name)}
            <button
              type="button"
              class="wild-chip"
              title={`${name} → ${WILDCARD_COLLECTIONS[name].join(" / ")}`}
              disabled={isDisabled}
              onclick={() => insertWildcard(name)}
            >
              __{name}__
            </button>
          {/each}
        </div>
        <div class="wild-hint">
          <code>{"{a|b|c}"}</code> inline choice · <code>__token__</code> collection · each Generate re-rolls all wildcards
        </div>
      </div>
    {/if}

    <div
      class="resolve-panel"
      class:active={resolvedPreview.used.length > 0 || resolvedPreview.unknown.length > 0}
      aria-live="polite"
    >
      <div class="resolve-head">
        <span class="resolve-lbl">Resolved</span>
        <span class="resolve-count num" title="Characters in resolved prompt">{resolvedPreview.text.length}</span>
        <div class="resolve-chips">
          {#each resolvedPreview.used as u (u)}
            <span class="resolve-chip">__{u}__</span>
          {/each}
          {#if choiceGroupCount > 0}
            <span class="resolve-chip choice" title={"Inline {a|b} choice groups in prompt"}>
              <Dices size={9} />{choiceGroupCount}
            </span>
          {/if}
        </div>
        <div class="resolve-actions">
          <button
            type="button"
            class="resolve-btn"
            title="Re-roll wildcard resolution"
            disabled={isDisabled || (wildcardTokenCount === 0 && choiceGroupCount === 0)}
            onclick={rerollPreview}
          >
            <Dices size={10} />
          </button>
          <button
            type="button"
            class="resolve-btn"
            title="Copy resolved prompt"
            disabled={!resolvedPreview.text}
            onclick={copyResolvedPrompt}
          >
            {#if copiedResolved}
              <Check size={10} />
            {:else}
              <Copy size={10} />
            {/if}
          </button>
        </div>
      </div>
      <div class="resolve-text" class:muted={!resolvedPreview.text}>
        {resolvedPreview.text || "Prompt resolves to empty."}
      </div>
      {#if resolvedPreview.unknown.length > 0}
        <div class="resolve-warn">
          Unknown token{resolvedPreview.unknown.length > 1 ? "s" : ""} left literal: {resolvedPreview.unknown.map((u) => `__${u}__`).join(", ")}
        </div>
      {/if}
    </div>

    <div class="preset-row">
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
      {#if wildcardTokenCount > 0 || choiceGroupCount > 0}
        {wildcardTokenCount + choiceGroupCount} wildcard{wildcardTokenCount + choiceGroupCount === 1 ? "" : "s"} re-roll per run ·
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

  .wild-toggle {
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
  .wild-toggle:hover {
    color: var(--ink);
    border-color: var(--accent-2);
  }
  .wild-toggle.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-2);
  }
  .wild-toggle.has-tokens {
    border-color: var(--accent);
  }
  .wild-count {
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

  .wild-panel {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 7px 8px;
    border-radius: 8px;
    background: var(--field);
    border: 1px dashed var(--line2);
  }
  .wild-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .wild-chip {
    height: 21px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--win, var(--side));
    color: var(--ink2);
    font: 600 10px var(--mono);
    transition: border-color 0.12s, color 0.12s, background-color 0.12s;
  }
  .wild-chip:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent-2);
    background: var(--accent-soft);
  }
  .wild-chip:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .wild-hint {
    font: 500 10px var(--sans);
    color: var(--ink3);
  }
  .wild-hint code {
    font: 600 10px var(--mono);
    color: var(--ink2);
    background: var(--win, var(--side));
    padding: 0 4px;
    border-radius: 4px;
    border: 1px solid var(--line);
  }

  .resolve-panel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line);
    transition: border-color 0.14s;
  }
  .resolve-panel.active {
    border-color: var(--accent);
  }
  .resolve-head {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .resolve-lbl {
    font: 700 9.5px var(--sans);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink3);
    flex: none;
  }
  .resolve-count {
    font: 500 10px var(--mono);
    color: var(--ink3);
    flex: none;
  }
  .resolve-chips {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    flex: 1;
    min-width: 0;
  }
  .resolve-chip {
    height: 16px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    color: var(--accent-2);
    font: 600 9.5px var(--mono);
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .resolve-chip.choice {
    background: var(--field);
    border-color: var(--line2);
    color: var(--ink2);
  }
  .resolve-actions {
    display: inline-flex;
    gap: 3px;
    flex: none;
  }
  .resolve-btn {
    width: 20px;
    height: 20px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--win, var(--side));
    color: var(--ink2);
    display: inline-grid;
    place-items: center;
    transition: border-color 0.12s, color 0.12s;
  }
  .resolve-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent-2);
  }
  .resolve-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .resolve-text {
    font: 500 11px/1.45 var(--mono);
    color: var(--ink);
    word-break: break-word;
    max-height: 54px;
    overflow-y: auto;
  }
  .resolve-text.muted {
    color: var(--ink3);
  }
  .resolve-warn {
    font: 500 10px var(--sans);
    color: var(--warn, #e5a50a);
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
