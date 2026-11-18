import { inTauri } from "./settings.svelte.js";
import { notify } from "./toasts.svelte.js";
import { t } from "./i18n.svelte.js";

export const fonts = $state({
  list: [],
  diagnostics: { total_families: 0, exact_duplicates: 0, style_conflicts: 0, corrupt_files: 0 },
  projects: [],
  loading: true,
  skeleton: false,
  busy: false,
});

let invokeFn = null;
export async function invokeCmd(cmd, args) {
  if (!inTauri) return demoResponse(cmd, args);
  if (!invokeFn) invokeFn = (await import("@tauri-apps/api/core")).invoke;
  return invokeFn(cmd, args);
}

const faceMap = new Map();
let convertSrc = null;

let styleSheet = null;
function getSheet() {
  if (!styleSheet) {
    const el = document.createElement("style");
    document.head.appendChild(el);
    styleSheet = el.sheet;
  }
  return styleSheet;
}

const cssQuote = (s) => `"${String(s).replace(/["\\]/g, "")}"`;

function registerFace(style, useLocal) {
  const key = style.file + "|" + style.full_name + "|" + useLocal;
  if (faceMap.has(key)) return faceMap.get(key);
  const id = "cf" + faceMap.size;
  faceMap.set(key, id);
  const src = [];
  if (useLocal && style.full_name) src.push(`local(${cssQuote(style.full_name)})`);
  if (convertSrc) src.push(`url("${convertSrc(style.file)}")`);
  if (src.length) {
    getSheet().insertRule(
      `@font-face{font-family:"${id}";src:${src.join(",")};font-display:block;}`,
      getSheet().cssRules.length,
    );
  }
  return id;
}

export function fontStyle(fam, style = null) {
  const s = style ?? fam.styles[0];
  if (!s) return { family: `${cssQuote(fam.name)}, var(--sans)`, weight: 400, italic: false };
  const useLocal = fam.active && fam.origin !== "dir";
  return {
    family: `${cssQuote(registerFace(s, useLocal))}, ${cssQuote(s.win_family)}, var(--sans)`,
    weight: s.weight,
    italic: s.italic,
  };
}

function setFonts(list, diagnostics = null, projects = null) {
  fonts.list = list;
  if (diagnostics) fonts.diagnostics = diagnostics;
  if (projects) fonts.projects = projects;
  fonts.loading = false;
  fonts.skeleton = false;
}

const warnedCorrupt = new Set();
function warnCorrupt(list) {
  const fresh = list.filter((f) => f.error && !warnedCorrupt.has(f.file));
  fresh.forEach((f) => warnedCorrupt.add(f.file));
  if (fresh.length === 1) notify("err", t("err.read", { n: fresh[0].name }), fresh[0].error);
  else if (fresh.length > 1) notify("err", t("err.readMany", { n: fresh.length }), t("err.readText"));
}

export async function initFonts() {
  if (inTauri) convertSrc = (await import("@tauri-apps/api/core")).convertFileSrc;
  const slow = setTimeout(() => fonts.loading && (fonts.skeleton = true), 150);
  try {
    const saved = await invokeCmd("saved_fonts");
    if (saved.length) setFonts(saved);
  } catch {}
  await refreshFonts();
  clearTimeout(slow);
  if (inTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    listen("fonts-changed", () => refreshFonts());
    listen("inventory-updated", ({ payload }) => {
      if (Array.isArray(payload)) setFonts(payload);
    });
  }
}

export async function refreshFonts() {
  try {
    const res = await invokeCmd("refresh_fonts");
    setFonts(res.families, res.diagnostics, res.projects);
    warnCorrupt(res.families);
  } catch (e) {
    fonts.loading = false;
    notify("err", t("err.list"), String(e));
  }
}

function reportOutcome(res) {
  const o = res.outcome;
  if (o.cancelled) notify("warn", t("warn.noAdmin"), t("warn.noAdminText"));
  if (o.errors.length) {
    notify("err", o.errors.length === 1 ? t("err.one") : t("err.many", { n: o.errors.length }), o.errors[0]);
  }
  return o;
}

