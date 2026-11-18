import { inTauri } from "./settings.svelte.js";
import { notify } from "./toasts.svelte.js";
import { t } from "./i18n.svelte.js";

export const discover = $state({
  list: [],
  loading: false,
  error: "",
  downloads: {},
});

const WEB_GOOGLE_CURATED = [
  ["Space Grotesk", "sans", 5, true, [{ tag: "wght", min: 300, max: 700, default: 400 }], ["Google Font Foundry"]],
  ["Inter", "sans", 18, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Rasmus Andersson"]],
  ["Syne", "display", 5, true, [{ tag: "wght", min: 400, max: 800, default: 400 }], ["Bonjour Monde"]],
  ["Plus Jakarta Sans", "sans", 7, true, [{ tag: "wght", min: 200, max: 800, default: 400 }], ["Tokotype"]],
  ["Outfit", "sans", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Rodrigo Fuenzalida"]],
  ["Orbitron", "display", 6, true, [{ tag: "wght", min: 400, max: 900, default: 400 }], ["Matt McInerney"]],
  ["Fraunces", "serif", 8, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Undercase Type"]],
  ["Instrument Serif", "serif", 2, false, [], ["Rodrigo Fuenzalida"]],
  ["Playfair Display", "serif", 6, true, [{ tag: "wght", min: 400, max: 900, default: 400 }], ["Claus Eggers Sørensen"]],
  ["Cormorant Garamond", "serif", 10, false, [], ["Christian Thalmann"]],
  ["Fira Code", "mono", 5, true, [{ tag: "wght", min: 300, max: 700, default: 400 }], ["Nikita Prokopov"]],
  ["IBM Plex Mono", "mono", 14, false, [], ["Mike Abbink"]],
  ["Space Mono", "mono", 4, false, [], ["Colophon Foundry"]],
  ["Manrope", "sans", 7, true, [{ tag: "wght", min: 200, max: 800, default: 400 }], ["Mikhail Sharanda"]],
  ["DM Sans", "sans", 12, true, [{ tag: "wght", min: 100, max: 1000, default: 400 }], ["Colophon Foundry"]],
  ["Bricolage Grotesque", "sans", 7, true, [{ tag: "wght", min: 200, max: 800, default: 400 }], ["Mathieu Triay"]],
  ["Unbounded", "display", 8, true, [{ tag: "wght", min: 200, max: 900, default: 400 }], ["Studio Koto"]],
  ["Caveat", "script", 4, true, [{ tag: "wght", min: 400, max: 700, default: 400 }], ["Impallari Type"]],
  ["Pacifico", "script", 1, false, [], ["Vernon Adams"]],
  ["Dancing Script", "script", 4, true, [{ tag: "wght", min: 400, max: 700, default: 400 }], ["Impallari Type"]],
  ["Sora", "sans", 8, true, [{ tag: "wght", min: 100, max: 800, default: 400 }], ["Jonathan Barnbrook"]],
  ["Urbanist", "sans", 18, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Corey Hu"]],
  ["Newsreader", "serif", 12, true, [{ tag: "wght", min: 200, max: 800, default: 400 }], ["Production Type"]],
  ["Geist", "sans", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Vercel"]],
  ["Geist Mono", "mono", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Vercel"]],
  ["Oxanium", "display", 7, false, [], ["Severin Meyer"]],
  ["Audiowide", "display", 1, false, [], ["Brian J. Bonislawsky"]],
  ["Chakra Petch", "sans", 10, false, [], ["Cadson Demak"]],
  ["Silkscreen", "display", 2, false, [], ["Jason Kottke"]],
  ["Bodoni Moda", "serif", 12, true, [{ tag: "wght", min: 400, max: 900, default: 400 }], ["Indestructible Type"]],
].map(([name, category, styles, is_variable, axes, designers], idx) => ({
  name,
  category,
  styles,
  popularity: idx + 1,
  date: "2024-01-01",
  scripts: ["Latin", "Latin Ext"],
  designers,
  is_variable,
  axes,
  variants: ["400", "700"],
  source: "google",
  source_id: name,
  preview_url: null,
  download_urls: [],
}));

function mapCategory(raw = "") {
  const s = String(raw).toLowerCase();
  if (s === "sans" || s === "sans-serif") return "sans";
  if (s === "serif" || s === "slab") return "serif";
  if (s === "handwritten" || s === "handwriting" || s === "script") return "script";
  if (s === "mono" || s === "monospace") return "mono";
  return "display";
}

function normalizeUrl(raw) {
  return raw?.startsWith("//") ? `https:${raw}` : raw;
}

async function fetchWebCatalog() {
  const [fsRes, srcRes] = await Promise.allSettled([
    fetch("https://api.fontshare.com/v2/fonts?offset=0&limit=100").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://api.fontsource.org/v1/fonts?type=other").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
  ]);

  const fontshareEntries =
    fsRes.status === "fulfilled" && Array.isArray(fsRes.value?.fonts)
      ? [...fsRes.value.fonts]
          .sort((a, b) => (b.views ?? 0) - (a.views ?? 0))
          .map((f, idx) => {
            const styles = Array.isArray(f.styles) ? f.styles : [];
            const def = styles.find((s) => s.default) ?? styles[0];
            const axes = (f.axes ?? []).map((a) => ({
              tag: a.property || a.name,
              min: a.range_left ?? 100,
              max: a.range_right ?? 900,
              default: a.range_default ?? 400,
            }));
            return {
              name: f.name,
              category: mapCategory(f.category),
              styles: styles.length || 1,
              popularity: idx + 1,
              date: "",
              scripts: [f.script === "devanagari" ? "Devanagari" : "Latin"],
              designers: (f.designers ?? []).map((d) => d.name).filter(Boolean),
              is_variable: axes.length > 0,
              axes,
              variants: styles.map((s) => String(s.weight?.number ?? 400)),
              source: "fontshare",
              source_id: f.slug,
              preview_url: def?.file ? `${normalizeUrl(def.file)}.woff2` : null,
              download_urls: styles.map((s) => (s.file ? `${normalizeUrl(s.file)}.ttf` : null)).filter(Boolean),
            };
          })
      : [];

  const fontsourceEntries =
    srcRes.status === "fulfilled" && Array.isArray(srcRes.value)
      ? srcRes.value
          .filter((f) => f.type === "other")
          .map((f, idx) => {
            const weights = f.weights?.length ? f.weights : [400];
            const styles = f.styles?.length ? f.styles : ["normal"];
            const defSubset = f.defSubset || "latin";
            const defW = weights.includes(400) ? 400 : weights[0];
            const defSt = styles.includes("normal") ? "normal" : styles[0];
            return {
              name: f.family,
              category: mapCategory(f.category),
              styles: Math.max(1, weights.length * styles.length),
              popularity: 50 + idx,
              date: f.lastModified ?? "",
              scripts: ["Latin"],
              designers: [],
              is_variable: Boolean(f.variable),
              axes: [],
              variants: weights.map(String),
              source: "fontsource",
              source_id: f.id,
              preview_url: `https://cdn.jsdelivr.net/fontsource/fonts/${f.id}@latest/${defSubset}-${defW}-${defSt}.woff2`,
              download_urls: weights.map(
                (w) => `https://cdn.jsdelivr.net/fontsource/fonts/${f.id}@latest/${defSubset}-${w}-${defSt}.ttf`,
              ),
            };
          })
      : [];

  const seen = new Set();
  const combined = [];
  for (const entry of [...WEB_GOOGLE_CURATED, ...fontshareEntries, ...fontsourceEntries]) {
    const key = entry.name.toLowerCase();
    if (!seen.has(key)) {
      seen.add(key);
      combined.push(entry);
    }
  }
  return combined;
}

let pendingCatalog = null;
export function loadCatalog(force = false) {
  if (pendingCatalog && !force) return pendingCatalog;
  discover.loading = true;
  discover.error = "";
  const req = inTauri
    ? import("@tauri-apps/api/core").then(({ invoke }) => invoke("google_catalog", { force }))
    : fetchWebCatalog();
  pendingCatalog = req
    .then((list) => {
      discover.list = list;
    })
    .catch((e) => {
      discover.error = String(e);
      pendingCatalog = null;
    })
    .finally(() => (discover.loading = false));
  return pendingCatalog;
}

const injectedFaces = new Set();
let faceSheet = null;
function getFaceSheet() {
  if (!faceSheet) {
    const el = document.createElement("style");
    el.dataset.cosmicDiscover = "1";
    document.head.appendChild(el);
    faceSheet = el.sheet;
  }
  return faceSheet;
}

function ensureDirectFace(entry) {
  if (!entry?.preview_url || injectedFaces.has(entry.name)) return;
  injectedFaces.add(entry.name);
  const safeName = String(entry.name).replace(/["\\]/g, "");
  try {
    getFaceSheet().insertRule(
      `@font-face{font-family:"${safeName}";src:url("${entry.preview_url}") format("woff2");font-display:swap;}`,
      getFaceSheet().cssRules.length,
    );
  } catch {}
}

const requestedChars = new Map();
const BATCH_SIZE = 15;

export function requestGooglePreviews(items, text) {
  const chars = new Set([...(text + "Aa")].filter((c) => c.trim()));
  const googleNames = [];
  for (const item of items) {
    if (item && typeof item === "object") {
      if (item.preview_url) {
        ensureDirectFace(item);
        continue;
      }
      const fam = item.name;
      const prev = requestedChars.get(fam) ?? new Set();
      if ([...chars].some((c) => !prev.has(c))) googleNames.push(fam);
    } else if (typeof item === "string") {
      const prev = requestedChars.get(item) ?? new Set();
      if ([...chars].some((c) => !prev.has(c))) googleNames.push(item);
    }
  }

  for (let i = 0; i < googleNames.length; i += BATCH_SIZE) {
    const batch = googleNames.slice(i, i + BATCH_SIZE);
    const merged = new Set(chars);
    for (const fam of batch) for (const c of requestedChars.get(fam) ?? []) merged.add(c);
    for (const fam of batch) requestedChars.set(fam, new Set(merged));
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href =
      "https://fonts.googleapis.com/css2?" +
      batch.map((f) => "family=" + encodeURIComponent(f).replace(/%20/g, "+")).join("&") +
      "&display=swap&text=" +
      encodeURIComponent([...merged].join(""));
    document.head.appendChild(link);
  }
}

const loadedFull = new Set();
/** Loads the full font stylesheet (including variable axis ranges) for Discover Specimen view. */
export function loadGoogleFullSpecimen(entry) {
  if (!entry || loadedFull.has(entry.name)) return;
  loadedFull.add(entry.name);
  if (entry.source === "fontshare" && entry.source_id) {
    ensureDirectFace(entry);
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = `https://api.fontshare.com/css?f[]=${encodeURIComponent(entry.source_id)}@300,400,500,600,700&display=swap`;
    document.head.appendChild(link);
    return;
  }
  if (entry.preview_url) {
    ensureDirectFace(entry);
    return;
  }
  let familyQuery = encodeURIComponent(entry.name).replace(/%20/g, "+");
  if (entry.is_variable && entry.axes?.length) {
    const sorted = [...entry.axes].sort((a, b) => a.tag.localeCompare(b.tag));
    const tags = sorted.map((a) => a.tag).join(",");
    const range = sorted.map((a) => `${Math.round(a.min)}..${Math.round(a.max)}`).join(",");
    familyQuery += `:${tags}@${range}`;
  } else {
    familyQuery += `:wght@300;400;500;600;700`;
  }
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `https://fonts.googleapis.com/css2?family=${familyQuery}&display=swap`;
  document.head.appendChild(link);
}

export function preloadDiscover(text) {
  const idle = window.requestIdleCallback ?? ((fn) => setTimeout(fn, 1200));
  idle(() =>
    loadCatalog().then(() => {
      const top = [...discover.list].sort((a, b) => a.popularity - b.popularity).slice(0, 45);
      requestGooglePreviews(top, text);
    }),
  );
}

let listening = false;
export async function installGoogleFont(family, onInstalled) {
  if (!inTauri || discover.downloads[family] != null) return;
  const { invoke } = await import("@tauri-apps/api/core");
  if (!listening) {
    listening = true;
    const { listen } = await import("@tauri-apps/api/event");
    listen("download", ({ payload }) => {
      if (payload.error || payload.pct >= 100) return;
      if (discover.downloads[payload.family] != null) {
        discover.downloads[payload.family] = payload.pct;
      }
    });
  }
  discover.downloads[family] = 1;
  try {
    const res = await invoke("google_install", { family });
    onInstalled(res);
    if (res.outcome.errors.length) notify("err", t("err.install"), res.outcome.errors[0]);
    else notify("ok", t("ok.installed", { n: family }), t("ok.availableNow"));
  } catch (e) {
    notify("err", t("err.download", { n: family }), String(e));
  } finally {
    delete discover.downloads[family];
  }
}
