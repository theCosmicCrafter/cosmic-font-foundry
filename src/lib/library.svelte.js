import { LazyStore } from "@tauri-apps/plugin-store";
import { inTauri } from "./settings.svelte.js";

export const library = $state({
  favorites: [],
  // [{ id, name, fonts: [family], pinned: boolean }]
  collections: [],
  // [{ id, name, rules: { origin?, category?, script?, filter?, commercialOnly?, installedOnly?, query?, tag? } }]
  smartCollections: [],
  // { [familyLower]: string[] }
  tags: {},
  ready: false,
});

const DEFAULT_SMART_COLLECTIONS = [
  { id: "sc-comm-var", name: "Commercial & Variable", rules: { commercialOnly: true, filter: "variable" } },
  { id: "sc-vault-dir", name: "Vault & Watched", rules: { origin: "dir" } },
];

const store = inTauri ? new LazyStore("library.json", { autoSave: 150 }) : null;

export async function initLibrary() {
  if (store) {
    library.favorites = (await store.get("favorites")) ?? [];
    library.collections = ((await store.get("collections")) ?? []).map((c) => ({
      pinned: false,
      ...c,
    }));
    const savedSmart = await store.get("smartCollections");
    library.smartCollections = savedSmart ?? DEFAULT_SMART_COLLECTIONS.map((s) => ({ ...s, rules: { ...s.rules } }));
    library.tags = (await store.get("tags")) ?? {};
  } else {
    library.smartCollections = DEFAULT_SMART_COLLECTIONS.map((s) => ({ ...s, rules: { ...s.rules } }));
  }
  library.ready = true;
}

$effect.root(() => {
  $effect(() => {
    const f = $state.snapshot(library.favorites);
    if (library.ready) store?.set("favorites", f);
  });
  $effect(() => {
    const c = $state.snapshot(library.collections);
    if (library.ready) store?.set("collections", c);
  });
  $effect(() => {
    const sc = $state.snapshot(library.smartCollections);
    if (library.ready) store?.set("smartCollections", sc);
  });
  $effect(() => {
    const t = $state.snapshot(library.tags);
    if (library.ready) store?.set("tags", t);
  });
});

export const isFavorite = (name) => library.favorites.includes(name);

export function toggleFavorite(name) {
  const idx = library.favorites.indexOf(name);
  if (idx >= 0) library.favorites.splice(idx, 1);
  else library.favorites.push(name);
}

export function addToFavorites(names) {
  for (const n of names) {
    if (!library.favorites.includes(n)) library.favorites.push(n);
  }
}

let nextColSeq = 1;
export function createCollection(name) {
  const id = "c" + Date.now().toString(36) + (nextColSeq++).toString(36);
  library.collections.push({ id, name, fonts: [], pinned: false });
  return id;
}

export function renameCollection(id, name) {
  const col = library.collections.find((c) => c.id === id);
  if (col && name.trim()) col.name = name.trim();
}

export function togglePinCollection(id) {
  const col = library.collections.find((c) => c.id === id);
  if (col) col.pinned = !col.pinned;
}

export function deleteCollection(id) {
  const idx = library.collections.findIndex((c) => c.id === id);
  if (idx >= 0) library.collections.splice(idx, 1);
}

export function addFontsToCollection(id, names) {
  const col = library.collections.find((c) => c.id === id);
  if (!col) return 0;
  const seen = new Set(col.fonts.map((n) => n.toLowerCase()));
  const toAdd = [];
  for (const name of names) {
    const key = String(name || "").toLowerCase();
    if (key && !seen.has(key)) {
      seen.add(key);
      toAdd.push(name);
    }
  }
  if (toAdd.length) col.fonts.push(...toAdd);
  return toAdd.length;
}

export function upsertCollectionWithFonts(name, fontNames, pinned = false) {
  const clean = String(name || "").trim();
  if (!clean || !fontNames?.length) return null;
  let col = library.collections.find((c) => c.name.toLowerCase() === clean.toLowerCase());
  if (!col) {
    const id = createCollection(clean);
    col = library.collections.find((c) => c.id === id);
    if (col && pinned) col.pinned = true;
  }
  if (!col) return null;
  addFontsToCollection(col.id, fontNames);
  return col.id;
}

