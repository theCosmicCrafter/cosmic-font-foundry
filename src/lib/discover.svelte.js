import { inTauri } from "./settings.svelte.js";
import { notify } from "./toasts.svelte.js";
import { t } from "./i18n.svelte.js";

export const discover = $state({
  list: [],
  loading: false,
  error: "",
  downloads: {},
});

const ADOBE_DEV_SET = new Set([
  "source sans 3",
  "source sans pro",
  "source serif 4",
  "source serif pro",
  "source code pro",
  "monaspace neon",
  "monaspace argon",
  "monaspace krypton",
  "monaspace xenon",
  "monaspace radon",
  "iosevka",
  "iosevka aile",
  "iosevka etoile",
  "iosevka curly",
  "iosevka curly slab",
  "iosevka charon",
  "iosevka charon mono",
  "commit mono",
  "geist",
  "geist mono",
  "geist pixel",
  "geist sans",
  "cascadia code",
  "cascadia mono",
  "fira code",
  "fira mono",
  "jetbrains mono",
  "victor mono",
  "hack",
  "league mono",
]);

const isAdobeOrDev = (name) => ADOBE_DEV_SET.has(String(name || "").trim().toLowerCase());

const WEB_GOOGLE_CURATED = [
  ["Space Grotesk", "sans", 5, true, [{ tag: "wght", min: 300, max: 700, default: 400 }], ["Florian Karsten"]],
  ["Inter", "sans", 18, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Rasmus Andersson"]],
  ["Source Sans 3", "sans", 12, true, [{ tag: "wght", min: 200, max: 900, default: 400 }], ["Adobe"]],
  ["Source Serif 4", "serif", 12, true, [{ tag: "wght", min: 200, max: 900, default: 400 }], ["Adobe"]],
  ["Source Code Pro", "mono", 14, true, [{ tag: "wght", min: 200, max: 900, default: 400 }], ["Adobe"]],
  ["JetBrains Mono", "mono", 16, true, [{ tag: "wght", min: 100, max: 800, default: 400 }], ["JetBrains"]],
  ["Fira Code", "mono", 5, true, [{ tag: "wght", min: 300, max: 700, default: 400 }], ["Nikita Prokopov"]],
  ["Geist", "sans", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Vercel"]],
  ["Geist Mono", "mono", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Vercel"]],
  ["Syne", "display", 5, true, [{ tag: "wght", min: 400, max: 800, default: 400 }], ["Bonjour Monde"]],
  ["Outfit", "sans", 9, true, [{ tag: "wght", min: 100, max: 900, default: 400 }], ["Rodrigo Fuenzalida"]],
  ["Orbitron", "display", 6, true, [{ tag: "wght", min: 400, max: 900, default: 400 }], ["Matt McInerney"]],
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
  source: isAdobeOrDev(name) ? "adobe_dev" : "google",
  source_id: name,
  preview_url: null,
  download_urls: [],
}));

function mapCategory(raw = "") {
  const s = String(raw).toLowerCase();
  if (s.includes("sans")) return "sans";
  if (s.includes("serif") || s.includes("slab")) return "serif";
  if (s.includes("hand") || s.includes("script") || s.includes("calligraph")) return "script";
  if (s.includes("mono") || s.includes("typewriter")) return "mono";
  return "display";
}

function normalizeUrl(raw) {
  return raw?.startsWith("//") ? `https:${raw}` : raw;
}

async function fetchWebCatalog() {
  const [fsRes, srcRes, sqRes] = await Promise.allSettled([
    fetch("https://api.fontshare.com/v2/fonts?offset=0&limit=100").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://api.fontsource.org/v1/fonts").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://www.fontsquirrel.com/api/fontlist/all").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
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
      ? srcRes.value.map((f, idx) => {
          const weights = f.weights?.length ? f.weights : [400];
          const styles = f.styles?.length ? f.styles : ["normal"];
          const defSubset = f.defSubset || "latin";
          const defW = weights.includes(400) ? 400 : weights[0];
          const defSt = styles.includes("normal") ? "normal" : styles[0];
          const isGoogleType = f.type === "google";
          const source = isAdobeOrDev(f.family) ? "adobe_dev" : isGoogleType ? "google" : "fontsource";
          return {
            name: f.family,
            category: mapCategory(f.category),
            styles: Math.max(1, weights.length * styles.length),
            popularity: source === "adobe_dev" ? 10 + idx : isGoogleType ? 30 + idx : 50 + idx,
            date: f.lastModified ?? "",
            scripts: ["Latin"],
            designers: [],
            is_variable: Boolean(f.variable),
            axes: [],
            variants: weights.map(String),
            source,
            source_id: isGoogleType ? f.family : f.id,
            preview_url: isGoogleType
              ? null
              : `https://cdn.jsdelivr.net/fontsource/fonts/${f.id}@latest/${defSubset}-${defW}-${defSt}.woff2`,
            download_urls: weights.map(
              (w) => `https://cdn.jsdelivr.net/fontsource/fonts/${f.id}@latest/${defSubset}-${w}-${defSt}.ttf`,
            ),
          };
        })
      : WEB_GOOGLE_CURATED;

  const fontsquirrelEntries =
    sqRes.status === "fulfilled" && Array.isArray(sqRes.value)
      ? sqRes.value
          .filter((f) => f.family_name && f.family_urlname)
          .map((f, idx) => ({
            name: f.family_name.trim(),
            category: mapCategory(f.classification),
            styles: Math.max(1, Number.parseInt(f.family_count, 10) || 1),
            popularity: 120 + idx,
            date: "",
            scripts: ["Latin"],
            designers: f.foundry_name ? [f.foundry_name] : [],
            is_variable: false,
            axes: [],
            variants: ["400"],
            source: isAdobeOrDev(f.family_name) ? "adobe_dev" : "fontsquirrel",
            source_id: f.family_urlname.trim(),
            preview_url: null,
            download_urls: [`https://www.fontsquirrel.com/fonts/download/${f.family_urlname.trim()}`],
          }))
      : [];

  const seen = new Set();
  const combined = [];
  for (const entry of [...fontshareEntries, ...fontsourceEntries, ...fontsquirrelEntries]) {
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
      if (item.source === "fontsquirrel") continue;
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
  if (entry.source === "fontsquirrel") return;
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
