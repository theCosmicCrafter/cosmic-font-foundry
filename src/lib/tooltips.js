const DELAY = 420;
const MARGIN = 8;

let box = null;
let target = null;
let timer = 0;

function ensureBox() {
  box = document.createElement("div");
  box.className = "tooltip";
  box.setAttribute("role", "tooltip");
  document.body.appendChild(box);
}

function readText(el) {
  const title = el.getAttribute("title");
  if (title) {
    el.dataset.tooltip = title;
    el.removeAttribute("title");
    if (!el.hasAttribute("aria-label") && !el.textContent.trim()) {
      el.setAttribute("aria-label", title);
    }
  }
  return el.dataset.tooltip ?? "";
}

function positionBox(el) {
  const rect = el.getBoundingClientRect();
  const boxRect = box.getBoundingClientRect();
  let x = rect.left + rect.width / 2 - boxRect.width / 2;
  x = Math.max(MARGIN, Math.min(x, innerWidth - boxRect.width - MARGIN));
  let y = rect.bottom + 6;
  let above = false;
  if (y + boxRect.height > innerHeight - MARGIN) {
    y = rect.top - boxRect.height - 6;
    above = true;
  }
  box.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
  box.classList.toggle("above", above);
}

function showTooltip(el) {
  const text = readText(el);
  if (!text) return;
  if (!box) ensureBox();
  box.textContent = text;
  box.classList.remove("visible");
  positionBox(el);
  requestAnimationFrame(() => box.classList.add("visible"));
}

function hideTooltip() {
  clearTimeout(timer);
  target = null;
  box?.classList.remove("visible");
}

export function initTooltips() {
  document.addEventListener("pointerover", (e) => {
    const el = e.target.closest?.("[title], [data-tooltip]");
    if (el === target) return;
    hideTooltip();
    if (!el) return;
    target = el;
    readText(el);
    timer = setTimeout(() => target === el && el.isConnected && showTooltip(el), DELAY);
  });
  document.addEventListener("pointerout", (e) => {
    if (target && !target.contains(e.relatedTarget)) hideTooltip();
  });
  for (const ev of ["pointerdown", "keydown", "wheel", "scroll"]) {
    document.addEventListener(ev, hideTooltip, true);
  }
  addEventListener("blur", hideTooltip);
}
