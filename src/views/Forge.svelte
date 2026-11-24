<script>
  import { onDestroy } from "svelte";
  import {
    Download, Sparkles, RotateCcw, FlipHorizontal2, Eraser, Check, Contrast,
    Undo2, Redo2, Save, FolderUp, Spline, Layers, FileCode, Upload, Search, Scan,
    PenTool, ArrowLeftRight, Combine, WandSparkles, ShieldCheck, PanelLeft,
    ChevronDown, ChevronUp, MoreHorizontal,
  } from "@lucide/svelte";
  import VectorGlyphEditor from "../components/foundry/VectorGlyphEditor.svelte";
  import ImageTraceDrawer from "../components/foundry/ImageTraceDrawer.svelte";
  import GhostPicker from "../components/foundry/GhostPicker.svelte";
  import SpacingKerningStrip from "../components/foundry/SpacingKerningStrip.svelte";
  import LayoutCompositesPanel from "../components/foundry/LayoutCompositesPanel.svelte";
  import StylesVariablePanel from "../components/foundry/StylesVariablePanel.svelte";
  import FoundryQaPanel from "../components/foundry/FoundryQaPanel.svelte";
  import AiGlyphGeneratorDrawer from "../components/foundry/AiGlyphGeneratorDrawer.svelte";
  import {
    MATRIX_COLS,
    MATRIX_ROWS,
    PRESETS,
    SHAPES,
    CHAR_GROUPS,
    buildPresetGlyphs,
    cloneGrid,
    resizeGlyphsResolution,
  } from "../lib/forge.js";
  import { fonts, installForgedFont, invokeCmd } from "../lib/fonts.svelte.js";
  import { settings, inTauri } from "../lib/settings.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let {
    sample = "The quick cosmic fox jumps over the lazy dog 0123456789",
    onInstalled = null,
    forkFont = null,
    onForkConsumed = null,
  } = $props();

  // ---- Studio Mode ("modular" | "vector") ----
  let mode = $state("modular");

  // ---- Shared Family Metadata ----
  let familyName = $state("Nebula Sans");
  let styleName = $state("Regular");

  // ---- Modular Matrix State ----
  let activePreset = $state("cosmic");
  let shape = $state("chamfer");
  let weight = $state(88);
  let contrast = $state(96);
  let slant = $state(0);
  let tracking = $state(80);
  let corner = $state(26);
  let removeOverlap = $state(true);
  let smoothCorners = $state(true);
  let opticalOffset = $state(0);

  let gridScale = $state(1); // 1 = 7x9 | 2 = 14x18
  const cols = $derived(MATRIX_COLS * gridScale);
  const rows = $derived(MATRIX_ROWS * gridScale);

  let glyphs = $state(buildPresetGlyphs("cosmic"));
  let glyphWidths = $state({});
  let undoStack = $state([]);
  let redoStack = $state([]);
  let projectInput = $state();
  let svgFileInput = $state();
  let strokeSnapshotTaken = false;
  let activeChar = $state("A");
  let painting = $state(null); // 1 | 0 | null while dragging

  // ---- Vector Foundry State ----
  let vectorProject = $state(null);
  let activeVectorKey = $state("65");
  let vectorUndoStack = $state([]);
  let vectorRedoStack = $state([]);
  let showVectorNodes = $state(true);
  let applyOpToAll = $state(false);
  let vectorGlyphQuery = $state("");
  let svgDrawerOpen = $state(false);
  let traceDrawerOpen = $state(false);
  let fileMenuOpen = $state(false);
  let glyphRailOpen = $state(true);
  let specimenExpanded = $state(false);
  let svgImportText = $state("");
  let vectorBusy = $state(false);
  let forking = $state(false);
  let ghostEntry = $state(null);
  let ghostPath = $state("");
  let caliperResults = $state([]);
  let vectorTab = $state("draw"); // "draw" | "ai" | "space" | "layout" | "styles" | "qa"
  let qaReport = $state(null);
  let ghostSeq = 0;

  const showGlyphRail = $derived(
    glyphRailOpen && (vectorTab === "draw" || vectorTab === "space" || vectorTab === "layout"),
  );

  // ---- Live Preview & Compile Output ----
  let testPhrase = $state("");
  let testSize = $state(38);
  let forgeFmt = $state({ bold: false, italic: false, underline: false, strike: false, uppercase: false });
  const forgeDec = $derived(
    [forgeFmt.underline && "underline", forgeFmt.strike && "line-through"].filter(Boolean).join(" ") || "none",
  );
  let installing = $state(false);
  let liveFamilyId = $state("ForgePreview");
  let compileError = $state("");
  let compiledBytes = $state(new Uint8Array(0));
  let compileStats = $state({ glyphCount: 0, contours: null, points: null });

  const currentGrid = $derived(glyphs[activeChar] ?? Array.from({ length: rows }, () => new Array(cols).fill(0)));
  const currentGlyphWidth = $derived(glyphWidths[activeChar] ?? 0);
  const effectivePhrase = $derived(testPhrase.trim() || sample || "COSMIC FONT FOUNDRY 0123456789");

  const vectorGlyphEntries = $derived.by(() => {
    if (!vectorProject?.glyphs) return [];
    const q = vectorGlyphQuery.trim().toLowerCase().replace(/^u\+/, "");
    return Object.entries(vectorProject.glyphs)
      .map(([key, g]) => {
        const cp = g.codepoint ?? (Number(key) || 0);
        const char = cp > 0 ? String.fromCodePoint(cp) : g.name || "?";
        const hex = cp.toString(16).toUpperCase().padStart(4, "0");
        return { key, glyph: g, cp, char, hex };
      })
      .filter((item) => {
        if (!q) return true;
        return (
          item.char.toLowerCase() === q ||
          item.hex.toLowerCase().includes(q) ||
          (item.glyph.name || "").toLowerCase().includes(q)
        );
      })
      .sort((a, b) => a.cp - b.cp);
  });

  const activeVectorGlyph = $derived(vectorProject?.glyphs?.[activeVectorKey] ?? null);
  const activeVectorChar = $derived.by(() => {
    const g = activeVectorGlyph;
    if (!g) return "?";
    return g.codepoint > 0 ? String.fromCodePoint(g.codepoint) : g.name || "?";
  });

  let activeFontFace = null;
  let compileSeq = 0;

  function buildModularReq() {
    return {
      family_name: familyName,
      style_name: styleName,
      weight,
      contrast,
      slant,
      tracking,
      corner,
      shape,
      glyphs,
      glyph_widths: glyphWidths,
      cols,
      rows,
      remove_overlap: removeOverlap,
      smooth_corners: smoothCorners,
      optical_offset: opticalOffset,
    };
  }

  function syncedVectorProject(proj = vectorProject) {
    if (!proj) return null;
    if (proj.family_name === familyName && proj.style_name === styleName) return proj;
    return { ...proj, family_name: familyName, style_name: styleName };
  }

  function applyLiveFontFace(bytes, seq) {
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
  }

  // ---- Live Compilation Effect (Modular + Vector) ----
  $effect(() => {
    if (mode === "modular") {
      const jsPayload = {
        familyName,
        styleName,
        weight,
        contrast,
        slant,
        tracking,
        corner,
        shape,
        glyphs,
        glyphWidths,
        cols,
        rows,
      };
      const rustReq = buildModularReq();
      const seq = ++compileSeq;

      if (!inTauri) {
        if (import.meta.env.DEV) {
          import("../lib/dev-forge-compiler.js")
            .then(({ compileTrueTypeFont }) => {
              if (seq !== compileSeq) return;
              const bytes = compileTrueTypeFont(jsPayload);
              compiledBytes = bytes;
              compileStats = { glyphCount: Object.keys(glyphs).length + 2, contours: null, points: null };
              compileError = "";
              applyLiveFontFace(bytes, seq);
            })
            .catch((err) => {
              if (seq === compileSeq) compileError = String(err);
            });
        }
        return;
      }

      const timer = setTimeout(() => {
        invokeCmd("foundry_compile_modular", { req: rustReq })
          .then((res) => {
            if (seq !== compileSeq) return;
            const bytes = new Uint8Array(res.ttf_bytes);
            compiledBytes = bytes;
            compileStats = {
              glyphCount: res.glyph_count,
              contours: res.total_contours,
              points: res.total_points,
            };
            compileError = "";
            applyLiveFontFace(bytes, seq);
          })
          .catch((err) => {
            if (seq === compileSeq) compileError = String(err);
          });
      }, 60);
      return () => clearTimeout(timer);
    }

    if (mode === "vector" && vectorProject) {
      const proj = syncedVectorProject(vectorProject);
      const seq = ++compileSeq;
      if (!inTauri) {
        compileStats = { glyphCount: Object.keys(proj.glyphs ?? {}).length, contours: null, points: null };
        return;
      }
      const timer = setTimeout(() => {
        invokeCmd("foundry_compile_project", { project: proj, cleanOutlines: false })
          .then((res) => {
            if (seq !== compileSeq) return;
            const bytes = new Uint8Array(res.ttf_bytes);
            compiledBytes = bytes;
            compileStats = {
              glyphCount: res.glyph_count,
              contours: res.total_contours,
              points: res.total_points,
            };
            compileError = "";
            applyLiveFontFace(bytes, seq);
          })
          .catch((err) => {
            if (seq === compileSeq) compileError = String(err);
          });
      }, 60);
      return () => clearTimeout(timer);
    }
  });

  // ---- Fork-to-Foundry Effect ----
  async function loadForkedFont(target) {
    mode = "vector";
    forking = true;
    compileError = "";
    try {
      const proj = await invokeCmd("foundry_fork_font", {
        path: target.path,
        newFamily: `${target.name} Remix`,
      });
      loadVectorProject(proj);
      notify("ok", t("forge.forkedOk", { n: proj.family_name }), "");
    } catch (err) {
      compileError = String(err);
      notify("err", t("err.one"), String(err));
    } finally {
      forking = false;
    }
  }

  $effect(() => {
    if (!forkFont) return;
    const target = forkFont;
    onForkConsumed?.();
    loadForkedFont(target);
  });

  onDestroy(() => {
    if (activeFontFace) document.fonts.delete(activeFontFace);
  });

  // ---- Vector Project Helpers ----
  function pickInitialVectorKey(proj) {
    const keys = Object.keys(proj?.glyphs ?? {});
    if (keys.includes("65")) return "65";
    const upper = keys.find((k) => {
      const cp = proj.glyphs[k]?.codepoint ?? Number(k);
      return cp >= 65 && cp <= 90;
    });
    return upper ?? keys[0] ?? "";
  }

  function loadVectorProject(proj) {
    vectorProject = proj;
    if (typeof proj.family_name === "string" && proj.family_name) familyName = proj.family_name;
    if (typeof proj.style_name === "string" && proj.style_name) styleName = proj.style_name;
    activeVectorKey = pickInitialVectorKey(proj);
    vectorUndoStack = [];
    vectorRedoStack = [];
    vectorGlyphQuery = "";
    svgDrawerOpen = false;
    mode = "vector";
  }

  function cloneProject(proj) {
    return JSON.parse(JSON.stringify(proj));
  }

  function pushVectorUndo() {
    if (!vectorProject) return;
    vectorUndoStack = [...vectorUndoStack.slice(-39), cloneProject(vectorProject)];
    vectorRedoStack = [];
  }

  function undoForge() {
    if (mode === "vector") {
      if (!vectorUndoStack.length || !vectorProject) return;
      const prev = vectorUndoStack[vectorUndoStack.length - 1];
      vectorRedoStack = [...vectorRedoStack.slice(-39), cloneProject(vectorProject)];
      vectorUndoStack = vectorUndoStack.slice(0, -1);
      vectorProject = prev;
      return;
    }
    if (!undoStack.length) return;
    const prev = undoStack[undoStack.length - 1];
    redoStack = [...redoStack.slice(-59), makeSnapshot()];
    undoStack = undoStack.slice(0, -1);
    applySnapshot(prev);
  }

  function redoForge() {
    if (mode === "vector") {
      if (!vectorRedoStack.length || !vectorProject) return;
      const next = vectorRedoStack[vectorRedoStack.length - 1];
      vectorUndoStack = [...vectorUndoStack.slice(-39), cloneProject(vectorProject)];
      vectorRedoStack = vectorRedoStack.slice(0, -1);
      vectorProject = next;
      return;
    }
    if (!redoStack.length) return;
    const next = redoStack[redoStack.length - 1];
    undoStack = [...undoStack.slice(-59), makeSnapshot()];
    redoStack = redoStack.slice(0, -1);
    applySnapshot(next);
  }

  const canUndo = $derived(mode === "vector" ? vectorUndoStack.length > 0 : undoStack.length > 0);
  const canRedo = $derived(mode === "vector" ? vectorRedoStack.length > 0 : redoStack.length > 0);

  // ---- SVG Geometry Rendering Helpers ----
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

  function contoursToSvgPath(contours, ascender) {
    if (!contours?.length) return "";
    const absHandles = detectAbsoluteHandles(contours);
    const fmt = (n) => Math.round(n * 100) / 100;
    const parts = [];
    for (const contour of contours) {
      const pts = contour.anchors;
      if (!pts?.length) continue;
      const p0 = pts[0];
      parts.push(`M ${fmt(p0.x)} ${fmt(ascender - p0.y)}`);
      const segCount = contour.closed ? pts.length : pts.length - 1;
      for (let i = 0; i < segCount; i++) {
        const a = pts[i];
        const b = pts[(i + 1) % pts.length];
        const outH = handleTipSvg(a, a.h_out, ascender, absHandles);
        const inH = handleTipSvg(b, b.h_in, ascender, absHandles);
        const endX = fmt(b.x);
        const endY = fmt(ascender - b.y);
        if (!outH.hasHandle && !inH.hasHandle) {
          parts.push(`L ${endX} ${endY}`);
        } else {
          parts.push(`C ${fmt(outH.x)} ${fmt(outH.y)} ${fmt(inH.x)} ${fmt(inH.y)} ${endX} ${endY}`);
        }
      }
      if (contour.closed) parts.push("Z");
    }
    return parts.join(" ");
  }

  const vectorMetrics = $derived.by(() => {
    const m = vectorProject?.metrics ?? {};
    const upm = m.upm || 1000;
    const asc = m.ascender ?? Math.round(upm * 0.8);
    const rawDesc = m.descender ?? -Math.round(upm * 0.2);
    const desc = rawDesc > 0 ? -rawDesc : rawDesc;
    const cap = m.cap_height ?? Math.round(asc * 0.88);
    const xh = m.x_height ?? Math.round(asc * 0.62);
    return { upm, asc, desc, cap, xh };
  });

  const activeVectorCounts = $derived.by(() => {
    const contours = activeVectorGlyph?.contours ?? [];
    const points = contours.reduce((acc, c) => acc + (c.anchors?.length ?? 0), 0);
    return { contours: contours.length, points };
  });

  // ---- Vector Mutations & Commands ----
  function shiftGlyphMetrics(glyph, { advance_width, lsb }) {
    if (!glyph) return glyph;
    const nextAdvance = advance_width !== undefined && Number.isFinite(Number(advance_width))
      ? Math.round(Number(advance_width))
      : (glyph.advance_width ?? 600);
    let nextLsb = glyph.lsb ?? 0;
    let nextContours = glyph.contours ?? [];

    if (lsb !== undefined && Number.isFinite(Number(lsb))) {
      nextLsb = Math.round(Number(lsb));
      let minX = Infinity;
      for (const c of nextContours) {
        for (const a of c.anchors ?? []) {
          if (a.x < minX) minX = a.x;
        }
      }
      const curLsb = Number.isFinite(minX) ? Math.round(minX) : (glyph.lsb ?? 0);
      const dx = nextLsb - curLsb;
      if (dx !== 0 && nextContours.length) {
        const absHandles = detectAbsoluteHandles(nextContours);
        nextContours = nextContours.map((c) => ({
          ...c,
          anchors: (c.anchors ?? []).map((a) => ({
            ...a,
            x: a.x + dx,
            h_in: absHandles ? [a.h_in[0] + dx, a.h_in[1]] : a.h_in,
            h_out: absHandles ? [a.h_out[0] + dx, a.h_out[1]] : a.h_out,
          })),
        }));
      }
    }
    return {
      ...glyph,
      advance_width: nextAdvance,
      lsb: nextLsb,
      contours: nextContours,
    };
  }

  function updateActiveVectorMetrics(field, rawValue) {
    if (!vectorProject || !activeVectorGlyph) return;
    const val = Number(rawValue);
    if (!Number.isFinite(val)) return;
    pushVectorUndo();
    const updatedGlyph = shiftGlyphMetrics(activeVectorGlyph, { [field]: val });
    vectorProject = {
      ...vectorProject,
      glyphs: { ...vectorProject.glyphs, [activeVectorKey]: updatedGlyph },
    };
  }

  async function runPathOp(op) {
    if (!vectorProject || vectorBusy) return;
    if (!inTauri) {
      notify("warn", "Desktop app required", "Vector PathOps run in the native Rust engine.");
      return;
    }
    vectorBusy = true;
    try {
      pushVectorUndo();
      if (applyOpToAll) {
        const nextGlyphs = { ...vectorProject.glyphs };
        for (const [key, g] of Object.entries(nextGlyphs)) {
          if (!g?.contours?.length) continue;
          const nextContours = await invokeCmd("foundry_transform_glyph", {
            contours: g.contours,
            op,
          });
          nextGlyphs[key] = { ...g, contours: nextContours };
        }
        vectorProject = { ...vectorProject, glyphs: nextGlyphs };
      } else if (activeVectorGlyph) {
        const nextContours = await invokeCmd("foundry_transform_glyph", {
          contours: activeVectorGlyph.contours,
          op,
        });
        vectorProject = {
          ...vectorProject,
          glyphs: {
            ...vectorProject.glyphs,
            [activeVectorKey]: { ...activeVectorGlyph, contours: nextContours },
          },
        };
      }
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function applySvgImport() {
    if (!vectorProject || !activeVectorGlyph || !svgImportText.trim() || vectorBusy) return;
    if (!inTauri) {
      notify("warn", "Desktop app required", "SVG outline import runs in the native Rust engine.");
      return;
    }
    vectorBusy = true;
    try {
      const contours = await invokeCmd("foundry_import_svg_glyph", {
        svg: svgImportText,
        upm: vectorProject.metrics?.upm || 1000,
      });
      pushVectorUndo();
      vectorProject = {
        ...vectorProject,
        glyphs: {
          ...vectorProject.glyphs,
          [activeVectorKey]: { ...activeVectorGlyph, contours },
        },
      };
      svgDrawerOpen = false;
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function onPickSvgFile(e) {
    const file = e.currentTarget?.files?.[0];
    e.currentTarget.value = "";
    if (!file) return;
    try {
      svgImportText = await file.text();
      await applySvgImport();
    } catch (err) {
      notify("err", t("err.one"), String(err));
    }
  }

  function handleCanvasGlyphUpdate(nextGlyph) {
    if (!vectorProject || !activeVectorKey) return;
    vectorProject = {
      ...vectorProject,
      glyphs: { ...vectorProject.glyphs, [activeVectorKey]: nextGlyph },
    };
  }

  async function handleMeasureCaliper(p0, p1) {
    if (!inTauri || !activeVectorGlyph?.contours?.length) {
      caliperResults = [];
      return;
    }
    try {
      caliperResults = await invokeCmd("foundry_measure_stems", {
        contours: activeVectorGlyph.contours,
        p0,
        p1,
      });
    } catch {
      caliperResults = [];
    }
  }

  async function handleKnifeSlice(p0, p1) {
    if (!inTauri || !vectorProject || !activeVectorGlyph?.contours?.length || vectorBusy) return;
    vectorBusy = true;
    try {
      const nextContours = await invokeCmd("foundry_slice_contours", {
        contours: activeVectorGlyph.contours,
        p0,
        p1,
      });
      pushVectorUndo();
      handleCanvasGlyphUpdate({ ...activeVectorGlyph, contours: nextContours });
      notify("ok", t("forge.knifeApplied"), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleTraceSingle(bytes, opts) {
    if (!inTauri || !vectorProject || !activeVectorGlyph || vectorBusy) return;
    vectorBusy = true;
    try {
      const contours = await invokeCmd("foundry_trace_image", { bytes, opts });
      pushVectorUndo();
      handleCanvasGlyphUpdate({ ...activeVectorGlyph, contours });
      notify("ok", t("forge.traceTraced"), `${contours.length} contours`);
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleTraceSheet(bytes, colsCount, rowsCount, chars, opts) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const mapped = await invokeCmd("foundry_trace_sheet", {
        bytes,
        cols: colsCount,
        rows: rowsCount,
        chars,
        opts,
      });
      const entries = Object.entries(mapped ?? {});
      if (!entries.length) {
        notify("warn", t("forge.traceEmpty"), "");
        return;
      }
      pushVectorUndo();
      const nextGlyphs = { ...vectorProject.glyphs };
      const upm = vectorProject.metrics?.upm || 1000;
      for (const [ch, contours] of entries) {
        const cp = ch.codePointAt(0) ?? 0;
        const existing = nextGlyphs[ch];
        nextGlyphs[ch] = existing
          ? { ...existing, contours }
          : {
              codepoint: cp,
              name: /^[a-z0-9]$/i.test(ch) ? ch : `uni${cp.toString(16).toUpperCase().padStart(4, "0")}`,
              advance_width: Math.round(upm * 0.62),
              lsb: 50,
              contours,
            };
      }
      vectorProject = { ...vectorProject, glyphs: nextGlyphs };
      const firstKey = entries[0][0];
      if (nextGlyphs[firstKey]) activeVectorKey = firstKey;
      notify("ok", t("forge.traceSheetDone", { n: entries.length }), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleApplyOutlineStroke(width, roundCaps) {
    if (!inTauri || !vectorProject || !activeVectorGlyph?.contours?.length || vectorBusy) return;
    vectorBusy = true;
    try {
      const contours = await invokeCmd("foundry_outline_stroke", {
        contours: activeVectorGlyph.contours,
        width,
        roundCaps,
      });
      pushVectorUndo();
      handleCanvasGlyphUpdate({ ...activeVectorGlyph, contours });
      notify("ok", t("forge.strokeExpanded"), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleApplyCalligraphy(size, angleDeg, roundness) {
    await runPathOp({
      op: "calligraphic_sweep",
      size,
      angle_deg: angleDeg,
      roundness,
    });
  }

  $effect(() => {
    caliperResults = [];
    const target = ghostEntry;
    const cp = activeVectorGlyph?.codepoint;
    const targetUpm = vectorMetrics.upm || 1000;
    if (!inTauri || !target?.file || !cp) {
      ghostPath = "";
      return;
    }
    const seq = ++ghostSeq;
    invokeCmd("foundry_inspect_glyph", { path: target.file, codepoint: cp })
      .then((info) => {
        if (seq !== ghostSeq) return;
        const scale = targetUpm / (info.upm || 1000);
        const scaled = (info.contours ?? []).map((c) => ({
          closed: c.closed,
          anchors: (c.anchors ?? []).map((a) => ({
            ...a,
            x: a.x * scale,
            y: a.y * scale,
            h_in: [(a.h_in?.[0] ?? 0) * scale, (a.h_in?.[1] ?? 0) * scale],
            h_out: [(a.h_out?.[0] ?? 0) * scale, (a.h_out?.[1] ?? 0) * scale],
          })),
        }));
        ghostPath = contoursToSvgPath(scaled, vectorMetrics.asc);
      })
      .catch(() => {
        if (seq === ghostSeq) ghostPath = "";
      });
  });

  function handleStripMetricsUpdate(key, nextMetrics) {
    const g = vectorProject?.glyphs?.[key];
    if (!vectorProject || !g) return;
    pushVectorUndo();
    const updatedGlyph = shiftGlyphMetrics(g, nextMetrics);
    vectorProject = {
      ...vectorProject,
      glyphs: { ...vectorProject.glyphs, [key]: updatedGlyph },
    };
  }

  function handleUpdateKerning(nextPairs) {
    if (!vectorProject) return;
    pushVectorUndo();
    vectorProject = { ...vectorProject, kerning: nextPairs };
  }

  function handleAiApplyGlyph(glyph) {
    if (!vectorProject || !glyph) return;
    pushVectorUndo();
    const key = String(glyph.codepoint || activeVectorKey);
    vectorProject = {
      ...vectorProject,
      glyphs: { ...vectorProject.glyphs, [key]: glyph },
    };
    activeVectorKey = key;
    const ch = String.fromCodePoint(Number(key) || 65);
    notify("ok", t("forge.aiGeneratedGlyph", { ch }), "");
  }

  function handleAiApplyProject(nextProj) {
    if (!nextProj) return;
    pushVectorUndo();
    vectorProject = nextProj;
    notify("ok", t("forge.aiGeneratedBatch"), "");
  }

  async function handleAutoSpace(targetSidebearing) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const nextProj = await invokeCmd("foundry_auto_space", {
        project: syncedVectorProject(vectorProject),
        targetSidebearing,
      });
      pushVectorUndo();
      vectorProject = nextProj;
      notify("ok", t("forge.autoSpaced"), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleAutoKern(targetGap, minAbsKern = 12) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const nextProj = await invokeCmd("foundry_auto_kern", {
        project: syncedVectorProject(vectorProject),
        targetGap,
        minAbsKern,
      });
      pushVectorUndo();
      vectorProject = nextProj;
      notify("ok", t("forge.autoKerned", { n: nextProj.kerning?.length ?? 0 }), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleGenerateComposites() {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const res = await invokeCmd("foundry_generate_composites", {
        project: syncedVectorProject(vectorProject),
      });
      pushVectorUndo();
      vectorProject = res.project;
      notify("ok", t("forge.compositesDone", { n: res.created ?? 0 }), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleApplyLinkedMetrics(nextRules) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const draft = { ...syncedVectorProject(vectorProject), linked_metrics: nextRules };
      const nextProj = await invokeCmd("foundry_apply_linked_metrics", { project: draft });
      pushVectorUndo();
      vectorProject = nextProj;
      notify("ok", t("forge.linkedApplied"), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  function handleUpdateLigatures(nextLigatures, optionalNewGlyphEntry) {
    if (!vectorProject) return;
    pushVectorUndo();
    const nextGlyphs = { ...vectorProject.glyphs };
    if (optionalNewGlyphEntry?.key && optionalNewGlyphEntry?.glyph) {
      nextGlyphs[optionalNewGlyphEntry.key] = optionalNewGlyphEntry.glyph;
    }
    vectorProject = {
      ...vectorProject,
      ligatures: nextLigatures,
      glyphs: nextGlyphs,
    };
  }

  async function handleGenerateStyle(kind, amount) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const nextProj = await invokeCmd("foundry_generate_style", {
        project: syncedVectorProject(vectorProject),
        kind,
        amount: kind === "light" ? Math.abs(Number(amount)) : Number(amount),
      });
      pushVectorUndo();
      vectorProject = nextProj;
      if (nextProj.style_name) styleName = nextProj.style_name;
      notify("ok", t("forge.styleGenerated", { style: nextProj.style_name || kind }), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleExportVariableTtf(cfg) {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const base = syncedVectorProject(vectorProject);
      const masters = [];
      if (cfg.minWght < 400) {
        const lightProj = await invokeCmd("foundry_generate_style", {
          project: base,
          kind: "light",
          amount: Math.abs(cfg.lightOffset),
        });
        masters.push({ name: "Light", wght: cfg.minWght, project: lightProj });
      }
      masters.push({ name: "Regular", wght: 400, project: base });
      if (cfg.maxWght > 400) {
        const boldProj = await invokeCmd("foundry_generate_style", {
          project: base,
          kind: "bold",
          amount: Math.abs(cfg.boldOffset),
        });
        masters.push({ name: "Bold", wght: cfg.maxWght, project: boldProj });
      }
      const savedPath = await invokeCmd("foundry_compile_variable_ttf", { base, masters });
      if (savedPath) notify("ok", t("forge.varExported"), savedPath);
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleRunAudit() {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      qaReport = await invokeCmd("foundry_audit_project", {
        project: syncedVectorProject(vectorProject),
      });
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function handleRunAutofix() {
    if (!inTauri || !vectorProject || vectorBusy) return;
    vectorBusy = true;
    try {
      const res = await invokeCmd("foundry_autofix_project", {
        project: syncedVectorProject(vectorProject),
      });
      pushVectorUndo();
      vectorProject = res.project;
      qaReport = res.report;
      notify("ok", t("forge.qaAutofixed", { n: res.fixed ?? 0 }), "");
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function convertModularToVector() {
    if (vectorBusy) return;
    if (!inTauri) {
      notify("warn", "Desktop app required", "Converting modular matrices to Vector Foundry requires the desktop app.");
      return;
    }
    vectorBusy = true;
    try {
      const proj = await invokeCmd("foundry_modular_to_project", { req: buildModularReq() });
      loadVectorProject(proj);
      const charKey = String(activeChar.codePointAt(0) ?? "");
      if (proj.glyphs?.[activeChar]) activeVectorKey = activeChar;
      else if (proj.glyphs?.[charKey]) activeVectorKey = charKey;
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  function switchStudioMode(nextMode) {
    if (nextMode === mode) return;
    if (nextMode === "vector" && !vectorProject) {
      convertModularToVector();
      return;
    }
    mode = nextMode;
  }

  async function importUfo() {
    if (!inTauri || vectorBusy) return;
    vectorBusy = true;
    try {
      const proj = await invokeCmd("foundry_import_ufo");
      if (proj) {
        loadVectorProject(proj);
        notify("ok", t("forge.ufoImported"), proj.family_name || "");
      }
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  async function exportUfo() {
    if (!inTauri || vectorBusy) return;
    vectorBusy = true;
    try {
      const proj =
        mode === "vector" && vectorProject
          ? syncedVectorProject(vectorProject)
          : await invokeCmd("foundry_modular_to_project", { req: buildModularReq() });
      const savedPath = await invokeCmd("foundry_export_ufo", { project: proj });
      if (savedPath) notify("ok", t("forge.ufoExported"), savedPath);
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      vectorBusy = false;
    }
  }

  // ---- Modular Matrix Editing Helpers ----
  function cloneGlyphMap(map) {
    const out = {};
    for (const [k, v] of Object.entries(map)) out[k] = cloneGrid(v);
    return out;
  }

  function makeSnapshot() {
    return {
      glyphs: cloneGlyphMap(glyphs),
      glyphWidths: { ...glyphWidths },
      gridScale,
    };
  }

  function pushUndo() {
    undoStack = [...undoStack.slice(-59), makeSnapshot()];
    redoStack = [];
  }

  function applySnapshot(snap) {
    glyphs = cloneGlyphMap(snap.glyphs);
    glyphWidths = { ...(snap.glyphWidths ?? {}) };
    gridScale = snap.gridScale ?? 1;
  }

  function setGridScale(nextScale) {
    if (nextScale === gridScale) return;
    pushUndo();
    const targetCols = MATRIX_COLS * nextScale;
    const targetRows = MATRIX_ROWS * nextScale;
    glyphs = resizeGlyphsResolution(glyphs, targetCols, targetRows);
    gridScale = nextScale;
  }

  function applyPreset(p) {
    pushUndo();
    activePreset = p.id;
    shape = p.shape;
    weight = p.weight;
    contrast = p.contrast;
    slant = p.slant;
    tracking = p.tracking;
    corner = p.corner;
    const base = buildPresetGlyphs(p.id);
    glyphs = gridScale === 2 ? resizeGlyphsResolution(base, MATRIX_COLS * 2, MATRIX_ROWS * 2) : base;
  }

  function setCell(r, c, val) {
    const grid = cloneGrid(currentGrid);
    if (grid[r][c] === val) return;
    grid[r][c] = val;
    glyphs = { ...glyphs, [activeChar]: grid };
  }

  function startPaint(r, c) {
    pushUndo();
    strokeSnapshotTaken = true;
    const next = currentGrid[r][c] ? 0 : 1;
    painting = next;
    setCell(r, c, next);
  }

  function enterPaint(r, c) {
    if (painting === null) return;
    setCell(r, c, painting);
  }

  function clearGlyph() {
    pushUndo();
    const empty = Array.from({ length: rows }, () => new Array(cols).fill(0));
    glyphs = { ...glyphs, [activeChar]: empty };
  }

  function invertGlyph() {
    pushUndo();
    const inv = currentGrid.map((row) => row.map((v) => (v ? 0 : 1)));
    glyphs = { ...glyphs, [activeChar]: inv };
  }

  function mirrorGlyph() {
    pushUndo();
    const mir = currentGrid.map((row) => [...row].reverse());
    glyphs = { ...glyphs, [activeChar]: mir };
  }

  function resetCurrentGlyph() {
    const fresh = buildPresetGlyphs(activePreset);
    if (fresh[activeChar]) {
      pushUndo();
      const next =
        gridScale === 2
          ? resizeGlyphsResolution({ [activeChar]: fresh[activeChar] }, cols, rows)[activeChar]
          : fresh[activeChar];
      glyphs = { ...glyphs, [activeChar]: next };
    }
  }

  function setActiveGlyphWidth(val) {
    const num = Number(val) || 0;
    if (!strokeSnapshotTaken) {
      pushUndo();
      strokeSnapshotTaken = true;
    }
    if (num === 0) {
      const next = { ...glyphWidths };
      delete next[activeChar];
      glyphWidths = next;
    } else {
      glyphWidths = { ...glyphWidths, [activeChar]: num };
    }
  }

  // ---- Project Save / Load & Exports ----
  function fontSlug() {
    return `${familyName.trim() || "cosmic-forge"}-${styleName}`.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  }

  function downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function exportTtf() {
    if (!compiledBytes.length) return;
    const file = `${fontSlug()}.ttf`;
    downloadBlob(new Blob([compiledBytes], { type: "font/ttf" }), file);
    notify("ok", t("forge.exported"), file);
  }

  async function exportWoff() {
    if (!compiledBytes.length) return;
    try {
      let woff;
      if (inTauri) {
        woff = new Uint8Array(await invokeCmd("foundry_compile_woff", { ttfBytes: Array.from(compiledBytes) }));
      } else if (import.meta.env.DEV) {
        const { compileWoffFont } = await import("../lib/dev-forge-compiler.js");
        woff = await compileWoffFont(compiledBytes);
      } else {
        return;
      }
      const file = `${fontSlug()}.woff`;
      downloadBlob(new Blob([woff], { type: "font/woff" }), file);
      notify("ok", t("forge.woffExported"), file);
    } catch (e) {
      notify("err", t("err.one"), String(e));
    }
  }

  function saveProject() {
    const project =
      mode === "vector" && vectorProject
        ? syncedVectorProject(vectorProject)
        : {
            version: 1,
            familyName,
            styleName,
            activePreset,
            shape,
            weight,
            contrast,
            slant,
            tracking,
            corner,
            gridScale,
            glyphs,
            glyphWidths,
          };
    const file = `${fontSlug()}.cosmicforge.json`;
    downloadBlob(new Blob([JSON.stringify(project, null, 2)], { type: "application/json" }), file);
    notify("ok", t("forge.projSaved"), file);
  }

  async function onLoadProjectFile(e) {
    const file = e.currentTarget?.files?.[0];
    e.currentTarget.value = "";
    if (!file) return;
    try {
      const data = JSON.parse(await file.text());
      if (!data || typeof data.glyphs !== "object") throw new Error("Invalid project");
      if (data.metrics && typeof data.metrics === "object") {
        loadVectorProject(data);
        notify("ok", t("forge.projLoaded"), file.name);
        return;
      }
      pushUndo();
      mode = "modular";
      if (typeof data.familyName === "string") familyName = data.familyName;
      if (typeof data.styleName === "string") styleName = data.styleName;
      if (typeof data.activePreset === "string") activePreset = data.activePreset;
      if (typeof data.shape === "string") shape = data.shape;
      if (typeof data.weight === "number") weight = data.weight;
      if (typeof data.contrast === "number") contrast = data.contrast;
      if (typeof data.slant === "number") slant = data.slant;
      if (typeof data.tracking === "number") tracking = data.tracking;
      if (typeof data.corner === "number") corner = data.corner;
      gridScale = data.gridScale === 2 ? 2 : 1;
      glyphs = cloneGlyphMap(data.glyphs);
      glyphWidths = data.glyphWidths && typeof data.glyphWidths === "object" ? { ...data.glyphWidths } : {};
      notify("ok", t("forge.projLoaded"), file.name);
    } catch {
      notify("err", t("forge.projInvalid"), file.name);
    }
  }

  function onForgeKeyDown(e) {
    if (e.target.closest("input, textarea, [contenteditable]")) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redoForge();
      else undoForge();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redoForge();
    }
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

  const PATH_OPS = [
    { id: "remove_overlap", labelKey: "forge.opRemoveOverlap", op: { op: "remove_overlap" } },
    { id: "round_corners", labelKey: "forge.opRoundCorners", op: { op: "round_corners", radius: 22, chamfer: false } },
    { id: "embolden", labelKey: "forge.opEmbolden", op: { op: "offset", delta: 10, round_joins: true } },
    { id: "thin", labelKey: "forge.opThin", op: { op: "offset", delta: -10, round_joins: true } },
    { id: "add_extrema", labelKey: "forge.opAddExtrema", op: { op: "add_extrema" } },
    { id: "simplify", labelKey: "forge.opSimplify", op: { op: "simplify", tolerance: 2.0 } },
    { id: "normalize_winding", labelKey: "forge.opFixWinding", op: { op: "normalize_winding" } },
  ];
</script>

<svelte:window onmouseup={() => ((painting = null), (strokeSnapshotTaken = false))} onkeydown={onForgeKeyDown} />

<input
  bind:this={projectInput}
  type="file"
  accept=".cosmicforge.json,.json"
  style="display:none"
  onchange={onLoadProjectFile}
/>

<input
  bind:this={svgFileInput}
  type="file"
  accept=".svg,image/svg+xml"
  style="display:none"
  onchange={onPickSvgFile}
/>

<div class="forge">
  <div class="forge-bar">
    <div class="mode-switch" role="tablist" aria-label="Foundry mode">
      <button type="button" role="tab" aria-selected={mode === "modular"} class="mode-btn" class:active={mode === "modular"} title={t("forge.modeModular")} onclick={() => switchStudioMode("modular")}>
        <Layers size={12} />Modular
      </button>
      <button type="button" role="tab" aria-selected={mode === "vector"} class="mode-btn" class:active={mode === "vector"} title={t("forge.modeVector")} onclick={() => switchStudioMode("vector")}>
        <Spline size={12} />Vector
      </button>
    </div>

    <label class="name-box" title={t("forge.family")}>
      <input bind:value={familyName} maxlength="40" spellcheck="false" placeholder="Family name" />
    </label>

    <select class="style-select" bind:value={styleName} aria-label="Font style">
      <option value="Regular">Regular</option>
      <option value="Bold">Bold</option>
      <option value="Oblique">Oblique</option>
    </select>

    <div class="bar-actions">
      <span class="size-badge num" title={`${Math.max(1, Math.round(compiledBytes.length / 1024))} KB · ${compileStats.glyphCount || Object.keys(glyphs).length + 2} glyphs`}>
        {Math.max(1, Math.round(compiledBytes.length / 1024))}K · {compileStats.glyphCount || Object.keys(glyphs).length + 2}g
      </span>
      <div class="file-menu-wrap">
        <button
          type="button"
          class="btn"
          aria-expanded={fileMenuOpen}
          onclick={() => (fileMenuOpen = !fileMenuOpen)}
        >
          <MoreHorizontal size={13} />File
          <ChevronDown size={11} />
        </button>
        {#if fileMenuOpen}
          <button
            type="button"
            class="file-menu-backdrop"
            aria-label="Close file menu"
            onclick={() => (fileMenuOpen = false)}
          ></button>
          <div class="file-menu-pop" role="menu">
            <button type="button" role="menuitem" onclick={() => { fileMenuOpen = false; projectInput?.click(); }}>
              <FolderUp size={12} />{t("forge.loadProj")}
            </button>
            <button type="button" role="menuitem" onclick={() => { fileMenuOpen = false; saveProject(); }}>
              <Save size={12} />{t("forge.saveProj")}
            </button>
            {#if inTauri}
              <div class="menu-sep"></div>
              <button type="button" role="menuitem" disabled={vectorBusy} onclick={() => { fileMenuOpen = false; importUfo(); }}>
                <FolderUp size={12} />{t("forge.importUfo")}
              </button>
              <button type="button" role="menuitem" disabled={vectorBusy} onclick={() => { fileMenuOpen = false; exportUfo(); }}>
                <Save size={12} />{t("forge.exportUfo")}
              </button>
            {/if}
            <div class="menu-sep"></div>
            <button type="button" role="menuitem" disabled={!compiledBytes.length} onclick={() => { fileMenuOpen = false; exportWoff(); }}>
              <Download size={12} />{t("forge.exportWoff")}
            </button>
          </div>
        {/if}
      </div>
      <button type="button" class="btn" title={t("forge.export")} onclick={exportTtf} disabled={!compiledBytes.length}>
        <Download size={13} strokeWidth={2} />.TTF
      </button>
      <button type="button" class="btn primary" title={t("forge.install")} onclick={installForged} disabled={installing || !compiledBytes.length}>
        <Check size={13} strokeWidth={2.2} />{installing ? t("forge.installing") : "Install"}
      </button>
    </div>
  </div>

  {#if mode === "modular"}
    <div class="workspace">
      <!-- Left Column: Character Picker + 7x9 Blueprint Matrix -->
      <section class="panel editor-col">
        <div class="panel-head">
          <span class="label">{t("forge.matrix")} · “{activeChar}”</span>
          <div class="tool-btns">
            <button type="button" class="mini" title={t("forge.undo")} onclick={undoForge} disabled={!canUndo}><Undo2 size={12} /></button>
            <button type="button" class="mini" title={t("forge.redo")} onclick={redoForge} disabled={!canRedo}><Redo2 size={12} /></button>
            <button
              type="button"
              class="res-pill"
              class:active={gridScale === 1}
              onclick={() => setGridScale(1)}
            >7×9</button>
            <button
              type="button"
              class="res-pill"
              class:active={gridScale === 2}
              onclick={() => setGridScale(2)}
            >14×18</button>
            <button type="button" class="mini" title={t("forge.mirror")} onclick={mirrorGlyph}><FlipHorizontal2 size={12} /></button>
            <button type="button" class="mini" title={t("forge.invert")} onclick={invertGlyph}><Contrast size={12} /></button>
            <button type="button" class="mini" title={t("forge.clear")} onclick={clearGlyph}><Eraser size={12} /></button>
            <button type="button" class="mini" title={t("forge.resetGlyph")} onclick={resetCurrentGlyph}><RotateCcw size={12} /></button>
          </div>
        </div>

        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="matrix-stage" onmouseleave={() => (painting = null)}>
          <div
            class="matrix"
            role="grid"
            aria-label="Glyph blueprint matrix"
            style:--cols={cols}
            style:--rows={rows}
            style:--cell={gridScale === 2 ? "13px" : "26px"}
            style:--gap={gridScale === 2 ? "2px" : "3px"}
          >
            {#each currentGrid as row, r (r)}
              {#each row as cell, c (c)}
                <button
                  type="button"
                  class="cell"
                  class:on={cell === 1}
                  class:baseline={r === rows - 1}
                  class:xheight={r === 2 * gridScale}
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
            <span>{cols}×{rows} EM GRID</span>
            <span>1000 UPM · TTF</span>
          </div>
          <label class="glyph-width-row">
            <span class="s-lbl">{t("forge.glyphWidth")} (“{activeChar}”)</span>
            <input
              type="range"
              min="-160"
              max="240"
              step="10"
              value={currentGlyphWidth}
              oninput={(e) => setActiveGlyphWidth(e.currentTarget.value)}
              onchange={() => (strokeSnapshotTaken = false)}
            />
            <span class="s-val num">{currentGlyphWidth > 0 ? `+${currentGlyphWidth}` : currentGlyphWidth}</span>
            {#if currentGlyphWidth !== 0}
              <button type="button" class="mini" title={t("forge.resetWidth")} onclick={() => setActiveGlyphWidth(0)}><RotateCcw size={10} /></button>
            {/if}
          </label>
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

      <!-- Right Column: DNA Presets, Module Shapes, Rust Engine Strip & Live Compiled Specimen -->
      <section class="controls-col">
        <div class="panel">
          <div class="panel-head">
            <span class="label"><Sparkles size={11} /> {t("forge.presets")}</span>
            {#if inTauri}
              <button type="button" class="btn-inline accent" onclick={convertModularToVector} disabled={vectorBusy}>
                <Spline size={12} />{t("forge.convertToVector")}
              </button>
            {/if}
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

          <!-- Rust Vector Engine Controls Strip -->
          <div class="engine-strip">
            <label class="toggle-chip" class:on={removeOverlap}>
              <input type="checkbox" bind:checked={removeOverlap} />
              <span>{t("forge.mergeOverlaps")}</span>
            </label>
            <label class="toggle-chip" class:on={smoothCorners}>
              <input type="checkbox" bind:checked={smoothCorners} />
              <span>{t("forge.trueCurves")}</span>
            </label>
            <label class="offset-slider">
              <span class="s-lbl">{t("forge.opticalOffset")}</span>
              <input type="range" min="-24" max="24" step="1" bind:value={opticalOffset} />
              <span class="s-val num">{opticalOffset > 0 ? `+${opticalOffset}` : opticalOffset}</span>
            </label>
          </div>
        </div>

        <div class="panel specimen-panel">
          <div class="panel-head">
            <span class="label">{t("forge.livePreview")}</span>
            <div class="test-controls">
              <div class="fmt-mini">
                <button type="button" class="f-pill b" class:on={forgeFmt.bold} title={t("fmt.bold")} onclick={() => (forgeFmt.bold = !forgeFmt.bold)}>B</button>
                <button type="button" class="f-pill i" class:on={forgeFmt.italic} title={t("fmt.italic")} onclick={() => (forgeFmt.italic = !forgeFmt.italic)}>I</button>
                <button type="button" class="f-pill u" class:on={forgeFmt.underline} title={t("fmt.underline")} onclick={() => (forgeFmt.underline = !forgeFmt.underline)}>U</button>
                <button type="button" class="f-pill s" class:on={forgeFmt.strike} title={t("fmt.strike")} onclick={() => (forgeFmt.strike = !forgeFmt.strike)}>S</button>
                <button type="button" class="f-pill" class:on={forgeFmt.uppercase} title={t("fmt.upper")} onclick={() => (forgeFmt.uppercase = !forgeFmt.uppercase)}>AA</button>
              </div>
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
            style:font-weight={forgeFmt.bold ? 700 : 400}
            style:font-style={forgeFmt.italic ? "italic" : "normal"}
            style:text-decoration={forgeDec}
            style:text-transform={forgeFmt.uppercase ? "uppercase" : "none"}
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
  {:else}
    <!-- Vector Foundry Full-Width Tab Bar -->
    <div class="vector-subtabs" role="tablist" aria-label="Vector Foundry Workspace">
      {#if vectorTab === "draw" || vectorTab === "space" || vectorTab === "layout"}
        <button
          type="button"
          class="mini rail-toggle"
          class:on={glyphRailOpen}
          title={glyphRailOpen ? "Hide Glyph Rail" : "Show Glyph Rail"}
          onclick={() => (glyphRailOpen = !glyphRailOpen)}
        >
          <PanelLeft size={13} />
        </button>
      {/if}
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "draw"}
        class="v-tab-btn"
        class:on={vectorTab === "draw"}
        onclick={() => (vectorTab = "draw")}
      >
        <PenTool size={12} />{t("forge.tabDraw")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "ai"}
        class="v-tab-btn ai-tab"
        class:on={vectorTab === "ai"}
        onclick={() => (vectorTab = "ai")}
      >
        <Sparkles size={12} />{t("forge.aiGenerate")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "space"}
        class="v-tab-btn"
        class:on={vectorTab === "space"}
        onclick={() => (vectorTab = "space")}
      >
        <ArrowLeftRight size={12} />{t("forge.tabSpace")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "layout"}
        class="v-tab-btn"
        class:on={vectorTab === "layout"}
        onclick={() => (vectorTab = "layout")}
      >
        <Combine size={12} />{t("forge.tabLayout")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "styles"}
        class="v-tab-btn"
        class:on={vectorTab === "styles"}
        onclick={() => (vectorTab = "styles")}
      >
        <WandSparkles size={12} />{t("forge.tabStyles")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={vectorTab === "qa"}
        class="v-tab-btn"
        class:on={vectorTab === "qa"}
        onclick={() => (vectorTab = "qa")}
      >
        <ShieldCheck size={12} />{t("forge.tabQa")}
      </button>
    </div>

    <!-- Vector Foundry Mode -->
    <div class="workspace vector-ws" class:rail-hidden={!showGlyphRail}>
      {#if showGlyphRail}
        <!-- Slim Left Glyph Rail -->
        <section class="panel editor-col glyph-rail">
          <div class="panel-head">
            <span class="label">Glyphs · {vectorGlyphEntries.length}</span>
            <div class="tool-btns">
              <button type="button" class="mini" title={t("forge.undo")} onclick={undoForge} disabled={!canUndo}><Undo2 size={12} /></button>
              <button type="button" class="mini" title={t("forge.redo")} onclick={redoForge} disabled={!canRedo}><Redo2 size={12} /></button>
            </div>
          </div>

          <label class="v-search">
            <Search size={12} />
            <input bind:value={vectorGlyphQuery} placeholder={t("spec.glyphSearch")} spellcheck="false" />
          </label>

          <div class="v-glyph-grid">
            {#each vectorGlyphEntries as item (item.key)}
              <button
                type="button"
                class="v-glyph-btn"
                class:active={activeVectorKey === item.key}
                class:empty-outline={!item.glyph.contours?.length}
                title={`U+${item.hex} · ${item.glyph.name || item.char}`}
                onclick={() => (activeVectorKey = item.key)}
              >
                <span>{item.char}</span>
                <span class="v-hex num">{item.hex}</span>
              </button>
            {/each}
          </div>
        </section>
      {/if}

      <!-- Right Column: Active Tool Surface & Compact Specimen Dock -->
      <section class="controls-col">
        {#if forking}
          <div class="panel forking-banner">
            <Sparkles size={15} />
            <span>{t("forge.forking")}</span>
          </div>
        {/if}

        {#if vectorTab === "draw"}
        <div class="panel vector-stage-panel">
          <div class="panel-head compact-head">
            <span class="label">
              “{activeVectorChar}”
              {#if activeVectorGlyph}
                <span class="v-meta num">
                  U+{(activeVectorGlyph.codepoint ?? 0).toString(16).toUpperCase().padStart(4, "0")} · {activeVectorCounts.contours}c/{activeVectorCounts.points}p
                </span>
              {/if}
            </span>
            <div class="tool-btns">
              {#if activeVectorGlyph}
                <label class="metric-input" title={t("forge.advanceWidth")}>
                  <span>W</span>
                  <input
                    type="number"
                    step="10"
                    value={activeVectorGlyph.advance_width}
                    onchange={(e) => updateActiveVectorMetrics("advance_width", e.currentTarget.value)}
                  />
                </label>
                <label class="metric-input" title={t("forge.lsb")}>
                  <span>LSB</span>
                  <input
                    type="number"
                    step="10"
                    value={activeVectorGlyph.lsb}
                    onchange={(e) => updateActiveVectorMetrics("lsb", e.currentTarget.value)}
                  />
                </label>
              {/if}
              {#if inTauri}
                <GhostPicker
                  fonts={fonts.list}
                  selectedFamily={ghostEntry?.name ?? ""}
                  onSelectGhost={(entry) => (ghostEntry = entry)}
                />
                <button
                  type="button"
                  class="btn-inline"
                  class:accent={traceDrawerOpen}
                  onclick={() => {
                    traceDrawerOpen = !traceDrawerOpen;
                    if (traceDrawerOpen) svgDrawerOpen = false;
                  }}
                >
                  <Scan size={12} />{t("forge.traceImage")}
                </button>
              {/if}
              <button
                type="button"
                class="btn-inline"
                class:accent={svgDrawerOpen}
                onclick={() => {
                  svgDrawerOpen = !svgDrawerOpen;
                  if (svgDrawerOpen) traceDrawerOpen = false;
                }}
              >
                <FileCode size={12} />{t("forge.importSvg")}
              </button>
            </div>
          </div>

          <!-- Single-Row Scrollable PathOps Toolbar -->
          <div class="pathops-bar">
            <div class="pathops-btns">
              {#each PATH_OPS as p (p.id)}
                <button
                  type="button"
                  class="op-btn"
                  disabled={vectorBusy || !activeVectorGlyph}
                  onclick={() => runPathOp(p.op)}
                >
                  {t(p.labelKey)}
                </button>
              {/each}
            </div>
            <label class="toggle-chip compact-chip" class:on={applyOpToAll}>
              <input type="checkbox" bind:checked={applyOpToAll} />
              <span>{t("forge.applyAll")}</span>
            </label>
          </div>

          <!-- Image Trace & Calligraphic Skeleton Drawer -->
          {#if traceDrawerOpen}
            <ImageTraceDrawer
              upm={vectorMetrics.upm}
              busy={vectorBusy}
              onTraceSingle={handleTraceSingle}
              onTraceSheet={handleTraceSheet}
              onApplyOutlineStroke={handleApplyOutlineStroke}
              onApplyCalligraphy={handleApplyCalligraphy}
              onclose={() => (traceDrawerOpen = false)}
            />
          {/if}

          <!-- SVG Import Drawer -->
          {#if svgDrawerOpen}
            <div class="svg-drawer">
              <textarea
                rows="3"
                bind:value={svgImportText}
                placeholder={t("forge.svgPlaceholder")}
                spellcheck="false"
              ></textarea>
              <div class="svg-drawer-actions">
                <button type="button" class="btn-inline" onclick={() => svgFileInput?.click()}>
                  <Upload size={12} />.svg
                </button>
                <button
                  type="button"
                  class="btn-inline accent"
                  disabled={vectorBusy || !svgImportText.trim() || !activeVectorGlyph}
                  onclick={applySvgImport}
                >
                  <Check size={12} />{t("forge.svgApply")}
                </button>
              </div>
            </div>
          {/if}

          <!-- Interactive Vector Bézier Glyph Editor -->
          <VectorGlyphEditor
            glyph={activeVectorGlyph}
            metrics={vectorMetrics}
            {ghostPath}
            ghostLabel={ghostEntry?.name ?? ""}
            showNodes={showVectorNodes}
            {caliperResults}
            onUpdateGlyph={handleCanvasGlyphUpdate}
            onPushUndo={pushVectorUndo}
            onToggleShowNodes={() => (showVectorNodes = !showVectorNodes)}
            onMeasureCaliper={handleMeasureCaliper}
            onKnifeSlice={handleKnifeSlice}
          />
        </div>
        {:else if vectorTab === "ai"}
          <AiGlyphGeneratorDrawer
            project={vectorProject}
            activeKey={activeVectorKey}
            activeChar={activeVectorChar}
            upm={vectorMetrics.upm}
            busy={vectorBusy}
            onApplyGlyph={handleAiApplyGlyph}
            onApplyProject={handleAiApplyProject}
          />
        {:else if vectorTab === "space"}
          <SpacingKerningStrip
            project={vectorProject}
            {liveFamilyId}
            activeKey={activeVectorKey}
            busy={vectorBusy}
            onSelectGlyph={(key) => (activeVectorKey = key)}
            onUpdateMetrics={handleStripMetricsUpdate}
            onUpdateKerning={handleUpdateKerning}
            onAutoSpace={handleAutoSpace}
            onAutoKern={handleAutoKern}
          />
        {:else if vectorTab === "layout"}
          <LayoutCompositesPanel
            project={vectorProject}
            busy={vectorBusy}
            onGenerateComposites={handleGenerateComposites}
            onApplyLinkedMetrics={handleApplyLinkedMetrics}
            onUpdateLigatures={handleUpdateLigatures}
            onSelectGlyph={(key) => {
              activeVectorKey = key;
              vectorTab = "draw";
            }}
          />
        {:else if vectorTab === "styles"}
          <StylesVariablePanel
            project={vectorProject}
            busy={vectorBusy}
            onGenerateStyle={handleGenerateStyle}
            onExportVariableTtf={handleExportVariableTtf}
          />
        {:else}
          <FoundryQaPanel
            project={vectorProject}
            report={qaReport}
            busy={vectorBusy}
            onRunAudit={handleRunAudit}
            onRunAutofix={handleRunAutofix}
            onSelectGlyph={(key) => {
              activeVectorKey = key;
              vectorTab = "draw";
            }}
          />
        {/if}

        <!-- Collapsible Live Compiled Specimen Dock (Vector Mode) -->
        <div class="panel specimen-dock" class:expanded={specimenExpanded}>
          <div class="panel-head">
            <button
              type="button"
              class="specimen-toggle"
              onclick={() => (specimenExpanded = !specimenExpanded)}
            >
              {#if specimenExpanded}<ChevronDown size={13} />{:else}<ChevronUp size={13} />{/if}
              <span class="label">{t("forge.livePreview")}</span>
            </button>
            <div class="test-controls">
              <input class="test-input" bind:value={testPhrase} placeholder={t("forge.typeTest")} spellcheck="false" />
              <input class="test-size" type="range" min="18" max="72" bind:value={testSize} aria-label="Preview size" />
              <span class="num size-val">{testSize}px</span>
            </div>
          </div>
          {#if compileError && inTauri}
            <div class="err-box">{compileError}</div>
          {/if}
          {#if specimenExpanded}
            <div
              class="live-stage"
              style:font-family={`"${liveFamilyId}", var(--sans)`}
              style:font-size="{testSize}px"
            >
              {effectivePhrase}
            </div>
          {/if}
        </div>
      </section>
    </div>
  {/if}
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
    gap: 8px;
    flex-wrap: nowrap;
    padding: 8px 12px;
    border-radius: 12px;
    background: var(--side);
    border: 1px solid var(--line);
  }
  .mode-switch {
    display: inline-flex;
    padding: 2px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
    gap: 2px;
    flex: none;
  }
  .mode-btn {
    height: 26px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: 600 11.5px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    transition: all 0.14s;
  }
  .mode-btn:hover { color: var(--ink); }
  .mode-btn.active { background: var(--accent); color: #fff; box-shadow: var(--glow); }

  .name-box {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 9px;
    height: 28px;
    border-radius: 8px;
    background: var(--field);
    border: 1px solid var(--line2);
    flex: 0 1 180px;
    min-width: 110px;
  }
  .name-box .lbl { font: 700 9.5px var(--mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent-2); flex: none; }
  .name-box input { border: 0; outline: 0; background: transparent; font: 600 12.5px var(--sans); color: var(--ink); width: 100%; min-width: 60px; }

  .style-select {
    height: 28px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 600 11.5px var(--sans);
    outline: none;
    flex: none;
  }

  .bar-actions { margin-left: auto; display: flex; align-items: center; gap: 6px; flex: none; }
  .file-menu-wrap { position: relative; }
  .file-menu-backdrop { position: fixed; inset: 0; z-index: 40; border: 0; background: transparent; padding: 0; }
  .file-menu-pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 50;
    min-width: 170px;
    padding: 5px;
    border-radius: 10px;
    background: var(--side);
    border: 1px solid var(--line2);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .file-menu-pop button {
    height: 28px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink);
    font: 500 11.5px var(--sans);
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
  }
  .file-menu-pop button:hover:not(:disabled) { background: var(--hov); }
  .file-menu-pop button:disabled { opacity: 0.45; cursor: default; }
  .menu-sep { height: 1px; background: var(--line); margin: 3px 4px; }
  .size-badge { font: 500 11px var(--mono); color: var(--ink3); padding-right: 4px; }
  .btn {
    height: 30px; padding: 0 11px; border: 1px solid var(--line2); border-radius: 8px;
    background: var(--field); color: var(--ink); font: 600 11.5px var(--sans);
    display: inline-flex; align-items: center; gap: 5px; transition: all 0.15s;
  }
  .btn:hover:not(:disabled) { background: var(--chip); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; box-shadow: var(--glow); }
  .btn.primary:hover:not(:disabled) { filter: brightness(1.08); }
  .btn:disabled { opacity: 0.55; cursor: default; }

  .btn-inline {
    height: 24px; padding: 0 9px; border: 1px solid var(--line2); border-radius: 7px;
    background: var(--field); color: var(--ink2); font: 600 11px var(--sans);
    display: inline-flex; align-items: center; gap: 5px;
  }
  .btn-inline:hover:not(:disabled) { color: var(--ink); border-color: var(--accent-2); }
  .btn-inline.accent { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); }
  .btn-inline:disabled { opacity: 0.5; cursor: default; }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 316px minmax(0, 1fr);
    gap: 12px;
  }
  .workspace.vector-ws {
    grid-template-columns: 196px minmax(0, 1fr);
  }
  .workspace.vector-ws.rail-hidden {
    grid-template-columns: minmax(0, 1fr);
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

  .panel-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .panel-head .label { display: inline-flex; align-items: center; gap: 5px; }
  .tool-btns { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; }
  .res-pill {
    height: 24px; padding: 0 7px; border: 1px solid var(--line); border-radius: 6px;
    background: var(--field); color: var(--ink3); font: 600 10px var(--mono);
  }
  .res-pill.active { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }
  .mini {
    width: 24px; height: 24px; border: 1px solid var(--line); border-radius: 6px;
    background: var(--field); color: var(--ink2); display: grid; place-items: center;
  }
  .mini:hover:not(:disabled) { color: var(--ink); border-color: var(--line2); }
  .mini:disabled { opacity: 0.4; cursor: default; }
  .glyph-width-row {
    width: 100%; display: flex; align-items: center; gap: 8px; padding-top: 4px; border-top: 1px solid var(--line);
  }
  .glyph-width-row .s-lbl { flex: none; font-size: 11px; color: var(--ink2); }
  .glyph-width-row input[type="range"] { flex: 1; min-width: 0; accent-color: var(--accent); }
  .glyph-width-row .s-val { width: 38px; text-align: right; font: 500 11px var(--mono); color: var(--ink3); }

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
    grid-template-columns: repeat(var(--cols, 7), var(--cell, 26px));
    grid-template-rows: repeat(var(--rows, 9), var(--cell, 26px));
    gap: var(--gap, 3px);
  }
  .cell {
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--field);
    padding: 0;
    transition: background-color 0.08s, border-color 0.08s, transform 0.08s;
  }
  .cell.xheight { border-top-color: rgba(251, 191, 36, 0.42); }
  .cell.baseline { border-bottom-color: rgba(242, 107, 33, 0.55); }
  .cell:hover { border-color: var(--accent-2); }
  .cell.on {
    background: var(--accent);
    border-color: var(--accent-2);
    box-shadow: 0 0 10px rgba(242, 107, 33, 0.45);
  }
  .guide-legend { width: 100%; display: flex; justify-content: space-between; font: 500 10px var(--mono); color: var(--ink3); }

  .char-groups { display: flex; flex-direction: column; gap: 8px; }
  .char-group { display: flex; flex-direction: column; gap: 4px; }
  .grp-lbl { font: 600 9.5px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink3); }
  .char-grid { display: grid; grid-template-columns: repeat(10, minmax(0, 1fr)); gap: 3px; }
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
  .slider-row input[type="range"],
  .offset-slider input[type="range"] { -webkit-appearance: none; appearance: none; height: 3px; border-radius: 3px; background: var(--line2); outline: none; }
  .slider-row input[type="range"]::-webkit-slider-thumb,
  .offset-slider input[type="range"]::-webkit-slider-thumb { -webkit-appearance: none; width: 12px; height: 12px; border-radius: 50%; background: var(--accent); cursor: pointer; }

  .engine-strip {
    display: flex; flex-wrap: wrap; align-items: center; gap: 10px;
    padding-top: 8px; margin-top: 2px; border-top: 1px solid var(--line);
  }
  .toggle-chip {
    display: inline-flex; align-items: center; gap: 6px;
    height: 26px; padding: 0 10px; border-radius: 7px; border: 1px solid var(--line);
    background: var(--field); color: var(--ink2); font: 600 11px var(--sans); cursor: pointer;
  }
  .toggle-chip input { accent-color: var(--accent); margin: 0; }
  .toggle-chip.on { border-color: var(--accent); background: var(--accent-soft); color: var(--ink); }
  .offset-slider {
    display: flex; align-items: center; gap: 8px; flex: 1; min-width: 190px;
  }
  .offset-slider .s-lbl { flex: none; }
  .offset-slider input[type="range"] { flex: 1; min-width: 60px; }
  .offset-slider .s-val { width: 32px; }

  /* Vector Foundry Mode Styles */
  .v-search {
    height: 28px; display: flex; align-items: center; gap: 6px; padding: 0 9px;
    border-radius: 8px; background: var(--field); border: 1px solid var(--line2); color: var(--ink3);
  }
  .v-search input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--ink); font: 500 12px var(--sans); }
  .v-glyph-grid {
    display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 4px;
    overflow-y: auto; padding-right: 2px;
  }
  .v-glyph-btn {
    height: 42px; border: 1px solid var(--line); border-radius: 7px;
    background: var(--field); color: var(--ink);
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px;
    font: 600 14px var(--sans); padding: 2px;
  }
  .v-glyph-btn.empty-outline { color: var(--ink3); }
  .v-glyph-btn:hover { border-color: var(--accent-2); }
  .v-glyph-btn.active { background: var(--accent-soft); border-color: var(--accent); }
  .v-hex { font-size: 9px; color: var(--ink3); }

  .forking-banner {
    flex-direction: row; align-items: center; gap: 8px;
    background: var(--accent-soft); border-color: var(--accent); color: var(--ink); font-size: 12.5px; font-weight: 600;
  }
  .vector-subtabs {
    flex: none;
    display: flex; align-items: center; gap: 4px; flex-wrap: nowrap;
    overflow-x: auto; scrollbar-width: none;
    padding: 4px 6px; border-radius: 10px; background: var(--side); border: 1px solid var(--line);
  }
  .rail-toggle.on { color: var(--accent-2); border-color: var(--accent); background: var(--accent-soft); }
  .v-tab-btn {
    height: 27px; padding: 0 10px; border: 1px solid transparent; border-radius: 7px;
    display: inline-flex; align-items: center; gap: 5px; white-space: nowrap; flex: none;
    background: transparent; color: var(--ink2); font: 600 11.5px var(--sans); cursor: pointer;
  }
  .v-tab-btn:hover { background: var(--field); color: var(--ink); }
  .v-tab-btn.on { background: var(--accent-soft); border-color: var(--accent); color: var(--ink); }
  .v-tab-btn.ai-tab { color: var(--accent-2); }
  .v-tab-btn.ai-tab.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .vector-stage-panel { flex: 1 0 auto; min-height: 320px; }
  .compact-head { gap: 6px; }
  .v-meta { margin-left: 6px; font-size: 11px; color: var(--accent-2); }
  .metric-input {
    display: inline-flex; align-items: center; gap: 4px;
    height: 24px; padding: 0 6px; border-radius: 6px; background: var(--field); border: 1px solid var(--line);
    font: 600 10px var(--mono); color: var(--ink3);
  }
  .metric-input input {
    width: 44px; border: 0; outline: 0; background: transparent; color: var(--ink);
    font: 600 11px var(--mono); text-align: right;
  }
  .pathops-bar {
    display: flex; flex-wrap: nowrap; align-items: center; justify-content: space-between; gap: 6px;
    padding: 6px 8px; border-radius: 9px; background: var(--win); border: 1px solid var(--line);
    overflow-x: auto; scrollbar-width: thin;
  }
  .pathops-btns { display: flex; flex-wrap: nowrap; gap: 4px; flex: none; }
  .op-btn {
    height: 24px; padding: 0 8px; border: 1px solid var(--line2); border-radius: 6px;
    background: var(--field); color: var(--ink); font: 600 10.5px var(--sans); white-space: nowrap; flex: none;
  }
  .op-btn:hover:not(:disabled) { border-color: var(--accent); background: var(--accent-soft); }
  .op-btn:disabled { opacity: 0.45; cursor: default; }
  .compact-chip { height: 24px; padding: 0 8px; font-size: 10.5px; white-space: nowrap; flex: none; }

  .svg-drawer {
    display: flex; flex-direction: column; gap: 8px;
    padding: 10px; border-radius: 9px; background: var(--win); border: 1px solid var(--line2);
  }
  .svg-drawer textarea {
    width: 100%; resize: vertical; padding: 8px; border-radius: 7px;
    border: 1px solid var(--line); background: var(--field); color: var(--ink);
    font: 500 11.5px var(--mono);
  }
  .svg-drawer-actions { display: flex; justify-content: flex-end; gap: 6px; }

  .specimen-panel { flex: 1 0 auto; min-height: 160px; }
  .specimen-dock { flex: none; padding: 8px 12px; gap: 8px; }
  .specimen-dock.expanded { min-height: 140px; }
  .specimen-toggle {
    display: inline-flex; align-items: center; gap: 5px;
    border: 0; background: transparent; padding: 0; color: var(--ink2); cursor: pointer;
  }
  .specimen-toggle:hover { color: var(--ink); }
  .test-controls { display: flex; flex-wrap: nowrap; align-items: center; gap: 6px; }
  .fmt-mini { display: flex; gap: 2px; }
  .f-pill { height: 24px; min-width: 22px; padding: 0 5px; border: 1px solid var(--line); border-radius: 6px; background: var(--field); color: var(--ink2); font: 600 10.5px var(--sans); }
  .f-pill.b { font-weight: 800; }
  .f-pill.i { font-style: italic; }
  .f-pill.u { text-decoration: underline; }
  .f-pill.s { text-decoration: line-through; }
  .f-pill.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .test-input {
    height: 24px; width: 140px; min-width: 90px; padding: 0 8px; border: 1px solid var(--line); border-radius: 6px;
    background: var(--field); color: var(--ink); font: 500 11px var(--sans);
  }
  .test-size { width: 64px; }
  .size-val { font: 500 11px var(--mono); color: var(--ink3); width: 32px; text-align: right; }
  .live-stage {
    flex: 1;
    min-height: 76px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--win);
    border: 1px solid var(--line);
    color: var(--ink);
    line-height: 1.3;
    word-break: break-word;
    overflow-y: auto;
  }
  .alphabet-strip {
    font-size: 14px;
    color: var(--ink2);
    letter-spacing: 0.04em;
    white-space: normal;
    overflow-wrap: anywhere;
    overflow-x: hidden;
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
