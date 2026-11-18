import { inTauri } from "./settings.svelte.js";
import { notify } from "./toasts.svelte.js";
import { t } from "./i18n.svelte.js";

export const discover = $state({
  list: [],
  customSources: [],
  loading: false,
  error: "",
  downloads: {},
});

export const COMMUNITY_PRESETS = [
  {
    key: "velvetyne",
    url: "preset:velvetyne",
    name: "Velvetyne Libre Foundry",
    kind: "preset",
    descKey: "cs.presetVelvetyne",
  },
  {
    key: "leagueof",
    url: "preset:leagueof",
    name: "League of Moveable Type",
    kind: "preset",
    descKey: "cs.presetLeague",
  },
  {
    key: "indie_mono",
    url: "preset:indie_mono",
    name: "Indie Coding & Terminal",
    kind: "preset",
    descKey: "cs.presetIndie",
  },
];

const CUSTOM_SOURCES_STORAGE_KEY = "cosmic.customSources.v1";
const CUSTOM_CACHE_STORAGE_KEY = "cosmic.customCache.v1";

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

let baseCatalogCache = null;
let pendingCatalog = null;

function mergeWithWebCustomEntries(baseList) {
  const customEntries = getEnabledWebCustomEntries();
  return [...baseList, ...customEntries];
}

