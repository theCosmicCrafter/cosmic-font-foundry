import { inTauri } from "./settings.svelte.js";
import { notify } from "./toasts.svelte.js";
import { t } from "./i18n.svelte.js";

export const discover = $state({
  list: [],
  loading: false,
  error: "",
  downloads: {},
});

let pendingCatalog = null;
export function loadCatalog(force = false) {
  if (pendingCatalog && !force) return pendingCatalog;
  if (!inTauri) return Promise.resolve();
  discover.loading = true;
  discover.error = "";
  pendingCatalog = import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke("google_catalog", { force }))
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

const requestedChars = new Map();
const BATCH_SIZE = 15;

export function requestGooglePreviews(families, text) {
  const chars = new Set([...(text + "Aa")].filter((c) => c.trim()));
  const missing = [];
  for (const fam of families) {
    const prev = requestedChars.get(fam) ?? new Set();
    if ([...chars].some((c) => !prev.has(c))) missing.push(fam);
  }
  for (let i = 0; i < missing.length; i += BATCH_SIZE) {
    const batch = missing.slice(i, i + BATCH_SIZE);
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
/** Loads the full Google Fonts stylesheet (including variable axis ranges) for Discover Specimen view. */
export function loadGoogleFullSpecimen(entry) {
  if (!entry || loadedFull.has(entry.name)) return;
  loadedFull.add(entry.name);
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
      const top = [...discover.list]
        .sort((a, b) => a.popularity - b.popularity)
        .slice(0, 45)
        .map((f) => f.name);
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
