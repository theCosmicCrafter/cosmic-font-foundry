<script>
  import {
    MousePointer,
    PenTool,
    Square,
    Circle,
    Scissors,
    Ruler,
    ZoomIn,
    ZoomOut,
    RotateCcw,
    Eye,
    EyeOff,
    Plus,
    Trash,
    Spline,
  } from "@lucide/svelte";

  const KAPPA = 0.55228475;
  const ZOOM_MIN = 0.5;
  const ZOOM_MAX = 3.0;

  let {
    glyph = null,
    metrics = {},
    ghostPath = "",
    ghostLabel = "",
    showNodes = true,
    caliperResults = [],
    onUpdateGlyph = null,
    onPushUndo = null,
    onToggleShowNodes = null,
    onMeasureCaliper = null,
    onKnifeSlice = null,
  } = $props();

  const TOOLS = [
    { id: "select", label: "Select", key: "V", icon: MousePointer },
    { id: "pen", label: "Pen", key: "P", icon: PenTool },
    { id: "rect", label: "Rect", key: "R", icon: Square },
    { id: "ellipse", label: "Ellipse", key: "E", icon: Circle },
    { id: "knife", label: "Knife", key: "K", icon: Scissors },
    { id: "caliper", label: "Caliper", key: "M", icon: Ruler },
  ];

  let tool = $state("select");
  let zoom = $state(1);
  let pan = $state({ x: 0, y: 0 });
  let selected = $state(null); // { ci, ai } | null
  let penAnchors = $state([]);
  let penCursor = $state(null); // { x, y } in font coords
  let cursorFont = $state(null); // { x, y } live readout
  let spaceDown = $state(false);
  let drag = $state(null);
  let dragBaseBox = $state(null);
  let localNodesOverride = $state(null);
  let svgEl = $state(null);
  let prevGlyphKey = null;

  const effectiveShowNodes = $derived(
    onToggleShowNodes ? showNodes : (localNodesOverride ?? showNodes),
  );

  const glyphKey = $derived(glyph ? `${glyph.codepoint ?? ""}:${glyph.name ?? ""}` : null);

  $effect(() => {
    const k = glyphKey;
    if (k !== prevGlyphKey) {
      prevGlyphKey = k;
      selected = null;
      penAnchors = [];
      penCursor = null;
      drag = null;
      dragBaseBox = null;
      pan = { x: 0, y: 0 };
    }
  });

  // ---- Metrics & Coordinate Helpers (matching Forge.svelte) ----
  const normMetrics = $derived.by(() => {
    const m = metrics ?? {};
    const upm = m.upm || 1000;
    const asc = m.ascender ?? Math.round(upm * 0.8);
    const rawDesc = m.descender ?? -Math.round(upm * 0.2);
    const desc = rawDesc > 0 ? -rawDesc : rawDesc;
    const cap = m.cap_height ?? Math.round(asc * 0.88);
    const xh = m.x_height ?? Math.round(asc * 0.62);
    return { upm, asc, desc, cap, xh };
  });

  function detectAbsoluteHandles(contours) {
    for (const c of contours ?? []) {
      for (const a of c.anchors ?? []) {
        if (Math.abs(a.x) > 1 || Math.abs(a.y) > 1) {
          if (
            (a.h_in?.[0] === 0 && a.h_in?.[1] === 0) ||
            (a.h_out?.[0] === 0 && a.h_out?.[1] === 0)
          ) {
            return false;
          }
          if (
            Math.abs((a.h_in?.[0] ?? 0) - a.x) < 0.01 &&
            Math.abs((a.h_in?.[1] ?? 0) - a.y) < 0.01
          ) {
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

  const computedBaseBox = $derived.by(() => {
    const { upm, asc, desc } = normMetrics;
    const g = glyph;
    const adv = g?.advance_width ?? Math.round(upm * 0.6);
    let minX = Math.min(0, -upm * 0.12);
    let maxX = Math.max(adv, upm * 0.6) + upm * 0.12;
    let minY = -upm * 0.1;
    let maxY = asc - desc + upm * 0.1;
    if (g?.contours?.length) {
      const absH = detectAbsoluteHandles(g.contours);
      for (const c of g.contours) {
        for (const a of c.anchors ?? []) {
          const ay = asc - a.y;
          minX = Math.min(minX, a.x - upm * 0.08);
          maxX = Math.max(maxX, a.x + upm * 0.08);
          minY = Math.min(minY, ay - upm * 0.08);
          maxY = Math.max(maxY, ay + upm * 0.08);
          for (const h of [a.h_in, a.h_out]) {
            const tip = handleTipSvg(a, h, asc, absH);
            if (tip.hasHandle) {
              minX = Math.min(minX, tip.x - upm * 0.05);
              maxX = Math.max(maxX, tip.x + upm * 0.05);
              minY = Math.min(minY, tip.y - upm * 0.05);
              maxY = Math.max(maxY, tip.y + upm * 0.05);
            }
          }
        }
      }
    }
    return [Math.round(minX), Math.round(minY), Math.round(maxX - minX), Math.round(maxY - minY)];
  });

  // Freeze base viewBox while dragging so getScreenCTM() stays stable without feedback jitter.
  const effectiveBaseBox = $derived(dragBaseBox ?? computedBaseBox);

  const stageViewBox = $derived.by(() => {
    const [bx, by, bw, bh] = effectiveBaseBox;
    const vw = bw / zoom;
    const vh = bh / zoom;
    const vx = bx + (bw - vw) / 2 + pan.x;
    const vy = by + (bh - vh) / 2 + pan.y;
    return [vx, vy, vw, vh];
  });

  const mainPath = $derived(
    glyph ? contoursToSvgPath(glyph.contours, normMetrics.asc) : "",
  );

  const penPreviewPath = $derived.by(() => {
    if (!penAnchors.length) return "";
    const asc = normMetrics.asc;
    const base = contoursToSvgPath([{ closed: false, anchors: penAnchors }], asc);
    if (!penCursor || drag?.type === "pen-point") return base;
    const last = penAnchors[penAnchors.length - 1];
    const fmt = (n) => Math.round(n * 100) / 100;
    const out = handleTipSvg(last, last.h_out, asc, false);
    if (out.hasHandle) {
      return `${base} C ${fmt(out.x)} ${fmt(out.y)} ${fmt(penCursor.x)} ${fmt(asc - penCursor.y)} ${fmt(penCursor.x)} ${fmt(asc - penCursor.y)}`;
    }
    return `${base} L ${fmt(penCursor.x)} ${fmt(asc - penCursor.y)}`;
  });

  const absHandles = $derived(detectAbsoluteHandles(glyph?.contours));

  const selectedContour = $derived(
    selected ? (glyph?.contours?.[selected.ci] ?? null) : null,
  );
  const selectedAnchor = $derived(
    selected ? (selectedContour?.anchors?.[selected.ai] ?? null) : null,
  );
  const canSubdivide = $derived.by(() => {
    if (!selected || !selectedContour || !selectedAnchor) return false;
    const len = selectedContour.anchors?.length ?? 0;
    if (len < 2) return false;
    return selectedContour.closed || selected.ai < len - 1;
  });

  // ---- Coordinate Conversion ----
  function r2(v) {
    return Math.round(v * 100) / 100;
  }

  function clientToSvg(clientX, clientY) {
    if (!svgEl) return null;
    const ctm = svgEl.getScreenCTM();
    if (!ctm) return null;
    const pt = new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse());
    return { x: pt.x, y: pt.y };
  }

  function clientToFont(clientX, clientY) {
    const s = clientToSvg(clientX, clientY);
    if (!s) return null;
    return { x: s.x, y: normMetrics.asc - s.y };
  }

  // ---- Immutable Glyph Cloning (always normalizes to relative handles) ----
  function cloneNormalizedGlyph(g) {
    if (!g) return null;
    const isAbs = detectAbsoluteHandles(g.contours);
    return {
      ...g,
      contours: (g.contours ?? []).map((c) => ({
        closed: !!c.closed,
        anchors: (c.anchors ?? []).map((a) => ({
          x: a.x,
          y: a.y,
          h_in: isAbs
            ? [r2((a.h_in?.[0] ?? a.x) - a.x), r2((a.h_in?.[1] ?? a.y) - a.y)]
            : [a.h_in?.[0] ?? 0, a.h_in?.[1] ?? 0],
          h_out: isAbs
            ? [r2((a.h_out?.[0] ?? a.x) - a.x), r2((a.h_out?.[1] ?? a.y) - a.y)]
            : [a.h_out?.[0] ?? 0, a.h_out?.[1] ?? 0],
          smooth: !!a.smooth,
        })),
      })),
    };
  }

  function commitGlyph(nextGlyph) {
    onUpdateGlyph?.(nextGlyph);
  }

  // ---- Tool Switching & Zoom/View Controls ----
  function setTool(nextTool) {
    if (tool === nextTool) return;
    if (tool === "pen" && penAnchors.length > 0) {
      finalizePenContour(false);
    }
    tool = nextTool;
    drag = null;
    dragBaseBox = null;
  }

  function clampZoom(z) {
    return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z * 100) / 100));
  }

  function zoomIn() {
    zoom = clampZoom(zoom + 0.25);
  }

  function zoomOut() {
    zoom = clampZoom(zoom - 0.25);
  }

  function resetView() {
    zoom = 1;
    pan = { x: 0, y: 0 };
  }

  function handleToggleNodes() {
    if (onToggleShowNodes) onToggleShowNodes();
    else localNodesOverride = !effectiveShowNodes;
  }

  // ---- Node Operations ----
  function applySmoothToggle(contour, ai) {
    const pts = contour.anchors;
    const a = pts[ai];
    if (!a) return;
    const nextSmooth = !a.smooth;
    a.smooth = nextSmooth;
    if (!nextSmooth) return;

    const hasIn = Math.hypot(a.h_in[0], a.h_in[1]) > 0.1;
    const hasOut = Math.hypot(a.h_out[0], a.h_out[1]) > 0.1;

    if (!hasIn && !hasOut) {
      const n = pts.length;
      const prev = ai > 0 ? pts[ai - 1] : contour.closed ? pts[n - 1] : null;
      const next = ai < n - 1 ? pts[ai + 1] : contour.closed ? pts[0] : null;
      let tx = 0;
      let ty = 0;
      if (prev && next) {
        tx = next.x - prev.x;
        ty = next.y - prev.y;
      } else if (next) {
        tx = next.x - a.x;
        ty = next.y - a.y;
      } else if (prev) {
        tx = a.x - prev.x;
        ty = a.y - prev.y;
      }
      const len = Math.hypot(tx, ty);
      if (len > 0.05) {
        const ux = tx / len;
        const uy = ty / len;
        const inLen = prev ? Math.hypot(a.x - prev.x, a.y - prev.y) * 0.28 : 0;
        const outLen = next ? Math.hypot(next.x - a.x, next.y - a.y) * 0.28 : 0;
        a.h_in = prev ? [r2(-ux * inLen), r2(-uy * inLen)] : [0, 0];
        a.h_out = next ? [r2(ux * outLen), r2(uy * outLen)] : [0, 0];
      }
    } else if (hasIn && hasOut) {
      const outLen = Math.hypot(a.h_out[0], a.h_out[1]);
      const inLen = Math.hypot(a.h_in[0], a.h_in[1]);
      if (outLen > 0.05) {
        a.h_in = [
          r2((-a.h_out[0] / outLen) * inLen),
          r2((-a.h_out[1] / outLen) * inLen),
        ];
      }
    }
  }

  function toggleSelectedSmooth() {
    if (!glyph || !selectedAnchor) return;
    const next = cloneNormalizedGlyph(glyph);
    const contour = next.contours[selected.ci];
    if (!contour) return;
    onPushUndo?.();
    applySmoothToggle(contour, selected.ai);
    commitGlyph(next);
  }

  function subdivideSelectedSegment() {
    if (!glyph || !canSubdivide) return;
    const next = cloneNormalizedGlyph(glyph);
    const contour = next.contours[selected.ci];
    const pts = contour.anchors;
    const ai = selected.ai;
    const nextIdx = (ai + 1) % pts.length;
    const a = pts[ai];
    const b = pts[nextIdx];

    onPushUndo?.();

    const isLine =
      Math.abs(a.h_out[0]) < 1e-4 &&
      Math.abs(a.h_out[1]) < 1e-4 &&
      Math.abs(b.h_in[0]) < 1e-4 &&
      Math.abs(b.h_in[1]) < 1e-4;

    let midAnchor;
    if (isLine) {
      midAnchor = {
        x: r2((a.x + b.x) / 2),
        y: r2((a.y + b.y) / 2),
        h_in: [0, 0],
        h_out: [0, 0],
        smooth: false,
      };
    } else {
      const p0 = [a.x, a.y];
      const p1 = [a.x + a.h_out[0], a.y + a.h_out[1]];
      const p2 = [b.x + b.h_in[0], b.y + b.h_in[1]];
      const p3 = [b.x, b.y];

      const m01 = [(p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2];
      const m12 = [(p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2];
      const m23 = [(p2[0] + p3[0]) / 2, (p2[1] + p3[1]) / 2];
      const m012 = [(m01[0] + m12[0]) / 2, (m01[1] + m12[1]) / 2];
      const m123 = [(m12[0] + m23[0]) / 2, (m12[1] + m23[1]) / 2];
      const m = [(m012[0] + m123[0]) / 2, (m012[1] + m123[1]) / 2];

      a.h_out = [r2(m01[0] - p0[0]), r2(m01[1] - p0[1])];
      b.h_in = [r2(m23[0] - p3[0]), r2(m23[1] - p3[1])];
      midAnchor = {
        x: r2(m[0]),
        y: r2(m[1]),
        h_in: [r2(m012[0] - m[0]), r2(m012[1] - m[1])],
        h_out: [r2(m123[0] - m[0]), r2(m123[1] - m[1])],
        smooth: true,
      };
    }

    pts.splice(ai + 1, 0, midAnchor);
    selected = { ci: selected.ci, ai: ai + 1 };
    commitGlyph(next);
  }

  function deleteSelectedAnchor() {
    if (!glyph || !selectedAnchor) return;
    const next = cloneNormalizedGlyph(glyph);
    const contour = next.contours[selected.ci];
    if (!contour) return;
    onPushUndo?.();
    contour.anchors.splice(selected.ai, 1);
    if (contour.anchors.length < 2) {
      next.contours.splice(selected.ci, 1);
    }
    selected = null;
    commitGlyph(next);
  }

  // ---- Pen Finalization ----
  function finalizePenContour(closed) {
    if (!glyph || penAnchors.length < 2) {
      penAnchors = [];
      penCursor = null;
      return;
    }
    const next = cloneNormalizedGlyph(glyph);
    onPushUndo?.();
    next.contours.push({
      closed,
      anchors: penAnchors.map((a) => ({
        x: a.x,
        y: a.y,
        h_in: [...a.h_in],
        h_out: [...a.h_out],
        smooth: !!a.smooth,
      })),
    });
    const newCi = next.contours.length - 1;
    selected = { ci: newCi, ai: closed ? 0 : penAnchors.length - 1 };
    penAnchors = [];
    penCursor = null;
    commitGlyph(next);
  }

  // ---- Constraint Helpers ----
  function snapAngleDelta(dx, dy) {
    const len = Math.hypot(dx, dy);
    if (len < 1e-4) return { dx: 0, dy: 0 };
    const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
    return { dx: Math.cos(angle) * len, dy: Math.sin(angle) * len };
  }

  function constrainSquareOrCircle(sx, sy, cx, cy) {
    const dx = cx - sx;
    const dy = cy - sy;
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    return {
      x: sx + (dx >= 0 ? side : -side),
      y: sy + (dy >= 0 ? side : -side),
    };
  }

  // ---- Pointer Interactions ----
  function startPan(e) {
    if (!svgEl) return;
    const ctm = svgEl.getScreenCTM();
    if (!ctm) return;
    dragBaseBox = [...computedBaseBox];
    drag = {
      type: "pan",
      startClientX: e.clientX,
      startClientY: e.clientY,
      startPanX: pan.x,
      startPanY: pan.y,
      scaleX: 1 / ctm.a,
      scaleY: 1 / ctm.d,
    };
  }

  function onAnchorPointerDown(e, ci, ai) {
    if (e.button === 1 || spaceDown) {
      e.preventDefault();
      e.stopPropagation();
      startPan(e);
      return;
    }
    if (tool !== "select" || e.button !== 0) return;
    e.stopPropagation();
    const f = clientToFont(e.clientX, e.clientY);
    if (!f) return;
    const anchor = glyph?.contours?.[ci]?.anchors?.[ai];
    if (!anchor) return;
    selected = { ci, ai };
    dragBaseBox = [...computedBaseBox];
    drag = {
      type: "anchor",
      ci,
      ai,
      startAnchor: { x: anchor.x, y: anchor.y },
      pointerStart: f,
      undoPushed: false,
    };
  }

  function onAnchorDblClick(e, ci, ai) {
    if (tool !== "select" || !glyph) return;
    e.stopPropagation();
    selected = { ci, ai };
    const next = cloneNormalizedGlyph(glyph);
    const contour = next.contours[ci];
    if (!contour) return;
    onPushUndo?.();
    applySmoothToggle(contour, ai);
    commitGlyph(next);
  }

  function onHandlePointerDown(e, ci, ai, which) {
    if (e.button === 1 || spaceDown) {
      e.preventDefault();
      e.stopPropagation();
      startPan(e);
      return;
    }
    if (tool !== "select" || e.button !== 0) return;
    e.stopPropagation();
    selected = { ci, ai };
    dragBaseBox = [...computedBaseBox];
    drag = {
      type: "handle",
      ci,
      ai,
      which,
      undoPushed: false,
    };
  }

  function onAdvanceGuidePointerDown(e) {
    if (e.button === 1 || spaceDown) {
      e.preventDefault();
      e.stopPropagation();
      startPan(e);
      return;
    }
    if (tool !== "select" || e.button !== 0) return;
    e.stopPropagation();
    dragBaseBox = [...computedBaseBox];
    drag = {
      type: "advance",
      undoPushed: false,
    };
  }

  function onStagePointerDown(e) {
    if (!glyph) return;
    if (e.button === 1 || (e.button === 0 && spaceDown)) {
      e.preventDefault();
      startPan(e);
      return;
    }
    if (e.button !== 0) return;

    const f = clientToFont(e.clientX, e.clientY);
    if (!f) return;

    if (tool === "select") {
      selected = null;
      return;
    }

    if (tool === "pen") {
      const closeThresh = Math.max(8, (normMetrics.upm * 0.024) / zoom);
      if (penAnchors.length >= 2) {
        const p0 = penAnchors[0];
        if (Math.hypot(f.x - p0.x, f.y - p0.y) <= closeThresh) {
          dragBaseBox = [...computedBaseBox];
          drag = { type: "pen-close" };
          return;
        }
      }
      const pt = {
        x: Math.round(f.x),
        y: Math.round(f.y),
        h_in: [0, 0],
        h_out: [0, 0],
        smooth: false,
      };
      penAnchors = [...penAnchors, pt];
      dragBaseBox = [...computedBaseBox];
      drag = {
        type: "pen-point",
        index: penAnchors.length - 1,
      };
      return;
    }

    if (tool === "rect" || tool === "ellipse") {
      dragBaseBox = [...computedBaseBox];
      drag = {
        type: "shape",
        kind: tool,
        start: { x: Math.round(f.x), y: Math.round(f.y) },
        cur: { x: Math.round(f.x), y: Math.round(f.y) },
      };
      return;
    }

    if (tool === "knife" || tool === "caliper") {
      dragBaseBox = [...computedBaseBox];
      drag = {
        type: "line",
        kind: tool,
        start: { x: r2(f.x), y: r2(f.y) },
        cur: { x: r2(f.x), y: r2(f.y) },
      };
    }
  }

  function onStagePointerMove(e) {
    const f = clientToFont(e.clientX, e.clientY);
    if (f) {
      cursorFont = { x: Math.round(f.x), y: Math.round(f.y) };
      if (tool === "pen" && penAnchors.length > 0 && !drag) {
        penCursor = f;
      }
    }
  }

  function onStagePointerLeave() {
    if (!drag) {
      cursorFont = null;
      penCursor = null;
    }
  }

  function onWindowPointerMove(e) {
    if (!drag) return;

    if (drag.type === "pan") {
      pan = {
        x: drag.startPanX - (e.clientX - drag.startClientX) * drag.scaleX,
        y: drag.startPanY - (e.clientY - drag.startClientY) * drag.scaleY,
      };
      return;
    }

    const f = clientToFont(e.clientX, e.clientY);
    if (!f || !glyph) return;
    cursorFont = { x: Math.round(f.x), y: Math.round(f.y) };

    if (drag.type === "anchor") {
      let dx = f.x - drag.pointerStart.x;
      let dy = f.y - drag.pointerStart.y;
      if (e.shiftKey) {
        if (Math.abs(dx) >= Math.abs(dy)) dy = 0;
        else dx = 0;
      }
      const nx = Math.round(drag.startAnchor.x + dx);
      const ny = Math.round(drag.startAnchor.y + dy);
      const curA = glyph.contours?.[drag.ci]?.anchors?.[drag.ai];
      if (!curA || (nx === curA.x && ny === curA.y)) return;
      if (!drag.undoPushed) {
        onPushUndo?.();
        drag.undoPushed = true;
      }
      const next = cloneNormalizedGlyph(glyph);
      const target = next.contours[drag.ci]?.anchors?.[drag.ai];
      if (target) {
        target.x = nx;
        target.y = ny;
        commitGlyph(next);
      }
      return;
    }

    if (drag.type === "handle") {
      if (!drag.undoPushed) {
        onPushUndo?.();
        drag.undoPushed = true;
      }
      const next = cloneNormalizedGlyph(glyph);
      const anchor = next.contours[drag.ci]?.anchors?.[drag.ai];
      if (!anchor) return;
      let hx = f.x - anchor.x;
      let hy = f.y - anchor.y;
      if (e.shiftKey) {
        const snapped = snapAngleDelta(hx, hy);
        hx = snapped.dx;
        hy = snapped.dy;
      }
      hx = r2(hx);
      hy = r2(hy);
      anchor[drag.which] = [hx, hy];

      if (e.altKey) {
        anchor.smooth = false;
      } else if (anchor.smooth) {
        const otherWhich = drag.which === "h_in" ? "h_out" : "h_in";
        const [ox, oy] = anchor[otherWhich];
        const otherLen = Math.hypot(ox, oy);
        const curLen = Math.hypot(hx, hy);
        if (curLen > 0.01 && otherLen > 0.01) {
          anchor[otherWhich] = [
            r2((-hx / curLen) * otherLen),
            r2((-hy / curLen) * otherLen),
          ];
        }
      }
      commitGlyph(next);
      return;
    }

    if (drag.type === "advance") {
      const nextAdv = Math.max(0, Math.round(f.x));
      if (nextAdv === glyph.advance_width) return;
      if (!drag.undoPushed) {
        onPushUndo?.();
        drag.undoPushed = true;
      }
      const next = cloneNormalizedGlyph(glyph);
      next.advance_width = nextAdv;
      commitGlyph(next);
      return;
    }

    if (drag.type === "pen-point") {
      const pt = penAnchors[drag.index];
      if (!pt) return;
      let dx = f.x - pt.x;
      let dy = f.y - pt.y;
      if (e.shiftKey) {
        const snapped = snapAngleDelta(dx, dy);
        dx = snapped.dx;
        dy = snapped.dy;
      }
      if (Math.hypot(dx, dy) > 2) {
        const updated = [...penAnchors];
        updated[drag.index] = {
          ...pt,
          h_out: [r2(dx), r2(dy)],
          h_in: [r2(-dx), r2(-dy)],
          smooth: true,
        };
        penAnchors = updated;
      }
      return;
    }

    if (drag.type === "pen-close") {
      const p0 = penAnchors[0];
      if (!p0) return;
      let dx = f.x - p0.x;
      let dy = f.y - p0.y;
      if (e.shiftKey) {
        const snapped = snapAngleDelta(dx, dy);
        dx = snapped.dx;
        dy = snapped.dy;
      }
      if (Math.hypot(dx, dy) > 2) {
        const updated = [...penAnchors];
        updated[0] = {
          ...p0,
          h_out: [r2(dx), r2(dy)],
          h_in: [r2(-dx), r2(-dy)],
          smooth: true,
        };
        penAnchors = updated;
      }
      return;
    }

    if (drag.type === "shape") {
      const end = e.shiftKey
        ? constrainSquareOrCircle(drag.start.x, drag.start.y, f.x, f.y)
        : f;
      drag = {
        ...drag,
        cur: { x: Math.round(end.x), y: Math.round(end.y) },
      };
      return;
    }

    if (drag.type === "line") {
      let endX = f.x;
      let endY = f.y;
      if (e.shiftKey) {
        const snapped = snapAngleDelta(f.x - drag.start.x, f.y - drag.start.y);
        endX = drag.start.x + snapped.dx;
        endY = drag.start.y + snapped.dy;
      }
      drag = {
        ...drag,
        cur: { x: r2(endX), y: r2(endY) },
      };
    }
  }

  function finishShapeDrag(d) {
    const x0 = Math.min(d.start.x, d.cur.x);
    const x1 = Math.max(d.start.x, d.cur.x);
    const y0 = Math.min(d.start.y, d.cur.y);
    const y1 = Math.max(d.start.y, d.cur.y);
    if (x1 - x0 < 2 || y1 - y0 < 2) return;

    const next = cloneNormalizedGlyph(glyph);
    onPushUndo?.();

    let anchors;
    if (d.kind === "rect") {
      // CW in Y-up: TL -> TR -> BR -> BL
      anchors = [
        { x: x0, y: y1, h_in: [0, 0], h_out: [0, 0], smooth: false },
        { x: x1, y: y1, h_in: [0, 0], h_out: [0, 0], smooth: false },
        { x: x1, y: y0, h_in: [0, 0], h_out: [0, 0], smooth: false },
        { x: x0, y: y0, h_in: [0, 0], h_out: [0, 0], smooth: false },
      ];
    } else {
      // 4-anchor smooth cubic ellipse (CW in Y-up)
      const cx = r2((x0 + x1) / 2);
      const cy = r2((y0 + y1) / 2);
      const rx = r2((x1 - x0) / 2);
      const ry = r2((y1 - y0) / 2);
      const kx = r2(rx * KAPPA);
      const ky = r2(ry * KAPPA);
      anchors = [
        { x: cx, y: r2(cy + ry), h_in: [-kx, 0], h_out: [kx, 0], smooth: true },
        { x: r2(cx + rx), y: cy, h_in: [0, ky], h_out: [0, -ky], smooth: true },
        { x: cx, y: r2(cy - ry), h_in: [kx, 0], h_out: [-kx, 0], smooth: true },
        { x: r2(cx - rx), y: cy, h_in: [0, -ky], h_out: [0, ky], smooth: true },
      ];
    }

    next.contours.push({ closed: true, anchors });
    selected = { ci: next.contours.length - 1, ai: 0 };
    commitGlyph(next);
  }

  function finishLineDrag(d) {
    const len = Math.hypot(d.cur.x - d.start.x, d.cur.y - d.start.y);
    if (len < 2) return;
    const p0 = [d.start.x, d.start.y];
    const p1 = [d.cur.x, d.cur.y];
    if (d.kind === "knife") onKnifeSlice?.(p0, p1);
    else onMeasureCaliper?.(p0, p1);
  }

  function onWindowPointerUp() {
    if (!drag) return;
    const finished = drag;
    drag = null;
    dragBaseBox = null;

    if (finished.type === "pen-close") {
      finalizePenContour(true);
    } else if (finished.type === "shape") {
      finishShapeDrag(finished);
    } else if (finished.type === "line") {
      finishLineDrag(finished);
    }
  }

  function onStageWheel(e) {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    zoom = clampZoom(zoom * factor);
  }

  // ---- Keyboard Shortcuts ----
  function isTypingTarget(target) {
    if (!target) return false;
    const tag = target.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
  }

  function onWindowKeyDown(e) {
    if (isTypingTarget(e.target)) return;

    if (e.code === "Space") {
      const activeTag = document.activeElement?.tagName;
      if (activeTag !== "BUTTON") {
        e.preventDefault();
        spaceDown = true;
      }
      return;
    }

    if (e.ctrlKey || e.metaKey || e.altKey) return;

    const k = e.key.toLowerCase();
    if (k === "v") setTool("select");
    else if (k === "p") setTool("pen");
    else if (k === "r") setTool("rect");
    else if (k === "e") setTool("ellipse");
    else if (k === "k") setTool("knife");
    else if (k === "m") setTool("caliper");
    else if ((e.key === "Delete" || e.key === "Backspace") && tool === "select" && selectedAnchor) {
      e.preventDefault();
      deleteSelectedAnchor();
    } else if (e.key === "Enter" || e.key === "Escape") {
      if (penAnchors.length > 0) {
        e.preventDefault();
        finalizePenContour(false);
      } else if (e.key === "Escape" && drag) {
        drag = null;
        dragBaseBox = null;
      }
    }
  }

  function onWindowKeyUp(e) {
    if (e.code === "Space") {
      spaceDown = false;
    }
  }
</script>

<svelte:window
  onpointermove={onWindowPointerMove}
  onpointerup={onWindowPointerUp}
  onpointercancel={onWindowPointerUp}
  onkeydown={onWindowKeyDown}
  onkeyup={onWindowKeyUp}
/>

<div class="v-editor">
  <!-- Canvas Tool Strip -->
  <div class="v-toolbar">
    <div class="v-tool-group" role="toolbar" aria-label="Vector tools">
      {#each TOOLS as tItem (tItem.id)}
        {@const Icon = tItem.icon}
        <button
          type="button"
          class="v-tbtn"
          class:active={tool === tItem.id}
          title="{tItem.label} ({tItem.key})"
          aria-label="{tItem.label} ({tItem.key})"
          aria-pressed={tool === tItem.id}
          onclick={() => setTool(tItem.id)}
        >
          <Icon size={13} />
          <span>{tItem.label}</span>
          <kbd>{tItem.key}</kbd>
        </button>
      {/each}
    </div>

    <div class="v-tool-group">
      {#if selectedAnchor}
        <button
          type="button"
          class="v-tbtn"
          class:active={selectedAnchor.smooth}
          title="Toggle Smooth / Corner (Double-click anchor)"
          onclick={toggleSelectedSmooth}
        >
          <Spline size={13} />
          <span>{selectedAnchor.smooth ? "Smooth" : "Corner"}</span>
        </button>
        <button
          type="button"
          class="v-tbtn"
          disabled={!canSubdivide}
          title="Subdivide outgoing segment at t = 0.5"
          onclick={subdivideSelectedSegment}
        >
          <Plus size={13} />
          <span>Subdivide Segment</span>
        </button>
        <button
          type="button"
          class="v-tbtn danger"
          title="Delete selected anchor (Del)"
          onclick={deleteSelectedAnchor}
        >
          <Trash size={13} />
          <span>Delete Anchor</span>
        </button>
      {:else if tool === "pen" && penAnchors.length > 0}
        <button
          type="button"
          class="v-tbtn accent"
          disabled={penAnchors.length < 2}
          title="Finish open stroke (Enter / Esc)"
          onclick={() => finalizePenContour(false)}
        >
          <span>Finish Open ({penAnchors.length})</span>
        </button>
      {/if}
    </div>

    <div class="v-tool-group v-zoom-group">
      <button
        type="button"
        class="v-tbtn icon-only"
        title="Zoom out"
        aria-label="Zoom out"
        disabled={zoom <= ZOOM_MIN}
        onclick={zoomOut}
      >
        <ZoomOut size={13} />
      </button>
      <button
        type="button"
        class="v-tbtn zoom-readout"
        title="Reset zoom & pan to 100%"
        onclick={resetView}
      >
        {Math.round(zoom * 100)}%
      </button>
      <button
        type="button"
        class="v-tbtn icon-only"
        title="Zoom in"
        aria-label="Zoom in"
        disabled={zoom >= ZOOM_MAX}
        onclick={zoomIn}
      >
        <ZoomIn size={13} />
      </button>
      <button
        type="button"
        class="v-tbtn icon-only"
        title="Reset pan & zoom"
        aria-label="Reset view"
        disabled={zoom === 1 && pan.x === 0 && pan.y === 0}
        onclick={resetView}
      >
        <RotateCcw size={12} />
      </button>
      <button
        type="button"
        class="v-tbtn icon-only"
        class:active={effectiveShowNodes}
        title={effectiveShowNodes ? "Hide nodes" : "Show nodes"}
        aria-label={effectiveShowNodes ? "Hide nodes" : "Show nodes"}
        aria-pressed={effectiveShowNodes}
        onclick={handleToggleNodes}
      >
        {#if effectiveShowNodes}
          <Eye size={13} />
        {:else}
          <EyeOff size={13} />
        {/if}
      </button>
    </div>
  </div>

  <!-- Interactive SVG Canvas Stage -->
  <div
    class="v-stage-wrap"
    class:panning={spaceDown || drag?.type === "pan"}
    class:crosshair={tool !== "select" && !spaceDown && drag?.type !== "pan"}
    onwheel={onStageWheel}
  >
    {#if ghostPath && ghostLabel}
      <div class="v-ghost-badge">{ghostLabel}</div>
    {/if}

    {#if cursorFont}
      <div class="v-coord-badge">
        x {cursorFont.x}, y {cursorFont.y}
      </div>
    {/if}

    {#if glyph}
      {@const { upm, asc, desc, cap, xh } = normMetrics}
      {@const vb = stageViewBox}
      {@const invZ = 1 / zoom}
      {@const sw = Math.max(0.5, upm * 0.0022 * invZ)}
      {@const nodeR = Math.max(2, upm * 0.0085 * invZ)}
      {@const lblSize = upm * 0.026 * invZ}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <svg
        bind:this={svgEl}
        class="v-svg"
        viewBox={vb.join(" ")}
        preserveAspectRatio="xMidYMid meet"
        role="application"
        aria-label="Interactive glyph vector editor"
        onpointerdown={onStagePointerDown}
        onpointermove={onStagePointerMove}
        onpointerleave={onStagePointerLeave}
      >
        <!-- Horizontal Typographic Guides -->
        <line x1={vb[0]} y1="0" x2={vb[0] + vb[2]} y2="0" class="v-guide v-asc" stroke-width={sw} />
        <line x1={vb[0]} y1={asc - cap} x2={vb[0] + vb[2]} y2={asc - cap} class="v-guide v-cap" stroke-width={sw} />
        <line x1={vb[0]} y1={asc - xh} x2={vb[0] + vb[2]} y2={asc - xh} class="v-guide v-xh" stroke-width={sw} />
        <line x1={vb[0]} y1={asc} x2={vb[0] + vb[2]} y2={asc} class="v-guide v-base" stroke-width={sw * 1.4} />
        <line x1={vb[0]} y1={asc - desc} x2={vb[0] + vb[2]} y2={asc - desc} class="v-guide v-desc" stroke-width={sw} />

        <!-- Left & Right Sidebearing Guides -->
        <line x1="0" y1={vb[1]} x2="0" y2={vb[1] + vb[3]} class="v-guide v-sb" stroke-width={sw} />
        <line
          x1={glyph.advance_width}
          y1={vb[1]}
          x2={glyph.advance_width}
          y2={vb[1] + vb[3]}
          class="v-guide v-sb"
          class:v-sb-active={drag?.type === "advance"}
          stroke-width={sw * (drag?.type === "advance" ? 1.8 : 1)}
        />
        {#if tool === "select"}
          <line
            x1={glyph.advance_width}
            y1={vb[1]}
            x2={glyph.advance_width}
            y2={vb[1] + vb[3]}
            class="v-sb-hit"
            stroke="transparent"
            stroke-width={nodeR * 3.2}
            role="slider"
            tabindex="-1"
            aria-label="Advance width guide"
            aria-valuenow={glyph.advance_width}
            onpointerdown={onAdvanceGuidePointerDown}
          >
            <title>Drag advance width ({glyph.advance_width})</title>
          </line>
        {/if}

        <!-- Guide Labels -->
        <text x={vb[0] + upm * 0.012 * invZ} y={-upm * 0.01 * invZ} font-size={lblSize} class="v-lbl">ASC {asc}</text>
        <text x={vb[0] + upm * 0.012 * invZ} y={asc - cap - upm * 0.01 * invZ} font-size={lblSize} class="v-lbl">CAP {cap}</text>
        <text x={vb[0] + upm * 0.012 * invZ} y={asc - xh - upm * 0.01 * invZ} font-size={lblSize} class="v-lbl">X-H {xh}</text>
        <text x={vb[0] + upm * 0.012 * invZ} y={asc - upm * 0.01 * invZ} font-size={lblSize} class="v-lbl v-lbl-base">BASE 0</text>
        <text x={vb[0] + upm * 0.012 * invZ} y={asc - desc - upm * 0.01 * invZ} font-size={lblSize} class="v-lbl">DESC {desc}</text>
        <text
          x={glyph.advance_width + upm * 0.01 * invZ}
          y={asc + lblSize * 1.2}
          font-size={lblSize * 0.9}
          class="v-lbl v-lbl-sb"
        >
          ADV {glyph.advance_width}
        </text>

        <!-- Ghost Reference Underlay -->
        {#if ghostPath}
          <path d={ghostPath} class="v-ghost" />
        {/if}

        <!-- Main Glyph Path -->
        <path d={mainPath} class="v-path" />

        <!-- Active Pen Working Contour Preview -->
        {#if penAnchors.length > 0}
          <path d={penPreviewPath} class="v-pen-preview" />
          {#each penAnchors as pt, idx (idx)}
            {@const ax = pt.x}
            {@const ay = asc - pt.y}
            {@const hin = handleTipSvg(pt, pt.h_in, asc, false)}
            {@const hout = handleTipSvg(pt, pt.h_out, asc, false)}
            {#if hin.hasHandle}
              <line x1={ax} y1={ay} x2={hin.x} y2={hin.y} class="v-handle-line" stroke-width={sw} />
              <circle cx={hin.x} cy={hin.y} r={nodeR * 0.68} class="v-handle-dot" />
            {/if}
            {#if hout.hasHandle}
              <line x1={ax} y1={ay} x2={hout.x} y2={hout.y} class="v-handle-line" stroke-width={sw} />
              <circle cx={hout.x} cy={hout.y} r={nodeR * 0.68} class="v-handle-dot" />
            {/if}
            <circle
              cx={ax}
              cy={ay}
              r={idx === 0 && penAnchors.length >= 2 ? nodeR * 1.25 : nodeR}
              class={idx === 0 ? "v-pen-start" : "v-node-smooth"}
            />
          {/each}
        {/if}

        <!-- Anchor Nodes & Bezier Handles -->
        {#if effectiveShowNodes}
          {#each glyph.contours ?? [] as contour, ci (ci)}
            {#each contour.anchors ?? [] as pt, ai (ai)}
              {@const ax = pt.x}
              {@const ay = asc - pt.y}
              {@const hin = handleTipSvg(pt, pt.h_in, asc, absHandles)}
              {@const hout = handleTipSvg(pt, pt.h_out, asc, absHandles)}
              {@const isSel = selected?.ci === ci && selected?.ai === ai}
              {#if hin.hasHandle}
                <line x1={ax} y1={ay} x2={hin.x} y2={hin.y} class="v-handle-line" stroke-width={sw} />
                <circle
                  cx={hin.x}
                  cy={hin.y}
                  r={nodeR * 0.72}
                  class="v-handle-dot"
                  class:interactive={tool === "select"}
                  role="button"
                  tabindex="-1"
                  aria-label="Incoming handle"
                  onpointerdown={(e) => onHandlePointerDown(e, ci, ai, "h_in")}
                />
              {/if}
              {#if hout.hasHandle}
                <line x1={ax} y1={ay} x2={hout.x} y2={hout.y} class="v-handle-line" stroke-width={sw} />
                <circle
                  cx={hout.x}
                  cy={hout.y}
                  r={nodeR * 0.72}
                  class="v-handle-dot"
                  class:interactive={tool === "select"}
                  role="button"
                  tabindex="-1"
                  aria-label="Outgoing handle"
                  onpointerdown={(e) => onHandlePointerDown(e, ci, ai, "h_out")}
                />
              {/if}
              {#if pt.smooth}
                <circle
                  cx={ax}
                  cy={ay}
                  r={isSel ? nodeR * 1.25 : nodeR}
                  class="v-node-smooth"
                  class:selected={isSel}
                  class:interactive={tool === "select"}
                  role="button"
                  tabindex="-1"
                  aria-label="Smooth anchor ({Math.round(pt.x)}, {Math.round(pt.y)})"
                  onpointerdown={(e) => onAnchorPointerDown(e, ci, ai)}
                  ondblclick={(e) => onAnchorDblClick(e, ci, ai)}
                >
                  <title>({Math.round(pt.x)}, {Math.round(pt.y)})</title>
                </circle>
              {:else}
                {@const r = isSel ? nodeR * 1.25 : nodeR}
                <rect
                  x={ax - r}
                  y={ay - r}
                  width={r * 2}
                  height={r * 2}
                  class="v-node-corner"
                  class:selected={isSel}
                  class:interactive={tool === "select"}
                  role="button"
                  tabindex="-1"
                  aria-label="Corner anchor ({Math.round(pt.x)}, {Math.round(pt.y)})"
                  onpointerdown={(e) => onAnchorPointerDown(e, ci, ai)}
                  ondblclick={(e) => onAnchorDblClick(e, ci, ai)}
                >
                  <title>({Math.round(pt.x)}, {Math.round(pt.y)})</title>
                </rect>
              {/if}
            {/each}
          {/each}
        {/if}

        <!-- Rect / Ellipse Live Drag Preview -->
        {#if drag?.type === "shape"}
          {@const sx0 = Math.min(drag.start.x, drag.cur.x)}
          {@const sx1 = Math.max(drag.start.x, drag.cur.x)}
          {@const sy0 = asc - Math.max(drag.start.y, drag.cur.y)}
          {@const sy1 = asc - Math.min(drag.start.y, drag.cur.y)}
          {#if drag.kind === "rect"}
            <rect
              x={sx0}
              y={sy0}
              width={sx1 - sx0}
              height={sy1 - sy0}
              class="v-shape-preview"
            />
          {:else}
            <ellipse
              cx={(sx0 + sx1) / 2}
              cy={(sy0 + sy1) / 2}
              rx={(sx1 - sx0) / 2}
              ry={(sy1 - sy0) / 2}
              class="v-shape-preview"
            />
          {/if}
        {/if}

        <!-- Knife / Caliper Live Drag Preview -->
        {#if drag?.type === "line"}
          {@const lx0 = drag.start.x}
          {@const ly0 = asc - drag.start.y}
          {@const lx1 = drag.cur.x}
          {@const ly1 = asc - drag.cur.y}
          {@const dist = Math.hypot(drag.cur.x - drag.start.x, drag.cur.y - drag.start.y)}
          <line
            x1={lx0}
            y1={ly0}
            x2={lx1}
            y2={ly1}
            class={drag.kind === "knife" ? "v-knife-preview" : "v-caliper-line"}
            stroke-width={sw * 1.4}
          />
          {#if drag.kind === "caliper" && dist >= 1}
            {@const mx = (lx0 + lx1) / 2}
            {@const my = (ly0 + ly1) / 2}
            {@const labelText = `${Math.round(dist)} u`}
            {@const pillW = Math.max(lblSize * 3.6, labelText.length * lblSize * 0.68 + lblSize * 0.8)}
            {@const pillH = lblSize * 1.5}
            <rect
              x={mx - pillW / 2}
              y={my - pillH / 2}
              width={pillW}
              height={pillH}
              rx={pillH / 2}
              class="v-caliper-pill"
            />
            <text x={mx} y={my} font-size={lblSize * 0.92} class="v-caliper-text">
              {labelText}
            </text>
          {/if}
        {/if}

        <!-- Saved Caliper Measurement Results -->
        {#each caliperResults ?? [] as r, idx (idx)}
          {@const cx0 = r.p0?.[0] ?? 0}
          {@const cy0 = asc - (r.p0?.[1] ?? 0)}
          {@const cx1 = r.p1?.[0] ?? 0}
          {@const cy1 = asc - (r.p1?.[1] ?? 0)}
          {@const mx = (cx0 + cx1) / 2}
          {@const my = (cy0 + cy1) / 2}
          {@const labelText = `${Math.round(r.distance ?? 0)} u`}
          {@const pillW = Math.max(lblSize * 3.6, labelText.length * lblSize * 0.68 + lblSize * 0.8)}
          {@const pillH = lblSize * 1.5}
          <g class="v-caliper-group">
            <line x1={cx0} y1={cy0} x2={cx1} y2={cy1} class="v-caliper-line" stroke-width={sw * 1.25} />
            <circle cx={cx0} cy={cy0} r={nodeR * 0.6} class="v-caliper-dot" />
            <circle cx={cx1} cy={cy1} r={nodeR * 0.6} class="v-caliper-dot" />
            <rect
              x={mx - pillW / 2}
              y={my - pillH / 2}
              width={pillW}
              height={pillH}
              rx={pillH / 2}
              class="v-caliper-pill"
            />
            <text x={mx} y={my} font-size={lblSize * 0.92} class="v-caliper-text">
              {labelText}
            </text>
          </g>
        {/each}
      </svg>
    {/if}
  </div>
</div>

<style>
  .v-editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
    min-height: 0;
  }

  .v-toolbar {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 7px;
    border-radius: 9px;
    background: var(--side);
    border: 1px solid var(--line);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .v-tool-group {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 3px;
    flex: none;
  }

  .v-zoom-group {
    margin-left: auto;
  }

  .v-tbtn {
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--field);
    color: var(--ink2);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    flex: none;
    font: 600 10.5px var(--sans);
    transition: border-color 0.12s, background-color 0.12s, color 0.12s;
  }

  .v-tbtn:hover:not(:disabled) {
    border-color: var(--accent-2);
    color: var(--ink);
  }

  .v-tbtn.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--ink);
  }

  .v-tbtn.accent {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }

  .v-tbtn.danger:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }

  .v-tbtn:disabled {
    opacity: 0.42;
    cursor: default;
  }

  .v-tbtn.icon-only {
    width: 25px;
    padding: 0;
    justify-content: center;
  }

  .v-tbtn.zoom-readout {
    min-width: 46px;
    justify-content: center;
    font-family: var(--mono);
    font-size: 10.5px;
  }

  .v-tbtn kbd {
    font: 600 9.5px var(--mono);
    color: var(--ink3);
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--chip);
  }

  .v-tbtn.active kbd {
    color: var(--accent);
  }

  .v-stage-wrap {
    position: relative;
    flex: 1;
    min-height: 260px;
    border-radius: 10px;
    background: var(--win);
    border: 1px solid var(--line);
    padding: 12px;
    display: grid;
    place-items: center;
    overflow: hidden;
  }

  .v-stage-wrap.crosshair {
    cursor: crosshair;
  }

  .v-stage-wrap.panning {
    cursor: grab;
  }

  .v-stage-wrap.panning:active {
    cursor: grabbing;
  }

  .v-ghost-badge {
    position: absolute;
    top: 8px;
    left: 10px;
    z-index: 2;
    pointer-events: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--field);
    border: 1px solid var(--line2);
    color: var(--ink3);
    font: 600 10px var(--mono);
  }

  .v-coord-badge {
    position: absolute;
    bottom: 8px;
    right: 10px;
    z-index: 2;
    pointer-events: none;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink3);
    font: 500 10px var(--mono);
    opacity: 0.9;
  }

  .v-svg {
    width: 100%;
    height: 100%;
    max-height: 400px;
    overflow: visible;
    color: var(--ink);
    user-select: none;
  }

  .v-guide {
    stroke-dasharray: 16 14;
    opacity: 0.5;
    pointer-events: none;
  }

  .v-asc {
    stroke: rgba(148, 163, 184, 0.5);
  }

  .v-cap {
    stroke: rgba(56, 189, 248, 0.5);
  }

  .v-xh {
    stroke: rgba(251, 191, 36, 0.55);
  }

  .v-base {
    stroke: rgba(242, 107, 33, 0.75);
    stroke-dasharray: none;
    opacity: 0.85;
  }

  .v-desc {
    stroke: rgba(148, 163, 184, 0.45);
  }

  .v-sb {
    stroke: rgba(56, 189, 248, 0.45);
  }

  .v-sb.v-sb-active {
    stroke: var(--accent-2);
    opacity: 0.95;
  }

  .v-sb-hit {
    cursor: ew-resize;
  }

  .v-lbl {
    fill: var(--ink3);
    font-family: var(--mono);
    font-weight: 600;
    opacity: 0.75;
    pointer-events: none;
  }

  .v-lbl-base {
    fill: var(--accent);
    opacity: 0.9;
  }

  .v-lbl-sb {
    fill: rgba(56, 189, 248, 0.85);
  }

  .v-ghost {
    fill: var(--ink3);
    fill-opacity: 0.08;
    stroke: var(--ink3);
    stroke-width: 1.2;
    stroke-dasharray: 4 3;
    vector-effect: non-scaling-stroke;
    opacity: 0.55;
    pointer-events: none;
  }

  .v-path {
    fill: currentColor;
    fill-opacity: 0.22;
    stroke: var(--ink);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }

  .v-pen-preview {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 5 3;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }

  .v-pen-start {
    fill: var(--accent-2);
    stroke: var(--win);
    stroke-width: 1.4;
    vector-effect: non-scaling-stroke;
    cursor: pointer;
  }

  .v-handle-line {
    stroke: var(--accent-2);
    opacity: 0.75;
    pointer-events: none;
  }

  .v-handle-dot {
    fill: var(--accent-2);
  }

  .v-handle-dot.interactive {
    cursor: grab;
  }

  .v-node-smooth {
    fill: var(--accent);
    stroke: var(--win);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }

  .v-node-corner {
    fill: #fbbf24;
    stroke: var(--win);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }

  .v-node-smooth.interactive,
  .v-node-corner.interactive {
    cursor: move;
  }

  .v-node-smooth.selected,
  .v-node-corner.selected {
    stroke: var(--ink);
    stroke-width: 2;
    filter: drop-shadow(0 0 3px var(--accent));
  }

  .v-shape-preview {
    fill: var(--accent-soft);
    stroke: var(--accent);
    stroke-width: 1.4;
    stroke-dasharray: 5 3;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }

  .v-knife-preview {
    stroke: var(--danger);
    stroke-dasharray: 6 4;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }

  .v-caliper-group {
    pointer-events: none;
  }

  .v-caliper-line {
    stroke: var(--ok);
    stroke-dasharray: 4 3;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }

  .v-caliper-dot {
    fill: var(--ok);
    stroke: var(--win);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }

  .v-caliper-pill {
    fill: var(--elevated, var(--field));
    stroke: var(--ok);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }

  .v-caliper-text {
    fill: var(--ok);
    font-family: var(--mono);
    font-weight: 700;
    text-anchor: middle;
    dominant-baseline: central;
    pointer-events: none;
  }
</style>