export function loadCatalog(force = false) {
  if (pendingCatalog && !force) return pendingCatalog;
  discover.loading = true;
  discover.error = "";
  loadCustomSources();
  const req = inTauri
    ? import("@tauri-apps/api/core").then(({ invoke }) => invoke("google_catalog", { force }))
    : (baseCatalogCache && !force ? Promise.resolve(baseCatalogCache) : fetchWebCatalog()).then((list) => {
        baseCatalogCache = list;
        return mergeWithWebCustomEntries(list);
      });
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

function reloadMergedCatalog() {
  pendingCatalog = null;
  return loadCatalog(false);
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
      if (item.source === "fontsquirrel" || item.source?.startsWith("custom:")) continue;
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
  if (entry.source === "fontsquirrel" || entry.source?.startsWith("custom:")) return;
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

// ---- Custom Font Repositories (RSS / Atom / JSON / GitHub / Presets) ----

function readLocalCustomSources() {
  try {
    return JSON.parse(localStorage.getItem(CUSTOM_SOURCES_STORAGE_KEY) || "[]");
  } catch {
    return [];
  }
}

function writeLocalCustomSources(list) {
  try {
    localStorage.setItem(CUSTOM_SOURCES_STORAGE_KEY, JSON.stringify(list));
  } catch {}
}

function readLocalCustomCache() {
  try {
    return JSON.parse(localStorage.getItem(CUSTOM_CACHE_STORAGE_KEY) || "{}");
  } catch {
    return {};
  }
}

function writeLocalCustomCache(cache) {
  try {
    localStorage.setItem(CUSTOM_CACHE_STORAGE_KEY, JSON.stringify(cache));
  } catch {}
}

function getEnabledWebCustomEntries() {
  const sources = readLocalCustomSources();
  const cache = readLocalCustomCache();
  const out = [];
  for (const s of sources) {
    if (s.enabled && Array.isArray(cache[s.id])) {
      out.push(...cache[s.id]);
    }
  }
  return out;
}

function webSlugId(name, url) {
  const stem = String(name || "feed")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "feed";
  let h = 2166136261;
  for (let i = 0; i < url.length; i++) {
    h ^= url.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return `${stem}-${(h >>> 0).toString(16).slice(0, 6)}`;
}

function webNormalizeFeedUrl(rawUrl, kind) {
  const trimmed = String(rawUrl || "").trim();
  if (!trimmed) throw new Error("Feed or repository URL cannot be empty.");
  if (trimmed.startsWith("preset:")) return trimmed;
  const parsed = new URL(trimmed);
  if (!["http:", "https:"].includes(parsed.protocol)) {
    throw new Error("Only HTTP/HTTPS URLs are supported.");
  }
  const host = parsed.hostname.toLowerCase();
  if (host === "localhost" || host === "127.0.0.1" || host === "::1" || host.endsWith(".local")) {
    throw new Error("Only public URLs are permitted for custom libraries.");
  }
  if (host === "github.com" && (kind === "auto" || kind === "github")) {
    const segs = parsed.pathname.split("/").filter(Boolean);
    if ((segs.length === 2 || (segs.length === 3 && segs[2] === "releases")) && !trimmed.endsWith(".atom") && !trimmed.endsWith(".xml")) {
      return `https://api.github.com/repos/${segs[0]}/${segs[1]}/releases?per_page=15`;
    }
  }
  return parsed.toString();
}

function webUrlHasExt(raw, ext) {
  return String(raw || "").split(/[?#]/)[0].toLowerCase().endsWith(ext);
}

function webPresetEntries(presetKey, sourceIdKey, sourceName) {
  const srcTag = `custom:${sourceIdKey}`;
  const table = {
    velvetyne: [
      ["Syne", "display", "Bonjour Monde", 5, true, "https://cdn.jsdelivr.net/fontsource/fonts/syne@latest/latin-700-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/syne@latest/latin-700-normal.ttf"],
      ["Silkscreen", "display", "Jason Kottke / Velvetyne", 2, false, "https://cdn.jsdelivr.net/fontsource/fonts/silkscreen@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/silkscreen@latest/latin-400-normal.ttf"],
      ["Unbounded", "display", "Studio Koto", 8, true, "https://cdn.jsdelivr.net/fontsource/fonts/unbounded@latest/latin-600-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/unbounded@latest/latin-600-normal.ttf"],
      ["Bricolage Grotesque", "sans", "Mathieu Triay", 8, true, "https://cdn.jsdelivr.net/fontsource/fonts/bricolage-grotesque@latest/latin-500-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/bricolage-grotesque@latest/latin-500-normal.ttf"],
    ],
    leagueof: [
      ["League Spartan", "sans", "Matt Bailey & Tyler Finck", 9, true, "https://cdn.jsdelivr.net/fontsource/fonts/league-spartan@latest/latin-600-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/league-spartan@latest/latin-600-normal.ttf"],
      ["Orbitron", "display", "Matt McInerney", 6, true, "https://cdn.jsdelivr.net/fontsource/fonts/orbitron@latest/latin-600-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/orbitron@latest/latin-600-normal.ttf"],
      ["League Gothic", "display", "Micah Rich & Caroline Hadilaksono", 2, true, "https://cdn.jsdelivr.net/fontsource/fonts/league-gothic@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/league-gothic@latest/latin-400-normal.ttf"],
      ["League Mono", "mono", "Tyler Finck", 8, true, "https://cdn.jsdelivr.net/fontsource/fonts/league-mono@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/league-mono@latest/latin-400-normal.ttf"],
    ],
    indie_mono: [
      ["Fira Code", "mono", "Nikita Prokopov", 5, true, "https://cdn.jsdelivr.net/fontsource/fonts/fira-code@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/fira-code@latest/latin-400-normal.ttf"],
      ["JetBrains Mono", "mono", "JetBrains", 16, true, "https://cdn.jsdelivr.net/fontsource/fonts/jetbrains-mono@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/jetbrains-mono@latest/latin-400-normal.ttf"],
      ["Victor Mono", "mono", "Rune B", 14, false, "https://cdn.jsdelivr.net/fontsource/fonts/victor-mono@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/victor-mono@latest/latin-400-normal.ttf"],
      ["Space Mono", "mono", "Colophon Foundry", 4, false, "https://cdn.jsdelivr.net/fontsource/fonts/space-mono@latest/latin-400-normal.woff2", "https://cdn.jsdelivr.net/fontsource/fonts/space-mono@latest/latin-400-normal.ttf"],
    ],
  };
  return (table[presetKey] || []).map(([name, category, designer, styles, is_variable, woff2, ttf], idx) => ({
    name,
    category,
    styles,
    popularity: idx + 1,
    date: "2026-01-01",
    scripts: ["Latin"],
    designers: [designer],
    is_variable,
    axes: [],
    variants: ["400"],
    source: srcTag,
    source_id: name,
    source_label: sourceName,
    preview_url: woff2,
    download_urls: [ttf],
  }));
}

function webParseXmlFeed(xmlText, srcTag, sourceName) {
  const doc = new DOMParser().parseFromString(xmlText, "text/xml");
  const nodes = [...doc.querySelectorAll("item, entry")];
  const out = [];
  const seen = new Set();
  for (let i = 0; i < nodes.length; i++) {
    const el = nodes[i];
    const rawTitle = el.querySelector("title")?.textContent?.trim() || "";
    const looksTag = /^v?\d[\d.]*$/i.test(rawTitle);
    const name = !rawTitle ? sourceName : looksTag ? `${sourceName} (${rawTitle})` : rawTitle;
    if (!name || seen.has(name.toLowerCase())) continue;
    seen.add(name.toLowerCase());
    const catHint = el.querySelector("category")?.textContent || el.querySelector("category")?.getAttribute("term") || "";
    const rawHtml = el.innerHTML || el.textContent || "";
    const urls = new Set();
    for (const u of rawHtml.match(/https?:\/\/[^\s"'<>()]+/g) || []) {
      if (webUrlHasExt(u, ".ttf") || webUrlHasExt(u, ".otf") || webUrlHasExt(u, ".woff2") || webUrlHasExt(u, ".zip")) {
        urls.add(u);
      }
    }
    const enc = el.querySelector("enclosure")?.getAttribute("url");
    if (enc) urls.add(enc);
    const arr = [...urls];
    out.push({
      name,
      category: mapCategory(catHint || name),
      styles: Math.max(1, arr.length),
      popularity: i + 1,
      date: el.querySelector("pubDate, published, updated")?.textContent?.trim() || "",
      scripts: ["Latin"],
      designers: [],
      is_variable: name.toLowerCase().includes("variable"),
      axes: [],
      variants: ["400"],
      source: srcTag,
      source_id: name,
      source_label: sourceName,
      preview_url: arr.find((u) => webUrlHasExt(u, ".woff2")) || null,
      download_urls: arr.filter((u) => webUrlHasExt(u, ".ttf") || webUrlHasExt(u, ".otf") || webUrlHasExt(u, ".zip")),
    });
  }
  return out;
}

function webParseJsonFeed(val, srcTag, sourceName) {
  const items = Array.isArray(val) ? val : val?.fonts || val?.families || val?.items || [];
  const out = [];
  const seen = new Set();
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (Array.isArray(it?.assets)) {
      const urls = it.assets.map((a) => a?.browser_download_url).filter((u) => u && (webUrlHasExt(u, ".ttf") || webUrlHasExt(u, ".otf") || webUrlHasExt(u, ".woff2") || webUrlHasExt(u, ".zip")));
      if (!urls.length) continue;
      const rawTitle = (it.name || it.tag_name || sourceName).trim();
      const name = /^v?\d[\d.]*$/i.test(rawTitle) ? `${sourceName} (${rawTitle})` : rawTitle;
      if (!seen.has(name.toLowerCase())) {
        seen.add(name.toLowerCase());
        out.push({
          name,
          category: mapCategory(name),
          styles: urls.length,
          popularity: i + 1,
          date: it.published_at || "",
          scripts: ["Latin"],
          designers: it.author?.login ? [it.author.login] : [],
          is_variable: name.toLowerCase().includes("variable"),
          axes: [],
          variants: ["400"],
          source: srcTag,
          source_id: name,
          source_label: sourceName,
          preview_url: urls.find((u) => webUrlHasExt(u, ".woff2")) || null,
          download_urls: urls.filter((u) => !webUrlHasExt(u, ".woff2")),
        });
      }
      continue;
    }
    const name = String(it?.name || it?.family || it?.title || "").trim();
    if (!name || seen.has(name.toLowerCase())) continue;
    seen.add(name.toLowerCase());
    const rawUrls = [...(it.download_urls || it.files || []), ...(it.attachments || []).map((a) => a.url), it.download_url, it.url].filter(Boolean);
    out.push({
      name,
      category: mapCategory(it.category || it.tags?.[0] || name),
      styles: it.styles || Math.max(1, rawUrls.length),
      popularity: i + 1,
      date: it.date || it.date_published || "",
      scripts: ["Latin"],
      designers: it.designers || (it.author ? [it.author] : []),
      is_variable: Boolean(it.is_variable),
      axes: [],
      variants: ["400"],
      source: srcTag,
      source_id: name,
      source_label: sourceName,
      preview_url: it.preview_url || rawUrls.find((u) => webUrlHasExt(u, ".woff2")) || null,
      download_urls: rawUrls.filter((u) => webUrlHasExt(u, ".ttf") || webUrlHasExt(u, ".otf") || webUrlHasExt(u, ".zip")),
    });
  }
  return out;
}

async function fetchWebCustomSourceEntries(id, name, url, kind) {
  if (url.startsWith("preset:")) {
    return webPresetEntries(url.slice(7), id, name);
  }
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`HTTP ${resp.status} from ${url}`);
  const text = await resp.text();
  const srcTag = `custom:${id}`;
  if (kind === "rss" || (text.trimStart().startsWith("<") && (text.includes("<rss") || text.includes("<feed") || text.includes("<item") || text.includes("<entry")))) {
    const entries = webParseXmlFeed(text, srcTag, name);
    if (!entries.length) throw new Error("Feed parsed cleanly, but contained no recognizable font entries.");
    return entries;
  }
  const entries = webParseJsonFeed(JSON.parse(text), srcTag, name);
  if (!entries.length) throw new Error("Feed parsed cleanly, but contained no recognizable font entries.");
  return entries;
}

export async function loadCustomSources() {
  if (inTauri) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      discover.customSources = await invoke("custom_sources_list");
    } catch {}
  } else {
    discover.customSources = readLocalCustomSources();
  }
  return discover.customSources;
}

export async function addCustomSource({ name, url, kind = "auto" }) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    const sources = await invoke("custom_source_add", { name, url, kind });
    discover.customSources = sources;
    await reloadMergedCatalog();
    return sources;
  }
  const cleanName = String(name || "").trim();
  if (!cleanName) throw new Error("Please enter a library name.");
  const normalizedUrl = webNormalizeFeedUrl(url, kind);
  const id = webSlugId(cleanName, normalizedUrl);
  const effectiveKind = normalizedUrl.startsWith("preset:") ? "preset" : kind || "auto";
  const entries = await fetchWebCustomSourceEntries(id, cleanName, normalizedUrl, effectiveKind);
  const sources = readLocalCustomSources().filter((s) => s.id !== id && s.url.toLowerCase() !== normalizedUrl.toLowerCase());
  sources.push({
    id,
    name: cleanName,
    url: normalizedUrl,
    kind: effectiveKind,
    enabled: true,
    last_synced: Math.floor(Date.now() / 1000),
    font_count: entries.length,
    last_error: null,
  });
  const cache = readLocalCustomCache();
  cache[id] = entries;
  writeLocalCustomCache(cache);
  writeLocalCustomSources(sources);
  discover.customSources = sources;
  await reloadMergedCatalog();
  return sources;
}

export async function removeCustomSource(id) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    const sources = await invoke("custom_source_remove", { id });
    discover.customSources = sources;
    await reloadMergedCatalog();
    return sources;
  }
  const sources = readLocalCustomSources().filter((s) => s.id !== id);
  const cache = readLocalCustomCache();
  delete cache[id];
  writeLocalCustomCache(cache);
  writeLocalCustomSources(sources);
  discover.customSources = sources;
  await reloadMergedCatalog();
  return sources;
}

export async function toggleCustomSource(id, enabled) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    const sources = await invoke("custom_source_toggle", { id, enabled });
    discover.customSources = sources;
    await reloadMergedCatalog();
    return sources;
  }
  const sources = readLocalCustomSources().map((s) => (s.id === id ? { ...s, enabled } : s));
  writeLocalCustomSources(sources);
  discover.customSources = sources;
  await reloadMergedCatalog();
  return sources;
}

export async function syncCustomSource(id) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    const sources = await invoke("custom_source_sync", { id });
    discover.customSources = sources;
    await reloadMergedCatalog();
    return sources;
  }
  const sources = readLocalCustomSources();
  const target = sources.find((s) => s.id === id);
  if (!target) throw new Error("Custom source not found.");
  const entries = await fetchWebCustomSourceEntries(target.id, target.name, target.url, target.kind);
  const cache = readLocalCustomCache();
  cache[id] = entries;
  writeLocalCustomCache(cache);
  const updated = sources.map((s) =>
    s.id === id ? { ...s, last_synced: Math.floor(Date.now() / 1000), font_count: entries.length, last_error: null } : s,
  );
  writeLocalCustomSources(updated);
  discover.customSources = updated;
  await reloadMergedCatalog();
  return updated;
}
