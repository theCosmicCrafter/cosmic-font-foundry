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
  {
    key: "horror_display",
    url: "preset:horror_display",
    name: "Horror, Gothic & Halloween",
    kind: "preset",
    descKey: "cs.presetHorror",
  },
  {
    key: "handwriting_brush",
    url: "preset:handwriting_brush",
    name: "Handwriting, Brush & Script",
    kind: "preset",
    descKey: "cs.presetHandwriting",
  },
  {
    key: "geometric_ui",
    url: "preset:geometric_ui",
    name: "Geometric & Modern UI Sans",
    kind: "preset",
    descKey: "cs.presetGeometric",
  },
  {
    key: "slab_editorial",
    url: "preset:slab_editorial",
    name: "Slab Serif & Magazine",
    kind: "preset",
    descKey: "cs.presetSlab",
  },
  {
    key: "accessible_reading",
    url: "preset:accessible_reading",
    name: "Accessible & Hyperlegible",
    kind: "preset",
    descKey: "cs.presetAccessible",
  },
  {
    key: "display_poster",
    url: "preset:display_poster",
    name: "Bold Poster & Art Deco",
    kind: "preset",
    descKey: "cs.presetPoster",
  },
];


let pendingCatalog = null;

export function loadCatalog(force = false) {
  if (pendingCatalog && !force) return pendingCatalog;
  discover.loading = true;
  discover.error = "";
  loadCustomSources();
  const req = inTauri
    ? import("@tauri-apps/api/core").then(({ invoke }) => invoke("google_catalog", { force }))
    : import.meta.env.DEV
      ? import("./dev-mock-discover.js").then(({ devLoadCatalog }) => devLoadCatalog(force))
      : Promise.resolve([]);
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
      if (
        item.source === "fontsquirrel" ||
        item.source === "nerdfonts" ||
        item.source === "homebrew" ||
        item.source === "scoop" ||
        item.source === "open_foundries" ||
        item.source === "fontesk" ||
        item.source?.startsWith("custom:")
      ) continue;
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
  if (
    entry.source === "fontsquirrel" ||
    entry.source === "nerdfonts" ||
    entry.source === "homebrew" ||
    entry.source === "scoop" ||
    entry.source === "open_foundries" ||
    entry.source === "fontesk" ||
    entry.source?.startsWith("custom:")
  ) return;
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

export async function loadCustomSources() {
  if (inTauri) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      discover.customSources = await invoke("custom_sources_list");
    } catch {}
  } else if (import.meta.env.DEV) {
    const { devLoadCustomSources } = await import("./dev-mock-discover.js");
    discover.customSources = devLoadCustomSources();
  }
  return discover.customSources;
}

export async function addCustomSource({ name, url, kind = "auto" }) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    discover.customSources = await invoke("custom_source_add", { name, url, kind });
  } else if (import.meta.env.DEV) {
    const { devAddCustomSource } = await import("./dev-mock-discover.js");
    discover.customSources = await devAddCustomSource({ name, url, kind });
  }
  await reloadMergedCatalog();
  return discover.customSources;
}

export async function removeCustomSource(id) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    discover.customSources = await invoke("custom_source_remove", { id });
  } else if (import.meta.env.DEV) {
    const { devRemoveCustomSource } = await import("./dev-mock-discover.js");
    discover.customSources = devRemoveCustomSource(id);
  }
  await reloadMergedCatalog();
  return discover.customSources;
}

export async function toggleCustomSource(id, enabled) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    discover.customSources = await invoke("custom_source_toggle", { id, enabled });
  } else if (import.meta.env.DEV) {
    const { devToggleCustomSource } = await import("./dev-mock-discover.js");
    discover.customSources = devToggleCustomSource(id, enabled);
  }
  await reloadMergedCatalog();
  return discover.customSources;
}

export async function syncCustomSource(id) {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    discover.customSources = await invoke("custom_source_sync", { id });
  } else if (import.meta.env.DEV) {
    const { devSyncCustomSource } = await import("./dev-mock-discover.js");
    discover.customSources = await devSyncCustomSource(id);
  }
  await reloadMergedCatalog();
  return discover.customSources;
}

export async function searchGithubFonts(query = "") {
  const trimmed = String(query || "").trim();
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke("discover_search_github_fonts", { query: trimmed });
  }
  if (import.meta.env.DEV) {
    const { devSearchGithubFonts } = await import("./dev-mock-discover.js");
    return devSearchGithubFonts(trimmed);
  }
  return [];
}

export async function searchFonteskFonts(query = "") {
  const trimmed = String(query || "").trim();
  if (!trimmed) return [];
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke("discover_search_fontesk", { query: trimmed });
  }
  return [];
}

export async function openFontSourceUrl(url) {
  const trimmed = String(url || "").trim();
  if (!trimmed.startsWith("https://")) return;
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke("open_font_source_url", { url: trimmed });
  }
  window.open(trimmed, "_blank", "noopener,noreferrer");
}