export async function toggleFamilies(names, activate, systemUnlocked) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("toggle_fonts", {
      families: names,
      activate,
      systemUnlocked,
    });
    setFonts(res.families, res.diagnostics);
    const o = reportOutcome(res);
    if (o.reboot_required) notify("warn", t("warn.reboot"), t("warn.rebootText"));
    return o;
  } catch (e) {
    notify("err", t("err.one"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}

export async function installFontFiles(paths, installForAll) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("install_fonts", { paths, installForAll });
    setFonts(res.families, res.diagnostics);
    return reportOutcome(res);
  } catch (e) {
    notify("err", t("err.install"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}

export async function restoreSystemFonts() {
  fonts.busy = true;
  try {
    const res = await invokeCmd("restore_system_fonts");
    setFonts(res.families, res.diagnostics);
    return reportOutcome(res);
  } finally {
    fonts.busy = false;
  }
}

export async function quarantineCorruptFont(path) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("quarantine_font", { path });
    setFonts(res.families, res.diagnostics);
    const o = reportOutcome(res);
    if (o.changed > 0) notify("ok", t("ok.quarantined"), t("ok.quarantinedText"));
    return o;
  } catch (e) {
    notify("err", t("err.one"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}

export const applyExternalList = (res) => setFonts(res.families, res.diagnostics);
export const backedUpCount = () => invokeCmd("backed_up_count");
export const fontCharacters = (path) => invokeCmd("font_characters", { path });
export const revealInExplorer = (path) => invokeCmd("show_in_explorer", { path }).catch(() => {});

export async function installForgedFont(familyName, styleName, bytes, installForAll = false) {
  const cleanName = String(familyName || "Cosmic Forge").trim() || "Cosmic Forge";
  const fileName = `${cleanName.replace(/[^A-Za-z0-9-_]+/g, "-")}-${styleName}.ttf`;
  if (!inTauri) {
    try {
      const face = new FontFace(cleanName, bytes);
      await face.load();
      document.fonts.add(face);
    } catch {}
    const existingIdx = fonts.list.findIndex((f) => f.name.toLowerCase() === cleanName.toLowerCase());
    const entry = {
      name: cleanName,
      category: "display",
      origin: "usr",
      active: true,
      essential: false,
      styles: [
        {
          name: styleName,
          weight: styleName === "Bold" ? 700 : 400,
          italic: styleName === "Oblique",
          file: cleanName,
          win_family: cleanName,
          full_name: `${cleanName} ${styleName}`,
          postscript_name: fileName.replace(".ttf", ""),
          axes: [],
          opentype_features: [],
        },
      ],
      file: cleanName,
      files: [cleanName],
      error: null,
      scripts: ["Latin"],
      is_variable: false,
      axes: [],
      opentype_features: [],
      foundry: "Cosmic Font Foundry",
      designer: "Forge Studio",
      license: "CC0 1.0 Universal (Public Domain)",
      license_url: null,
      sha256: "forged-" + Date.now(),
      postscript_name: fileName.replace(".ttf", ""),
      duplicates: [],
      has_conflicts: false,
    };
    const next = [...fonts.list];
    if (existingIdx >= 0) next[existingIdx] = entry;
    else next.unshift(entry);
    setFonts(next);
    return { changed: 1, errors: [], cancelled: false };
  }
  fonts.busy = true;
  try {
    const res = await invokeCmd("install_forged_font", {
      fileName,
      bytes: Array.from(bytes),
      installForAll,
    });
    setFonts(res.families, res.diagnostics);
    return reportOutcome(res);
  } catch (e) {
    notify("err", t("err.install"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}

function demoResponse(cmd) {
  const mk = (name, category, origin, active, extra = {}) => ({
    name,
    category,
    origin,
    active,
    essential: ["Arial", "Segoe UI", "Consolas"].includes(name),
    styles: [
      {
        name: "Regular",
        weight: 400,
        italic: false,
        file: name,
        win_family: name,
        full_name: name,
        postscript_name: name.replace(/\s+/g, "-"),
        axes: extra.axes ?? [],
        opentype_features: extra.opentype_features ?? ["liga", "kern"],
      },
    ],
    file: name,
    files: [name],
    error: null,
    scripts: extra.scripts ?? ["Latin", "Latin Ext"],
    is_variable: Boolean(extra.axes?.length),
    axes: extra.axes ?? [],
    opentype_features: extra.opentype_features ?? ["liga", "kern"],
    foundry: extra.foundry ?? "Cosmic Foundry Demo",
    designer: extra.designer ?? "Demo Type Studio",
    license: "SIL Open Font License 1.1",
    license_url: "https://scripts.sil.org/OFL",
    sha256: "demo-" + name.toLowerCase(),
    postscript_name: name.replace(/\s+/g, "-"),
    duplicates: [],
    has_conflicts: false,
  });

  const list = [
    mk("Archivo", "sans", "lib", true, {
      axes: [
        { tag: "wght", name: "Weight", min: 100, default: 400, max: 900, hidden: false },
        { tag: "wdth", name: "Width", min: 62.5, default: 100, max: 125, hidden: false },
      ],
      opentype_features: ["liga", "dlig", "onum", "tnum", "smcp", "ss01"],
    }),
    mk("Segoe UI", "sans", "sys", true),
    mk("Arial", "sans", "sys", true),
    mk("Georgia", "serif", "sys", true),
    mk("JetBrains Mono", "mono", "lib", true, {
      opentype_features: ["liga", "calt", "zero", "ss01", "ss02"],
    }),
    mk("Gochi Hand", "script", "usr", true),
    mk("Consolas", "mono", "sys", true),
    mk("Impact", "display", "sys", false),
  ];

  if (cmd === "saved_fonts") return Promise.resolve(list);
  if (cmd === "refresh_fonts") {
    return Promise.resolve({
      families: list,
      parsed: 0,
      diagnostics: { total_families: list.length, exact_duplicates: 0, style_conflicts: 0, corrupt_files: 0 },
      projects: [],
    });
  }
  if (cmd === "watched_folders") return Promise.resolve([]);
  if (cmd === "backed_up_count") return Promise.resolve(0);
  if (cmd === "font_characters") return Promise.resolve([...Array(94)].map((_, i) => 33 + i));
  if (cmd === "detect_tuna_migration") return Promise.resolve({ available: false });
  return Promise.reject("desktop app required");
}
