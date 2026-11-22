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
  {
    key: "nerd_fonts",
    url: "preset:nerd_fonts",
    name: "Nerd Fonts Patched",
    kind: "preset",
    descKey: "cs.presetNerd",
  },
  {
    key: "historical",
    url: "preset:historical",
    name: "Historical & Editorial Serif",
    kind: "preset",
    descKey: "cs.presetHistorical",
  },
  {
    key: "retro_pixel",
    url: "preset:retro_pixel",
    name: "Retro, Pixel & Bitmap",
    kind: "preset",
    descKey: "cs.presetRetro",
  },
  {
    key: "global_scripts",
    url: "preset:global_scripts",
    name: "Global Multi-Script",
    kind: "preset",
    descKey: "cs.presetGlobal",
  },
  {
    key: "sil_scripts",
    url: "preset:sil_scripts",
    name: "SIL International Scripts",
    kind: "preset",
    descKey: "cs.presetSil",
  },
  {
    key: "collletttivo",
    url: "preset:collletttivo",
    name: "Collletttivo & Experimental",
    kind: "preset",
    descKey: "cs.presetCollletttivo",
  },
  {
    key: "use_modify",
    url: "preset:use_modify",
    name: "Use & Modify Editorial",
    kind: "preset",
    descKey: "cs.presetUseModify",
  },
  {
    key: "oldschool_pc",
    url: "preset:oldschool_pc",
    name: "Oldschool PC & CRT Mono",
    kind: "preset",
    descKey: "cs.presetOldschool",
  },
  {
    key: "indestructible",
    url: "preset:indestructible",
    name: "Indestructible Type",
    kind: "preset",
    descKey: "cs.presetIndestructible",
  },
  {
    key: "open_foundry",
    url: "preset:open_foundry",
    name: "Open Foundry & Uncut",
    kind: "preset",
    descKey: "cs.presetOpenFoundry",
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
  const [fsRes, srcRes, sqRes, bunnyRes] = await Promise.allSettled([
    fetch("https://api.fontshare.com/v2/fonts?offset=0&limit=100").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://api.fontsource.org/v1/fonts").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://www.fontsquirrel.com/api/fontlist/all").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
    fetch("https://fonts.bunny.net/list").then((r) => (r.ok ? r.json() : Promise.reject(r.status))),
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

  const bunnyEntries =
    bunnyRes.status === "fulfilled" && bunnyRes.value && typeof bunnyRes.value === "object" && !Array.isArray(bunnyRes.value)
      ? Object.entries(bunnyRes.value).map(([slug, f], idx) => {
          const weights = f?.weights?.length ? f.weights : [400];
          const styles = f?.styles?.length ? f.styles : ["normal"];
          const defSubset = f?.defSubset || "latin";
          const name = (f?.familyName || slug.replace(/-/g, " ")).trim();
          return {
            name,
            category: mapCategory(f?.category || ""),
            styles: Math.max(1, weights.length * styles.length),
            popularity: 50000 + idx,
            date: "",
            scripts: ["Latin"],
            designers: [],
            is_variable: Boolean(f?.isVariable),
            axes: [],
            variants: weights.map(String),
            source: "bunny",
            source_id: slug,
            preview_url: null,
            download_urls: weights.map(
              (w) => `https://cdn.jsdelivr.net/fontsource/fonts/${slug}@latest/${defSubset}-${w}-normal.ttf`,
            ),
          };
        })
      : [];

  const seen = new Set();
  const combined = [];
  for (const entry of [...fontshareEntries, ...fontsourceEntries, ...fontsquirrelEntries, ...bunnyEntries]) {
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

const loadedBunny = new Set();
function ensureBunnyStylesheet(entry) {
  const slug = entry?.source_id;
  if (!slug || loadedBunny.has(slug)) return;
  loadedBunny.add(slug);
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `https://fonts.bunny.net/css?family=${encodeURIComponent(slug)}:400,700&display=swap`;
  document.head.appendChild(link);
}

const requestedChars = new Map();
const BATCH_SIZE = 15;

export function requestGooglePreviews(items, text) {
  const chars = new Set([...(text + "Aa")].filter((c) => c.trim()));
  const googleNames = [];
  for (const item of items) {
    if (item && typeof item === "object") {
      if (item.source === "bunny") {
        ensureBunnyStylesheet(item);
        continue;
      }
      if (item.preview_url) {
        ensureDirectFace(item);
        continue;
      }
      if (item.source === "fontsquirrel" || item.source === "nerdfonts" || item.source?.startsWith("custom:")) continue;
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
  if (entry.source === "bunny" && entry.source_id) {
    ensureBunnyStylesheet(entry);
    return;
  }
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
  if (entry.source === "fontsquirrel" || entry.source === "nerdfonts" || entry.source?.startsWith("custom:")) return;
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

const fsPreset = (name, category, designer, styles, is_variable, slug, subset = "latin", weight = 400, scripts = ["Latin"]) => [
  name,
  category,
  designer,
  styles,
  is_variable,
  scripts,
  `https://cdn.jsdelivr.net/fontsource/fonts/${slug}@latest/${subset}-${weight}-normal.woff2`,
  `https://cdn.jsdelivr.net/fontsource/fonts/${slug}@latest/${subset}-${weight}-normal.ttf`,
];

const nfPreset = (name, stem, previewSlug = null) => [
  name,
  "mono",
  "Nerd Fonts",
  1,
  false,
  ["Latin", "Symbols"],
  previewSlug ? `https://cdn.jsdelivr.net/fontsource/fonts/${previewSlug}@latest/latin-400-normal.woff2` : null,
  `https://github.com/ryanoasis/nerd-fonts/releases/latest/download/${stem}.zip`,
];

function webPresetEntries(presetKey, sourceIdKey, sourceName) {
  const srcTag = `custom:${sourceIdKey}`;
  const table = {
    velvetyne: [
      fsPreset("Syne", "display", "Bonjour Monde", 5, true, "syne", "latin", 700),
      fsPreset("Silkscreen", "display", "Jason Kottke / Velvetyne", 2, false, "silkscreen", "latin", 400),
      fsPreset("Unbounded", "display", "Studio Koto", 8, true, "unbounded", "latin", 600),
      fsPreset("Bricolage Grotesque", "sans", "Mathieu Triay", 8, true, "bricolage-grotesque", "latin", 500),
    ],
    leagueof: [
      fsPreset("League Spartan", "sans", "Matt Bailey & Tyler Finck", 9, true, "league-spartan", "latin", 600),
      fsPreset("Orbitron", "display", "Matt McInerney", 6, true, "orbitron", "latin", 600),
      fsPreset("League Gothic", "display", "Micah Rich & Caroline Hadilaksono", 2, true, "league-gothic", "latin", 400),
      fsPreset("League Mono", "mono", "Tyler Finck", 8, true, "league-mono", "latin", 400),
    ],
    indie_mono: [
      fsPreset("Fira Code", "mono", "Nikita Prokopov", 5, true, "fira-code"),
      fsPreset("JetBrains Mono", "mono", "JetBrains", 16, true, "jetbrains-mono"),
      fsPreset("Victor Mono", "mono", "Rune B", 14, false, "victor-mono"),
      fsPreset("Space Mono", "mono", "Colophon Foundry", 4, false, "space-mono"),
      fsPreset("Commit Mono", "mono", "Eigil Nikolajsen", 4, false, "commit-mono"),
      fsPreset("Monaspace Neon", "mono", "GitHub Next", 8, true, "monaspace-neon"),
      fsPreset("Geist Mono", "mono", "Vercel", 9, true, "geist-mono"),
      fsPreset("Iosevka", "mono", "Belleve Invis", 18, false, "iosevka"),
    ],
    nerd_fonts: [
      nfPreset("JetBrainsMono Nerd Font", "JetBrainsMono", "jetbrains-mono"),
      nfPreset("FiraCode Nerd Font", "FiraCode", "fira-code"),
      nfPreset("Hack Nerd Font", "Hack", "hack"),
      nfPreset("CascadiaCode Nerd Font", "CascadiaCode", "cascadia-code"),
      nfPreset("Meslo Nerd Font", "Meslo", null),
      nfPreset("Monaspace Nerd Font", "Monaspace", "monaspace-neon"),
    ],
    historical: [
      fsPreset("EB Garamond", "serif", "Georg Duffner", 10, true, "eb-garamond"),
      fsPreset("Cormorant Garamond", "serif", "Christian Thalmann", 10, false, "cormorant-garamond"),
      fsPreset("Libre Baskerville", "serif", "Impallari Type", 3, false, "libre-baskerville"),
      fsPreset("IM Fell English", "serif", "Igino Marini", 2, false, "im-fell-english"),
      fsPreset("Fraunces", "serif", "Undercase Type", 14, true, "fraunces"),
      fsPreset("Spectral", "serif", "Production Type", 14, false, "spectral"),
      fsPreset("Bodoni Moda", "serif", "Indestructible Type", 12, true, "bodoni-moda"),
    ],
    retro_pixel: [
      fsPreset("Silkscreen", "display", "Jason Kottke", 2, false, "silkscreen"),
      fsPreset("Press Start 2P", "display", "CodeMan38", 1, false, "press-start-2p"),
      fsPreset("VT323", "mono", "Peter Hull", 1, false, "vt323"),
      fsPreset("Pixelify Sans", "display", "Stefie Justprince", 4, true, "pixelify-sans"),
      fsPreset("Jersey 10", "display", "Sarah Cadigan-Fried", 1, false, "jersey-10"),
      fsPreset("Tiny5", "display", "Gisselle Trejos", 1, false, "tiny5"),
    ],
    global_scripts: [
      fsPreset("Vazirmatn", "sans", "Saber Rastikerdar", 9, true, "vazirmatn", "arabic", 400, ["Arabic", "Latin"]),
      fsPreset("Cairo", "sans", "Mohamed Gaber", 8, true, "cairo", "arabic", 400, ["Arabic", "Latin"]),
      fsPreset("Rubik", "sans", "Hubert & Fischer", 14, true, "rubik", "latin", 400, ["Latin", "Cyrillic", "Hebrew", "Arabic"]),
      fsPreset("Noto Sans JP", "other", "Google / Monotype", 9, true, "noto-sans-jp", "japanese", 400, ["Japanese", "Latin"]),
      fsPreset("Noto Sans Arabic", "other", "Google / Monotype", 9, true, "noto-sans-arabic", "arabic", 400, ["Arabic"]),
      fsPreset("Shippori Mincho", "serif", "Fontdasu", 5, false, "shippori-mincho", "japanese", 400, ["Japanese", "Latin"]),
    ],
    sil_scripts: [
      fsPreset("Gentium Plus", "serif", "SIL International", 4, false, "gentium-plus", "latin", 400, ["Latin", "Greek", "Cyrillic"]),
      fsPreset("Charis SIL", "serif", "SIL International", 4, false, "charis-sil", "latin", 400, ["Latin", "Cyrillic"]),
      fsPreset("Andika", "sans", "SIL International", 4, false, "andika", "latin", 400, ["Latin", "Cyrillic"]),
      fsPreset("Scheherazade New", "other", "SIL International", 4, false, "scheherazade-new", "arabic", 400, ["Arabic", "Latin"]),
      fsPreset("Abyssinica SIL", "other", "SIL International", 1, false, "abyssinica-sil", "latin", 400, ["Latin"]),
      fsPreset("Padauk", "other", "SIL International", 4, false, "padauk", "latin", 400, ["Latin"]),
    ],
    collletttivo: [
      fsPreset("Instrument Serif", "serif", "Rodrigo Fuenzalida", 2, false, "instrument-serif"),
      fsPreset("Climate Crisis", "display", "Daniel Coull", 1, true, "climate-crisis"),
      fsPreset("Plus Jakarta Sans", "sans", "Tokotype", 14, true, "plus-jakarta-sans"),
      fsPreset("Rubik Glitch", "display", "Meir Sadan", 1, false, "rubik-glitch"),
      fsPreset("Archivo Black", "display", "Omnibus-Type", 1, false, "archivo-black"),
    ],
    use_modify: [
      fsPreset("Work Sans", "sans", "Wei Huang", 18, true, "work-sans"),
      fsPreset("Epilogue", "sans", "Tyler Finck", 18, true, "epilogue"),
      fsPreset("Space Grotesk", "sans", "Florian Karsten", 5, true, "space-grotesk"),
      fsPreset("Fraunces", "serif", "Undercase Type", 14, true, "fraunces"),
      fsPreset("Commissioner", "sans", "Kostas Bartsokas", 18, true, "commissioner"),
    ],
    oldschool_pc: [
      fsPreset("IBM Plex Mono", "mono", "Mike Abbink / Bold Monday", 14, false, "ibm-plex-mono"),
      fsPreset("VT323", "mono", "Peter Hull", 1, false, "vt323"),
      fsPreset("Press Start 2P", "display", "CodeMan38", 1, false, "press-start-2p"),
      fsPreset("Silkscreen", "display", "Jason Kottke", 2, false, "silkscreen"),
      fsPreset("Share Tech Mono", "mono", "Ralph du Carrois", 1, false, "share-tech-mono"),
      fsPreset("Cousine", "mono", "Steve Matteson", 4, false, "cousine"),
    ],
    indestructible: [
      fsPreset("Jost", "sans", "Owen Earl (indestructible type*)", 9, true, "jost"),
      fsPreset("Besley", "serif", "Owen Earl (indestructible type*)", 12, true, "besley"),
      fsPreset("Bodoni Moda", "serif", "Owen Earl (indestructible type*)", 12, true, "bodoni-moda"),
      fsPreset("Libre Bodoni", "serif", "Pablo Impallari", 4, true, "libre-bodoni"),
      fsPreset("Newsreader", "serif", "Production Type", 14, true, "newsreader"),
    ],
    open_foundry: [
      fsPreset("Inter", "sans", "Rasmus Andersson", 18, true, "inter"),
      fsPreset("Instrument Sans", "sans", "Rodrigo Fuenzalida", 8, true, "instrument-sans"),
      fsPreset("Archivo", "sans", "Omnibus-Type", 18, true, "archivo"),
      fsPreset("DM Sans", "sans", "Colophon Foundry", 12, true, "dm-sans"),
      fsPreset("Manrope", "sans", "Mikhail Sharanda", 7, true, "manrope"),
    ],
  };
  return (table[presetKey] || []).map(([name, category, designer, styles, is_variable, scripts, woff2, ttf], idx) => ({
    name,
    category,
    styles,
    popularity: idx + 1,
    date: "2026-01-01",
    scripts,
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

function extractVersionTag(raw) {
  const m = String(raw || "").match(/\bv?\d+\.\d+(?:\.\d+)?\b/i);
  return m ? m[0] : null;
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
      version_tag: extractVersionTag(rawTitle),
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
          version_tag: it.tag_name || it.version || null,
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
      version_tag: it.version || it.tag_name || null,
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

export async function searchGithubFonts(query = "") {
  const trimmed = String(query || "").trim();
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke("discover_search_github_fonts", { query: trimmed });
  }
  const q = trimmed ? `${trimmed} topic:font` : "topic:font license:ofl-1.1";
  const url = `https://api.github.com/search/repositories?q=${encodeURIComponent(q)}&sort=stars&per_page=18`;
  const resp = await fetch(url, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!resp.ok) {
    throw new Error(`GitHub search failed (HTTP ${resp.status})`);
  }
  const data = await resp.json();
  const items = Array.isArray(data?.items) ? data.items : [];
  return items.map((item) => {
    const htmlUrl = item.html_url || "";
    const rawDesc = String(item.description || "").trim();
    const description = rawDesc.length > 180 ? `${rawDesc.slice(0, 177)}...` : rawDesc;
    const releasesUrl =
      (item.releases_url ? String(item.releases_url).replace("{/id}", "") : "") ||
      (htmlUrl ? `${htmlUrl.replace(/\/+$/, "")}/releases` : "");
    return {
      full_name: item.full_name || "",
      name: item.name || item.full_name || "",
      description,
      stars: Number(item.stargazers_count || 0),
      license: item.license?.spdx_id || "Unknown",
      html_url: htmlUrl,
      releases_url: releasesUrl,
      updated_at: item.updated_at || "",
    };
  });
}
