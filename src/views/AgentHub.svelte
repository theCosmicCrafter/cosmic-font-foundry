<script>
  import { onMount } from "svelte";
  import {
    Activity,
    Bot,
    Check,
    Copy,
    Cpu,
    Play,
    RefreshCw,
    Server,
    Shield,
    Sparkles,
    Terminal,
    Wrench,
  } from "@lucide/svelte";
  import { agentGetState, agentRunCommand, agentSetConfig } from "../lib/fonts.svelte.js";
  import { inTauri } from "../lib/settings.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let { notify = () => {}, embedded = false } = $props();

  let snap = $state({
    config: {
      server_enabled: true,
      allow_mutations: true,
      provider: "builtin",
      endpoint: "http://127.0.0.1:11434",
      model: "llama3.2",
      api_key: "",
    },
    port: 17420,
    listening: true,
    mcp_url: "http://127.0.0.1:17420/mcp",
    api_base: "http://127.0.0.1:17420",
    logs: [],
  });

  let draftProvider = $state("builtin");
  let draftEndpoint = $state("http://127.0.0.1:11434");
  let draftModel = $state("llama3.2");
  let draftApiKey = $state("");

  let promptInput = $state("");
  let running = $state(false);
  let savingConfig = $state(false);
  let copiedMcp = $state(false);
  let lastResult = $state(null);
  let cmdError = $state("");

  const PRESETS = [
    { key: "audit", labelKey: "agent.presetAudit", prompt: "Audit Font Health" },
    { key: "os", labelKey: "agent.presetOs", prompt: "Organize OS Collections" },
    { key: "coding", labelKey: "agent.presetCoding", prompt: "Create Coding & Terminal Kit" },
    { key: "scan", labelKey: "agent.presetScan", prompt: "Scan PC for Font Kits" },
    { key: "variable", labelKey: "agent.presetVariable", prompt: "Find Variable Sans Fonts" },
  ];

  const MCP_TOOLS = [
    { name: "cosmic_get_status", mode: "READ", descKey: "agent.toolStatus" },
    { name: "cosmic_search_fonts", mode: "READ", descKey: "agent.toolSearch" },
    { name: "cosmic_toggle_fonts", mode: "WRITE", descKey: "agent.toolToggle" },
    { name: "cosmic_scan_pc_fonts", mode: "WRITE", descKey: "agent.toolScan" },
    { name: "cosmic_ui_navigate", mode: "WRITE", descKey: "agent.toolNavigate" },
    { name: "cosmic_auto_organize", mode: "WRITE", descKey: "agent.toolOrganize" },
    {
      name: "cosmic_inspect_glyph_outline",
      mode: "READ",
      desc: "Extract vector Bezier contours, SVG path, bbox, and sidebearing metrics for a glyph.",
    },
    {
      name: "cosmic_search_github_fonts",
      mode: "READ",
      desc: "Search open-source font repositories on GitHub with license and release metadata.",
    },
    {
      name: "cosmic_foundry_audit_font",
      mode: "READ",
      desc: "Audit a font file, UFO directory, or family for outline, winding, and OpenType QA issues.",
    },
    {
      name: "cosmic_foundry_autofix_and_compile",
      mode: "WRITE",
      desc: "Auto-repair outline QA issues and compile a font or UFO project to a .ttf binary.",
    },
    {
      name: "cosmic_foundry_trace_image",
      mode: "READ",
      desc: "Vectorize a glyph raster image into Bezier contours, SVG path data, and bbox metrics.",
    },
    {
      name: "cosmic_foundry_generate_style",
      mode: "WRITE",
      desc: "Generate a derived style variant (bold, light, condensed, extended, small caps, oblique, inline) and compile .ttf.",
    },
    {
      name: "cosmic_visual_match_font",
      mode: "READ",
      desc: "Visually identify matching cached fonts from a glyph image or SVG outline.",
    },
    {
      name: "cosmic_vibe_search_fonts",
      mode: "READ",
      desc: "Search local fonts using natural-language typographic vibe queries without ML weights.",
    },
    {
      name: "cosmic_find_outline_duplicates",
      mode: "READ",
      desc: "Detect renamed font duplicates by hashing canonical outline geometry.",
    },
  ];

  function syncDraftFromSnap(nextSnap) {
    snap = nextSnap;
    draftProvider = nextSnap.config?.provider || "builtin";
    draftEndpoint = nextSnap.config?.endpoint || "http://127.0.0.1:11434";
    draftModel = nextSnap.config?.model || "llama3.2";
    draftApiKey = nextSnap.config?.api_key || "";
  }

  async function loadState() {
    try {
      const next = await agentGetState();
      syncDraftFromSnap(next);
    } catch {
      // Ignore in non-tauri fallback edge cases
    }
  }

  onMount(() => {
    loadState();
    if (!inTauri) return;
    let unlisten;
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlisten = await listen("agent-updated", ({ payload }) => {
        if (payload) snap = payload;
      });
    });
    return () => {
      unlisten?.();
    };
  });

  async function savePartialConfig(patch) {
    savingConfig = true;
    try {
      const next = await agentSetConfig({
        ...snap.config,
        ...patch,
      });
      syncDraftFromSnap(next);
    } catch (e) {
      notify("err", t("agent.configErr"), String(e));
    } finally {
      savingConfig = false;
    }
  }

  function selectProvider(nextProvider) {
    draftProvider = nextProvider;
    if (nextProvider === "ollama" && (!draftEndpoint || draftEndpoint.includes("openai.com"))) {
      draftEndpoint = "http://127.0.0.1:11434";
      if (!draftModel || draftModel.startsWith("gpt-")) draftModel = "llama3.2";
    } else if (nextProvider === "openai" && (!draftEndpoint || draftEndpoint.includes("11434"))) {
      draftEndpoint = "https://api.openai.com";
      if (!draftModel || draftModel === "llama3.2") draftModel = "gpt-4o-mini";
    }
    if (nextProvider === "builtin") {
      savePartialConfig({ provider: "builtin" });
    }
  }

  async function saveEngineSettings() {
    await savePartialConfig({
      provider: draftProvider,
      endpoint: draftEndpoint,
      model: draftModel,
      api_key: draftApiKey,
    });
    notify("ok", t("agent.saved"), `${draftProvider} (${draftModel || "deterministic"})`);
  }

  async function runPrompt(raw = promptInput) {
    const clean = String(raw || "").trim();
    if (!clean || running) return;
    promptInput = clean;
    running = true;
    cmdError = "";
    try {
      const res = await agentRunCommand(clean);
      lastResult = res;
      if (res?.snapshot) snap = res.snapshot;
      if (!res?.ok) {
        cmdError = res?.reply || t("agent.cmdFailed");
      }
    } catch (e) {
      cmdError = String(e);
    } finally {
      running = false;
    }
  }

  async function copyMcpConfig() {
    const snippet = JSON.stringify(
      {
        mcpServers: {
          "cosmic-font-foundry": {
            url: snap.mcp_url,
          },
        },
      },
      null,
      2,
    );
    try {
      await navigator.clipboard.writeText(snippet);
      copiedMcp = true;
      setTimeout(() => (copiedMcp = false), 1800);
      notify("ok", t("agent.copiedTitle"), snap.mcp_url);
    } catch {
      notify("err", t("agent.copyFail"), snap.mcp_url);
    }
  }
