import { LazyStore } from "@tauri-apps/plugin-store";
import { inTauri } from "./settings.svelte.js";

export const library = $state({
  favorites: [],
  // [{ id, name, fonts: [family], pinned: boolean }]
  collections: [],
  // { [familyLower]: string[] }
  tags: {},
  ready: false,
});

const store = inTauri ? new LazyStore("library.json", { autoSave: 150 }) : null;

export async function initLibrary() {
  if (store) {
    library.favorites = (await store.get("favorites")) ?? [];
    library.collections = ((await store.get("collections")) ?? []).map((c) => ({
      pinned: false,
      ...c,
    }));
    library.tags = (await store.get("tags")) ?? {};
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
  let count = 0;
  for (const name of names) {
    if (!col.fonts.includes(name)) {
      col.fonts.push(name);
      count++;
    }
  }
  return count;
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
  const groups = [
    ["OS Default Fonts", fontsList.filter((f) => f.origin === "sys").map((f) => f.name)],
    ["My Installed Fonts", fontsList.filter((f) => f.origin === "usr" || f.origin === "lib").map((f) => f.name)],
    ["Monospace & Coding", fontsList.filter((f) => f.category === "mono").map((f) => f.name)],
    ["Display & Script", fontsList.filter((f) => f.category === "display" || f.category === "script").map((f) => f.name)],
  ];
  let created = 0;
  for (const [label, names] of groups) {
    if (names.length && upsertCollectionWithFonts(label, names)) created++;
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

export function snapshotLibrary() {
  return {
    favorites: [...library.favorites],
    collections: library.collections.map((c) => ({
      id: c.id,
      name: c.name,
      fonts: [...c.fonts],
      pinned: Boolean(c.pinned),
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
  if (snap.tags && typeof snap.tags === "object") library.tags = snap.tags;
}
