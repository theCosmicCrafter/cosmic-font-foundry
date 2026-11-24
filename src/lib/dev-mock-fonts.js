const demoWatched = [];
const demoExtraFamilies = [];

export function demoResponse(cmd, args = {}) {
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
  if (cmd === "discover_search_github_fonts") return Promise.resolve([]);
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