</script>

<div class="agent-hub" class:embedded>
  <!-- Control Plane Banner -->
  <section class="card status-banner">
    <div class="banner-left">
      <div class="banner-title-row">
        <span class="hub-icon">
          <Bot size={18} />
        </span>
        <div>
          <div class="title-line">
            <h2>{t("agent.title")}</h2>
            <span class="pill" class:live={snap.listening} class:paused={!snap.listening}>
              <span class="dot"></span>
              {snap.listening ? t("agent.listening") : t("agent.paused")}
            </span>
            <span class="pill zero-ml">{t("agent.zeroMl")}</span>
          </div>
          <p class="sub">{t("agent.subtitle")}</p>
        </div>
      </div>

      <div class="endpoints">
        <div class="ep-chip">
          <span class="ep-k">MCP</span>
          <code>{snap.mcp_url}</code>
        </div>
        <div class="ep-chip">
          <span class="ep-k">REST</span>
          <code>{snap.api_base}/api/status</code>
        </div>
      </div>
    </div>

    <div class="banner-controls">
      <div class="switch-row">
        <label class="toggle-pill">
          <input
            type="checkbox"
            checked={snap.config.server_enabled}
            onchange={(e) => savePartialConfig({ server_enabled: e.currentTarget.checked })}
          />
          <span>{t("agent.serverToggle")}</span>
        </label>

        <label class="toggle-pill" class:warn-border={!snap.config.allow_mutations}>
          <input
            type="checkbox"
            checked={snap.config.allow_mutations}
            onchange={(e) => savePartialConfig({ allow_mutations: e.currentTarget.checked })}
          />
          <Shield size={13} />
          <span>
            {snap.config.allow_mutations ? t("agent.writeMode") : t("agent.readOnlyMode")}
          </span>
        </label>
      </div>

      <button type="button" class="btn-primary" onclick={copyMcpConfig}>
        {#if copiedMcp}
          <Check size={14} />
          {t("agent.copied")}
        {:else}
          <Copy size={14} />
          {t("agent.copyMcp")}
        {/if}
      </button>
    </div>
  </section>

  <!-- Main Two-Column Grid -->
  <div class="hub-grid">
    <!-- Left Column: Director + BYO-AI Engine -->
    <div class="col">
      <section class="card">
        <div class="card-head">
          <Sparkles size={15} class="head-ico" />
          <h3>{t("agent.directorTitle")}</h3>
          <span class="head-badge">{draftProvider === "builtin" ? "0 MB Engine" : draftProvider}</span>
        </div>
        <p class="card-desc">{t("agent.directorDesc")}</p>

        <div class="presets">
          {#each PRESETS as p (p.key)}
            <button
              type="button"
              class="preset-chip"
              disabled={running}
              onclick={() => runPrompt(p.prompt)}
            >
              <Terminal size={12} />
              {t(p.labelKey)}
            </button>
          {/each}
        </div>

        <form
          class="cmd-form"
          onsubmit={(e) => {
            e.preventDefault();
            runPrompt();
          }}
        >
          <input
            type="text"
            class="cmd-input"
            bind:value={promptInput}
            placeholder={t("agent.promptPlaceholder")}
            aria-label={t("agent.directorTitle")}
          />
          <button type="submit" class="btn-accent" disabled={running || !promptInput.trim()}>
            <Play size={13} />
            {running ? t("agent.running") : t("agent.runBtn")}
          </button>
        </form>

        {#if cmdError}
          <div class="result-box error">{cmdError}</div>
        {:else if lastResult}
          <div class="result-box">
            <div class="result-meta">
              <span>{lastResult.provider} / {lastResult.model}</span>
              <span>{lastResult.executed_actions?.length || 0} {t("agent.actionsRun")}</span>
            </div>
            <p class="result-reply">{lastResult.reply}</p>
          </div>
        {/if}
      </section>

      <section class="card">
        <div class="card-head">
          <Cpu size={15} class="head-ico" />
          <h3>{t("agent.providerTitle")}</h3>
        </div>
        <p class="card-desc">{t("agent.providerDesc")}</p>

        <div class="provider-tabs">
          <button
            type="button"
            class="prov-btn"
            class:selected={draftProvider === "builtin"}
            onclick={() => selectProvider("builtin")}
          >
            <strong>{t("agent.provBuiltin")}</strong>
            <span>{t("agent.provBuiltinSub")}</span>
          </button>
          <button
            type="button"
            class="prov-btn"
            class:selected={draftProvider === "ollama"}
            onclick={() => selectProvider("ollama")}
          >
            <strong>{t("agent.provOllama")}</strong>
            <span>{t("agent.provOllamaSub")}</span>
          </button>
          <button
            type="button"
            class="prov-btn"
            class:selected={draftProvider === "openai"}
            onclick={() => selectProvider("openai")}
          >
            <strong>{t("agent.provOpenai")}</strong>
            <span>{t("agent.provOpenaiSub")}</span>
          </button>
        </div>

        {#if draftProvider !== "builtin"}
          <div class="byo-fields">
            <label>
              <span>{t("agent.endpoint")}</span>
              <input type="url" bind:value={draftEndpoint} placeholder="http://127.0.0.1:11434" />
            </label>
            <div class="two-fields">
              <label>
                <span>{t("agent.model")}</span>
                <input type="text" bind:value={draftModel} placeholder="llama3.2" />
              </label>
              {#if draftProvider === "openai"}
                <label>
                  <span>{t("agent.apiKey")}</span>
                  <input type="password" bind:value={draftApiKey} placeholder="sk-..." autocomplete="off" />
                </label>
              {/if}
            </div>
            <div class="save-row">
              <button type="button" class="btn-primary" disabled={savingConfig} onclick={saveEngineSettings}>
                {savingConfig ? t("agent.saving") : t("agent.saveConfig")}
              </button>
            </div>
          </div>
        {/if}
      </section>
    </div>

    <!-- Right Column: MCP Tools & Telemetry Feed -->
    <div class="col">
      <section class="card">
        <div class="card-head">
          <Wrench size={15} class="head-ico" />
          <h3>{t("agent.toolsTitle")}</h3>
          <span class="head-badge">JSON-RPC 2.0</span>
        </div>

        <div class="tools-list">
          {#each MCP_TOOLS as tool (tool.name)}
            <div class="tool-item">
              <div class="tool-top">
                <code class="tool-name">{tool.name}</code>
                <span class="mode-tag" class:write={tool.mode === "WRITE"}>{tool.mode}</span>
              </div>
              <p>{tool.desc ?? t(tool.descKey)}</p>
            </div>
          {/each}
        </div>

        <div class="rest-bar">
          <Server size={13} />
          <span>REST:</span>
          <code>GET /api/status</code>
          <code>GET /api/fonts</code>
          <code>POST /api/toggle</code>
          <code>POST /api/navigate</code>
        </div>
      </section>

      <section class="card logs-card">
        <div class="card-head">
          <Activity size={15} class="head-ico" />
          <h3>{t("agent.logsTitle")}</h3>
          <button type="button" class="icon-refresh" title={t("agent.refresh")} onclick={loadState}>
            <RefreshCw size={13} />
          </button>
        </div>

        <div class="log-feed">
          {#if !snap.logs || snap.logs.length === 0}
            <p class="empty-logs">{t("agent.logsEmpty")}</p>
          {:else}
            {#each snap.logs as entry (entry.id)}
              <div class="log-row" class:fail={!entry.ok}>
                <span class="log-time">{entry.timestamp}</span>
                <span class="log-src">{entry.source}</span>
                <span class="log-act">{entry.action}</span>
                <span class="log-detail" title={entry.detail}>{entry.detail}</span>
              </div>
            {/each}
          {/if}
        </div>
      </section>
    </div>
  </div>
</div>

<style>
  .agent-hub {
    height: 100%;
    overflow-y: auto;
    padding: 18px 22px 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    color: var(--ink);
  }

  .agent-hub.embedded {
    height: auto;
    overflow-y: visible;
    padding: 2px 0 8px;
  }

  .card {
    background: var(--side);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 15px 16px;
  }

  .status-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    background: linear-gradient(135deg, var(--side), var(--field));
    border-color: color-mix(in srgb, var(--accent) 28%, var(--line));
  }

  .banner-left {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .banner-title-row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .hub-icon {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .title-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--ink2);
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    border: 1px solid var(--line2);
  }

  .pill.live {
    background: color-mix(in srgb, var(--ok) 15%, transparent);
    color: var(--ok);
    border-color: color-mix(in srgb, var(--ok) 35%, transparent);
  }

  .pill.live .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ok);
  }

  .pill.paused {
    color: var(--ink2);
    background: var(--field);
  }

  .pill.paused .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink3);
  }

  .pill.zero-ml {
    background: var(--cyan-soft);
    color: var(--accent-2);
    border-color: color-mix(in srgb, var(--accent-2) 30%, transparent);
  }

  .endpoints {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .ep-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--win);
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 4px 8px;
    font-size: 11.5px;
  }

  .ep-k {
    font-size: 10px;
    font-weight: 700;
    color: var(--accent);
    letter-spacing: 0.04em;
  }

  code {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink2);
  }

  .banner-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .switch-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .toggle-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--win);
    border: 1px solid var(--line2);
    font-size: 12px;
    cursor: pointer;
    user-select: none;
  }

  .toggle-pill.warn-border {
    border-color: color-mix(in srgb, var(--warn) 45%, var(--line));
    color: var(--warn);
  }

  .toggle-pill input {
    accent-color: var(--accent);
    margin: 0;
  }

  .btn-primary,
  .btn-accent {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
    transition: opacity 0.14s, transform 0.12s;
  }

  .btn-primary {
    background: var(--field);
    color: var(--ink);
    border-color: var(--line2);
  }

  .btn-primary:hover {
    border-color: var(--accent);
  }

  .btn-accent {
    background: var(--accent);
    color: #fff;
    white-space: nowrap;
  }

  .btn-accent:disabled,
  .btn-primary:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .hub-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.12fr) minmax(0, 0.95fr);
    gap: 14px;
    align-items: start;
  }

  @media (max-width: 980px) {
    .hub-grid {
      grid-template-columns: 1fr;
    }
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .card-head {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 6px;
  }

  .card-head :global(.head-ico) {
    color: var(--accent);
  }

  .head-badge {
    margin-left: auto;
    font-size: 10.5px;
    font-family: var(--mono);
    color: var(--accent-2);
    background: var(--cyan-soft);
    padding: 2px 7px;
    border-radius: 5px;
  }

  .icon-refresh {
    margin-left: auto;
    background: var(--win);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 6px;
    color: var(--ink2);
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .icon-refresh:hover {
    color: var(--ink);
    border-color: var(--line2);
  }

  .card-desc {
    margin: 0 0 11px;
    font-size: 12px;
    color: var(--ink2);
    line-height: 1.45;
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-bottom: 11px;
  }

  .preset-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: 7px;
    background: var(--win);
    border: 1px solid var(--line);
    color: var(--ink);
    font-size: 11.5px;
    cursor: pointer;
    transition: border-color 0.14s, background 0.14s;
  }

  .preset-chip:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .cmd-form {
    display: flex;
    gap: 8px;
  }

  .cmd-input {
    flex: 1;
    min-width: 0;
    background: var(--win);
    border: 1px solid var(--line2);
    border-radius: 8px;
    padding: 8px 11px;
    color: var(--ink);
    font-size: 12.5px;
  }

  .cmd-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .result-box {
    margin-top: 11px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--win);
    border: 1px solid color-mix(in srgb, var(--ok) 30%, var(--line));
  }

  .result-box.error {
    border-color: color-mix(in srgb, var(--danger) 45%, var(--line));
    color: var(--danger);
    font-size: 12px;
  }

  .result-meta {
    display: flex;
    justify-content: space-between;
    font-size: 10.5px;
    font-family: var(--mono);
    color: var(--ink3);
    margin-bottom: 4px;
  }

  .result-reply {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
  }

  .provider-tabs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .prov-btn {
    text-align: left;
    padding: 9px 10px;
    border-radius: 8px;
    background: var(--win);
    border: 1px solid var(--line);
    color: var(--ink);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .prov-btn strong {
    font-size: 12px;
  }

  .prov-btn span {
    font-size: 10.5px;
    color: var(--ink2);
  }

  .prov-btn.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .byo-fields {
    margin-top: 11px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .byo-fields label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11.5px;
    color: var(--ink2);
  }

  .byo-fields input {
    background: var(--win);
    border: 1px solid var(--line2);
    border-radius: 7px;
    padding: 7px 10px;
    color: var(--ink);
    font-size: 12px;
    font-family: var(--mono);
  }

  .two-fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 9px;
  }

  .save-row {
    display: flex;
    justify-content: flex-end;
  }

  .tools-list {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    margin-top: 8px;
  }

  @media (max-width: 1180px) {
    .tools-list {
      grid-template-columns: 1fr;
    }
  }

  .tool-item {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--win);
    border: 1px solid var(--line);
  }

  .tool-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-bottom: 4px;
  }

  .tool-name {
    color: var(--ink);
    font-weight: 600;
  }

  .mode-tag {
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--cyan-soft);
    color: var(--accent-2);
  }

  .mode-tag.write {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tool-item p {
    margin: 0;
    font-size: 11px;
    color: var(--ink2);
    line-height: 1.35;
  }

  .rest-bar {
    margin-top: 10px;
    padding-top: 9px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--ink2);
  }

  .log-feed {
    max-height: 230px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-top: 6px;
  }

  .log-row {
    display: grid;
    grid-template-columns: 62px 58px 135px minmax(0, 1fr);
    gap: 7px;
    align-items: center;
    padding: 5px 8px;
    border-radius: 6px;
    background: var(--win);
    border: 1px solid var(--line);
    font-size: 11px;
  }

  .log-row.fail {
    border-color: color-mix(in srgb, var(--danger) 40%, var(--line));
  }

  .log-time {
    font-family: var(--mono);
    color: var(--ink3);
    font-size: 10px;
  }

  .log-src {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    color: var(--accent-2);
    text-align: center;
    background: var(--cyan-soft);
    border-radius: 4px;
    padding: 1px 4px;
  }

  .log-act {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .log-detail {
    color: var(--ink2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty-logs {
    margin: 6px 0;
    font-size: 12px;
    color: var(--ink3);
  }
</style>