export function autoCreateOsCollections(fontsList = []) {
  if (!fontsList.length) return 0;
  const buckets = {
    sys: [],
    user: [],
    dir: [],
    variable: [],
    sans: [],
    serif: [],
    mono: [],
    displayScript: [],
  };
  const seen = {
    sys: new Set(),
    user: new Set(),
    dir: new Set(),
    variable: new Set(),
    sans: new Set(),
    serif: new Set(),
    mono: new Set(),
    displayScript: new Set(),
  };

  const pushUnique = (bucket, name) => {
    const key = String(name || "").toLowerCase();
    if (key && !seen[bucket].has(key)) {
      seen[bucket].add(key);
      buckets[bucket].push(name);
    }
  };

  for (const f of fontsList) {
    if (f.origin === "sys") pushUnique("sys", f.name);
    else if (f.origin === "usr" || f.origin === "lib") pushUnique("user", f.name);
    else if (f.origin === "dir") pushUnique("dir", f.name);
    if (f.is_variable) pushUnique("variable", f.name);
    if (f.category === "sans") pushUnique("sans", f.name);
    else if (f.category === "serif") pushUnique("serif", f.name);
    else if (f.category === "mono") pushUnique("mono", f.name);
    else if (f.category === "display" || f.category === "script") pushUnique("displayScript", f.name);
  }

  const groups = [
    ["OS Default Fonts", buckets.sys],
    ["My Installed Fonts", buckets.user],
    ["Watched Folders", buckets.dir],
    ["Variable Fonts", buckets.variable],
    ["Sans & UI", buckets.sans],
    ["Serif & Editorial", buckets.serif],
    ["Monospace & Coding", buckets.mono],
    ["Display & Script", buckets.displayScript],
  ];

  let created = 0;
  for (const [label, names] of groups) {
    const existing = library.collections.find((c) => c.name.toLowerCase() === label.toLowerCase());
    if (!names.length) {
      if (existing) existing.fonts = [];
      continue;
    }
    if (existing) {
      existing.fonts = [...names];
      created++;
    } else if (upsertCollectionWithFonts(label, names)) {
      created++;
    }
  }
  return created;
}

export function tagsForFamily(familyName) {
  return library.tags[familyName.toLowerCase()] ?? [];
}

export function addTagToFamily(familyName, rawTag) {
  const clean = rawTag.trim().toLowerCase();
  if (!clean) return;
  const key = familyName.toLowerCase();
  const current = library.tags[key] ?? [];
  if (!current.some((t) => t.toLowerCase() === clean)) {
    library.tags = { ...library.tags, [key]: [...current, clean] };
  }
}

export function removeTagFromFamily(familyName, tag) {
  const key = familyName.toLowerCase();
  const current = library.tags[key] ?? [];
  const next = current.filter((t) => t !== tag);
  if (next.length) library.tags = { ...library.tags, [key]: next };
  else {
    const copy = { ...library.tags };
    delete copy[key];
    library.tags = copy;
  }
}

export function createSmartCollection(name, rules = {}) {
  const clean = String(name || "").trim();
  if (!clean) return null;
  const id = "sc" + Date.now().toString(36) + (nextColSeq++).toString(36);
  library.smartCollections.push({ id, name: clean, rules: { ...rules } });
  return id;
}

export function renameSmartCollection(id, name) {
  const sc = library.smartCollections.find((c) => c.id === id);
  if (sc && name.trim()) sc.name = name.trim();
}

export function deleteSmartCollection(id) {
  const idx = library.smartCollections.findIndex((c) => c.id === id);
  if (idx >= 0) library.smartCollections.splice(idx, 1);
}

export function matchesSmartCollection(font, rules = {}, getTagsFn = tagsForFamily) {
  if (!font) return false;
  if (rules.origin && font.origin !== rules.origin) return false;
  if (rules.installedOnly && font.origin === "dir") return false;
  if (rules.category && font.category !== rules.category) return false;
  if (rules.script && !(font.scripts ?? []).includes(rules.script)) return false;
  if (rules.commercialOnly && font.license_tier !== "free_commercial") return false;
  if (rules.filter === "variable" && !font.is_variable) return false;
  if (rules.filter === "active" && !font.active) return false;
  if (rules.filter === "inactive" && font.active) return false;
  if (rules.tag) {
    const want = String(rules.tag).trim().toLowerCase();
    if (!getTagsFn(font.name).some((tg) => tg.toLowerCase() === want)) return false;
  }
  if (rules.query) {
    const ql = String(rules.query).trim().toLowerCase();
    if (
      ql &&
      !font.name.toLowerCase().includes(ql) &&
      !getTagsFn(font.name).some((tg) => tg.includes(ql))
    ) {
      return false;
    }
  }
  return true;
}

export function snapshotLibrary() {
  return {
    favorites: [...library.favorites],
    collections: library.collections.map((c) => ({
      id: c.id,
      name: c.name,
      fonts: [...c.fonts],
      pinned: Boolean(c.pinned),
    })),
    smartCollections: library.smartCollections.map((sc) => ({
      id: sc.id,
      name: sc.name,
      rules: { ...(sc.rules ?? {}) },
    })),
    tags: JSON.parse(JSON.stringify(library.tags ?? {})),
  };
}

export function applyLibrarySnapshot(snap) {
  if (!snap) return;
  if (Array.isArray(snap.favorites)) library.favorites = snap.favorites;
  if (Array.isArray(snap.collections)) {
    library.collections = snap.collections.map((c) => ({ pinned: false, ...c }));
  }
  if (Array.isArray(snap.smartCollections)) {
    library.smartCollections = snap.smartCollections.map((sc) => ({
      ...sc,
      rules: { ...(sc.rules ?? {}) },
    }));
  }
  if (snap.tags && typeof snap.tags === "object") library.tags = snap.tags;
}
