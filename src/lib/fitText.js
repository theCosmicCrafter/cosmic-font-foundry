const ctx = document.createElement("canvas").getContext("2d");
const REF = 100;

function measure(text, font) {
  ctx.font = font;
  const m = ctx.measureText(text);
  return {
    width: m.width,
    left: m.actualBoundingBoxLeft,
    right: m.actualBoundingBoxRight,
    ascent: m.actualBoundingBoxAscent,
    descent: m.actualBoundingBoxDescent,
    fAscent: m.fontBoundingBoxAscent,
    fDescent: m.fontBoundingBoxDescent,
  };
}

export function fitText(node, options) {
  let opts = options;
  let alive = true;

  async function apply() {
    const { text, family, weight = 400, italic = false, base = 64, margin = 14 } = opts;
    const box = node.parentElement;
    if (!box) return;
    const stack = family.replace(/var\([^)]*\)/g, "sans-serif");
    const font = `${italic ? "italic " : ""}${weight} ${REF}px ${stack}`;
    try {
      await document.fonts.load(font, text);
    } catch {}
    if (!alive) return;
    const m = measure(text, font);
    const w = box.clientWidth - margin * 2;
    const h = box.clientHeight - margin * 2;
    const inkW = m.left + m.right;
    const inkH = m.ascent + m.descent;
    if (!(w > 0 && h > 0 && inkW > 0 && inkH > 0)) return;

    const s = Math.min(base, (w / inkW) * REF, (h / inkH) * REF);
    const k = s / REF;
    const dx = (m.width / 2 - (m.right - m.left) / 2) * k;
    const dy = (-(m.fAscent - m.fDescent) / 2 - (m.descent - m.ascent) / 2) * k;
    node.style.fontSize = s.toFixed(1) + "px";
    node.style.transform = `translate(${dx.toFixed(1)}px, ${dy.toFixed(1)}px)`;
  }

  const onDone = () => apply();
  document.fonts.addEventListener("loadingdone", onDone);
  apply();

  return {
    update(next) {
      opts = next;
      apply();
    },
    destroy() {
      alive = false;
      document.fonts.removeEventListener("loadingdone", onDone);
    },
  };
}
