export const drag = $state({ active: false, x: 0, y: 0, names: [], over: null });

export function startDrag(e, getNames, onDrop) {
  if (e.button !== 0 || e.target.closest("button, input")) return;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let active = false;

  const onMove = (ev) => {
    if (!active && Math.hypot(ev.clientX - x0, ev.clientY - y0) > 6) {
      active = true;
      drag.names = getNames();
      drag.active = true;
      document.body.style.cursor = "grabbing";
    }
    if (!active) return;
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest("[data-collection]");
    drag.over = el?.dataset.collection ?? null;
  };

  const onEnd = () => {
    removeEventListener("pointermove", onMove);
    removeEventListener("pointerup", onEnd);
    removeEventListener("pointercancel", onEnd);
    if (!active) return;
    document.body.style.cursor = "";
    const id = drag.over;
    const names = drag.names;
    drag.active = false;
    drag.over = null;
    if (id) onDrop(id, names);
    const stopClick = (ev) => ev.stopPropagation();
    addEventListener("click", stopClick, { capture: true, once: true });
    setTimeout(() => removeEventListener("click", stopClick, { capture: true }), 0);
  };

  addEventListener("pointermove", onMove);
  addEventListener("pointerup", onEnd);
  addEventListener("pointercancel", onEnd);
}
