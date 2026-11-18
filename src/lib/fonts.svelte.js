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

export async function installFamilyToWindows(family, installForAll = false) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("install_family_to_windows", { family, installForAll });
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
export const scanComputerFonts = (customDir = null) => invokeCmd("scan_computer_fonts", { customDir });
export const cancelComputerScan = () => invokeCmd("cancel_computer_scan");
export const watchFolderByPath = (path) => invokeCmd("watch_folder_path", { path });

export async function copyFamiliesToVault(families) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("copy_families_to_vault", { families });
    if (res?.families) setFonts(res.families, res.diagnostics);
    return res;
  } catch (e) {
    notify("err", t("err.one"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}

export async function installFamiliesToWindows(families, installForAll = false) {
  fonts.busy = true;
  try {
    const res = await invokeCmd("install_families_to_windows", { families, installForAll });
    setFonts(res.families, res.diagnostics);
    return reportOutcome(res);
  } catch (e) {
    notify("err", t("err.install"), String(e));
    return null;
  } finally {
    fonts.busy = false;
  }
}
export const agentGetState = () => invokeCmd("agent_get_state");
export const agentSetConfig = (config) => invokeCmd("agent_set_config", { config });
export const agentRunCommand = (prompt) => invokeCmd("agent_run_command", { prompt });

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

const demoWatched = [];
const demoExtraFamilies = [];

function demoResponse(cmd, args = {}) {
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

  const baseList = [
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
  const list = [...demoExtraFamilies, ...baseList];

  if (cmd === "saved_fonts") return Promise.resolve(list);
  if (cmd === "refresh_fonts") {
    return Promise.resolve({
      families: list,
      parsed: 0,
      diagnostics: { total_families: list.length, exact_duplicates: 0, style_conflicts: 0, corrupt_files: 0 },
      projects: [],
    });
  }
  if (cmd === "watched_folders") return Promise.resolve([...demoWatched]);
  if (cmd === "watch_folder_path") {
    const p = args?.path;
    if (p && !demoWatched.some((d) => d.path === p)) {
      const parts = p.split(/[/\\]+/).filter(Boolean);
      demoWatched.push({ path: p, name: parts.slice(-2).join("\\") || p });
    }
    return Promise.resolve([...demoWatched]);
  }
  if (cmd === "scan_computer_fonts") {
    return Promise.resolve({
      os_name: "Windows 11",
      arch: "x86_64",
      system_fonts_dir: "C:\\Windows\\Fonts",
      user_fonts_dir: "C:\\Users\\User\\AppData\\Local\\Microsoft\\Windows\\Fonts",
      system_font_count: baseList.filter((f) => f.origin === "sys").length,
      user_font_count: baseList.filter((f) => f.origin !== "sys").length + demoExtraFamilies.length,
      discovered_folders: [
        {
          path: "C:\\Users\\User\\Downloads\\Brand-Typography-Kit",
          label: "Brand-Typography-Kit",
          origin_kind: "Downloads",
          font_count: 3,
          families: ["Space Grotesk", "Clash Display", "Satoshi"],
          files: [
            "C:\\Users\\User\\Downloads\\Brand-Typography-Kit\\SpaceGrotesk-Variable.ttf",
            "C:\\Users\\User\\Downloads\\Brand-Typography-Kit\\ClashDisplay-Variable.ttf",
            "C:\\Users\\User\\Downloads\\Brand-Typography-Kit\\Satoshi-Variable.ttf",
          ],
        },
        {
          path: "C:\\Users\\User\\Desktop\\Client-Fonts",
          label: "Client-Fonts",
          origin_kind: "Desktop",
          font_count: 2,
          families: ["Outfit", "Syne"],
          files: [
            "C:\\Users\\User\\Desktop\\Client-Fonts\\Outfit-Regular.ttf",
            "C:\\Users\\User\\Desktop\\Client-Fonts\\Syne-Bold.ttf",
          ],
        },
        {
          path: "C:\\Users\\User\\Documents\\Code-Fonts",
          label: "Code-Fonts",
          origin_kind: "Documents",
          font_count: 2,
          families: ["Fira Code", "Source Code Pro"],
          files: [
            "C:\\Users\\User\\Documents\\Code-Fonts\\FiraCode-Regular.ttf",
            "C:\\Users\\User\\Documents\\Code-Fonts\\SourceCodePro-Regular.ttf",
          ],
        },
      ],
    });
  }
  if (cmd === "install_fonts") {
    const paths = args?.paths ?? [];
    for (const rawPath of paths) {
      const stem = rawPath.split(/[/\\]+/).pop()?.replace(/\.(ttf|otf|ttc|otc)$/i, "") || "Custom Font";
      const fam = stem.replace(/-(Regular|Bold|Italic|Variable)$/i, "").replace(/([a-z])([A-Z])/g, "$1 $2");
      if (!list.some((f) => f.name.toLowerCase() === fam.toLowerCase())) {
        demoExtraFamilies.unshift(mk(fam, fam.includes("Code") ? "mono" : "sans", "usr", true));
      }
    }
    const updated = [...demoExtraFamilies, ...baseList];
    return Promise.resolve({
      families: updated,
      outcome: { changed: paths.length, errors: [], cancelled: false },
      diagnostics: { total_families: updated.length, exact_duplicates: 0, style_conflicts: 0, corrupt_files: 0 },
    });
  }
  if (cmd === "agent_get_state") return Promise.resolve(getDemoAgentSnapshot());
  if (cmd === "agent_set_config") {
    const next = { ...demoAgentState.config, ...(args?.config ?? {}) };
    demoAgentState.config = next;
    pushDemoAgentLog(
      "system",
      "config:saved",
      `server=${next.server_enabled}, mutations=${next.allow_mutations}, provider=${next.provider}`,
      true,
    );
    return Promise.resolve(getDemoAgentSnapshot());
  }
  if (cmd === "agent_run_command") {
    return Promise.resolve(runDemoAgentCommand(args?.prompt ?? "", list));
  }
  if (cmd === "backed_up_count") return Promise.resolve(0);
  if (cmd === "font_characters") return Promise.resolve([...Array(94)].map((_, i) => 33 + i));
  if (cmd === "detect_tuna_migration") return Promise.resolve({ available: false });
  return Promise.reject("desktop app required");
}

let demoLogCounter = 3;
const demoAgentState = {
  config: {
    server_enabled: true,
    allow_mutations: true,
    provider: "builtin",
    endpoint: "http://127.0.0.1:11434",
    model: "llama3.2",
    api_key: "",
  },
  port: 17420,
  mcp_url: "http://127.0.0.1:17420/mcp",
  api_base: "http://127.0.0.1:17420",
  logs: [
    {
      id: 2,
      timestamp: new Date().toISOString().slice(11, 19) + "Z",
      source: "system",
      action: "server:listen",
      detail: "Listening on http://127.0.0.1:17420 (MCP + REST)",
      ok: true,
    },
    {
      id: 1,
      timestamp: new Date().toISOString().slice(11, 19) + "Z",
      source: "system",
      action: "agent:boot",
      detail: "Local MCP & REST control plane initialized (0 MB deterministic core)",
      ok: true,
    },
  ],
};

function pushDemoAgentLog(source, action, detail, ok = true) {
  demoAgentState.logs = [
    {
      id: demoLogCounter++,
      timestamp: new Date().toISOString().slice(11, 19) + "Z",
      source,
      action,
      detail,
      ok,
    },
    ...demoAgentState.logs,
  ].slice(0, 64);
}

function getDemoAgentSnapshot() {
  return {
    config: { ...demoAgentState.config },
    port: demoAgentState.port,
    listening: demoAgentState.config.server_enabled,
    mcp_url: demoAgentState.mcp_url,
    api_base: demoAgentState.api_base,
    logs: demoAgentState.logs.map((l) => ({ ...l })),
  };
}

function emitDemoAgentUi(payload) {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("cosmic-demo-agent-ui", { detail: payload }));
  }
}

function runDemoAgentCommand(rawPrompt, list) {
  const prompt = String(rawPrompt || "").trim();
  const lower = prompt.toLowerCase();
  const allowMut = demoAgentState.config.allow_mutations;
  let ok = true;
  let reply = "";
  const executed_actions = [];

  if (demoAgentState.config.provider !== "builtin") {
    pushDemoAgentLog(
      demoAgentState.config.provider,
      "director:external",
      `External BYO-AI (${demoAgentState.config.provider}) selected; preview uses local deterministic router.`,
      true,
    );
  }

  if (lower.includes("audit") || lower.includes("health") || lower.includes("diagnostic")) {
    const active = list.filter((f) => f.active).length;
    const variable = list.filter((f) => f.is_variable).length;
    const status = { total_families: list.length, active_families: active, variable_families: variable };
    pushDemoAgentLog("director", "tool:cosmic_get_status", `total=${list.length}, active=${active}`, true);
    executed_actions.push({ tool: "cosmic_get_status", result: status });
    reply = `Health Audit complete (0 MB local engine): ${list.length} families total (${active} active, ${variable} variable), 0 exact duplicates, 0 style conflicts, 0 corrupt files.`;
  } else if ((lower.includes("organize") || lower.includes("auto")) && (lower.includes("os") || lower.includes("system"))) {
    if (!allowMut) {
      ok = false;
      reply = "Tool 'cosmic_auto_organize' blocked because Read-Only monitoring mode is active.";
      pushDemoAgentLog("director", "tool:cosmic_auto_organize", reply, false);
    } else {
      emitDemoAgentUi({ action: "auto_organize", mode: "os" });
      const monoCount = list.filter((f) => f.category === "mono").length;
      const dispCount = list.filter((f) => f.category === "display").length;
      const sysCount = list.filter((f) => f.origin === "sys").length;
      const usrCount = list.filter((f) => f.origin === "usr" || f.origin === "lib").length;
      pushDemoAgentLog("director", "tool:cosmic_auto_organize", "mode=os", true);
      executed_actions.push({ tool: "cosmic_auto_organize", result: { ok: true, mode: "os" } });
      reply = `OS Collections organized algorithmically: System (${sysCount}), User (${usrCount}), Monospace (${monoCount}), Display (${dispCount}).`;
    }
  } else if (lower.includes("coding") || lower.includes("terminal") || lower.includes("collection") || lower.includes("group")) {
    if (!allowMut) {
      ok = false;
      reply = "Tool 'cosmic_auto_organize' blocked because Read-Only monitoring mode is active.";
      pushDemoAgentLog("director", "tool:cosmic_auto_organize", reply, false);
    } else {
      const wantVar = lower.includes("variable");
      const wantMono = lower.includes("coding") || lower.includes("terminal") || lower.includes("mono");
      const wantSerif = lower.includes("serif");
      const matches = list.filter((f) => {
        if (wantVar && !f.is_variable) return false;
        if (wantMono) return f.category === "mono";
        if (wantSerif) return f.category === "serif";
        return true;
      });
      const names = matches.map((f) => f.name);
      const colName = wantMono ? "Coding & Terminal" : wantVar ? "Variable Typography" : wantSerif ? "Serif Editorial" : "Agent Curated Fonts";
      emitDemoAgentUi({ action: "auto_organize", mode: "smart_collections", collection_name: colName, families: names });
      pushDemoAgentLog("director", "tool:cosmic_auto_organize", `mode=smart_collections, collection=${colName}`, true);
      executed_actions.push({ tool: "cosmic_auto_organize", result: { ok: true, collection_name: colName, count: names.length, families: names } });
      reply = `Created/updated collection '${colName}' with ${names.length} matching families.`;
    }
  } else if (lower.includes("scan")) {
    if (!allowMut) {
      ok = false;
      reply = "Tool 'cosmic_scan_pc_fonts' blocked because Read-Only monitoring mode is active.";
      pushDemoAgentLog("director", "tool:cosmic_scan_pc_fonts", reply, false);
    } else {
      pushDemoAgentLog("director", "tool:cosmic_scan_pc_fonts", "Scanned Downloads, Desktop, Documents & Adobe folders", true);
      executed_actions.push({ tool: "cosmic_scan_pc_fonts", result: { os_name: "Windows", discovered_folders: 3 } });
      reply = "Scanned Windows & user directories: 3 extra font folders discovered (Brand-Typography-Kit, Client-Fonts, Code-Fonts).";
    }
  } else if (lower.startsWith("enable ") || lower.startsWith("activate ") || lower.startsWith("disable ") || lower.startsWith("deactivate ")) {
    if (!allowMut) {
      ok = false;
      reply = "Tool 'cosmic_toggle_fonts' blocked because Read-Only monitoring mode is active.";
      pushDemoAgentLog("director", "tool:cosmic_toggle_fonts", reply, false);
    } else {
      const activate = lower.startsWith("enable ") || lower.startsWith("activate ");
      const targetStr = prompt.replace(/^(enable|activate|disable|deactivate)\s+/i, "");
      const needles = targetStr.split(",").map((s) => s.trim().toLowerCase()).filter(Boolean);
      const matched = list.filter((f) => needles.some((n) => f.name.toLowerCase().includes(n)));
      pushDemoAgentLog("director", "tool:cosmic_toggle_fonts", `families=${matched.map((f) => f.name).join(", ")}, activate=${activate}`, true);
      executed_actions.push({ tool: "cosmic_toggle_fonts", result: { changed: matched.length, activate } });
      reply = matched.length ? `${activate ? "Activated" : "Deactivated"} ${matched.length} matching families (${matched.map((m) => m.name).join(", ")}).` : `No matching family found for "${targetStr}".`;
    }
  } else {
    const cat = lower.includes("sans") ? "sans" : lower.includes("serif") ? "serif" : lower.includes("mono") ? "mono" : lower.includes("display") ? "display" : null;
    const varOnly = lower.includes("variable");
    const matched = list.filter((f) => (!cat || f.category === cat) && (!varOnly || f.is_variable));
    pushDemoAgentLog("director", "intent:search", `category=${cat ?? "all"}, variable=${varOnly}, matched=${matched.length}`, true);
    executed_actions.push({ tool: "cosmic_search_fonts", result: { total_matches: matched.length, fonts: matched.slice(0, 8) } });
    reply = `Found ${matched.length} matching fonts${matched.length ? `: ${matched.slice(0, 5).map((f) => f.name).join(", ")}` : ""}.`;
  }

  return {
    ok,
    provider: demoAgentState.config.provider,
    model: demoAgentState.config.provider === "builtin" ? "deterministic-v1" : demoAgentState.config.model,
    reply,
    executed_actions,
    snapshot: getDemoAgentSnapshot(),
  };
}
