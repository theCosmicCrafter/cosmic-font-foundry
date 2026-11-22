<script>
  import { onMount, onDestroy } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { X, RefreshCw, Sparkles, Folder, Layers, Check, Search, Square, ShieldCheck, Archive } from "@lucide/svelte";
  import {
    fonts,
    installFontFiles,
    scanComputerFonts,
    cancelComputerScan,
    watchFolderByPath,
    invokeCmd,
  } from "../lib/fonts.svelte.js";
  import { autoCreateOsCollections, upsertCollectionWithFonts } from "../lib/library.svelte.js";
  import { settings, inTauri } from "../lib/settings.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let { onclose, onImported = null, onFoldersChanged = null } = $props();

  let scanning = $state(true);
  let busyAll = $state(false);
  let report = $state(null);
  let error = $state("");
  let activeDir = $state(null);
  let liveDirs = $state(0);
  let liveCurrentDir = $state("");
  let liveFolders = $state([]);
  let cancelling = $state(false);
  const importedPaths = new SvelteSet();
  const watchedPaths = new SvelteSet();
  const liveSeenPaths = new Set();
  const displayFolders = $derived(scanning ? liveFolders : (report?.discovered_folders ?? []));

  let unlistenProgress = null;

  async function runScan(customDir = activeDir) {
    activeDir = customDir || null;
    scanning = true;
    cancelling = false;
    liveDirs = 0;
    liveCurrentDir = activeDir ?? "";
    liveFolders = [];
    liveSeenPaths.clear();
    error = "";
    try {
      report = await scanComputerFonts(activeDir);
    } catch (e) {
      error = String(e);
    } finally {
      scanning = false;
      cancelling = false;
    }
  }

  onMount(async () => {
    if (inTauri) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenProgress = await listen("scan-progress", ({ payload }) => {
          if (!payload) return;
          liveDirs = payload.scanned_dirs ?? liveDirs;
          if (payload.current_dir) liveCurrentDir = payload.current_dir;
          if (payload.found_folder && !liveSeenPaths.has(payload.found_folder.path)) {
            liveSeenPaths.add(payload.found_folder.path);
            liveFolders = [...liveFolders, payload.found_folder];
          }
        });
      } catch {}
    }
    runScan();
  });

  onDestroy(() => {
    unlistenProgress?.();
  });

  function requestCancelScan() {
    if (!scanning || cancelling) return;
    cancelling = true;
    if (inTauri) cancelComputerScan().catch(() => {});
  }

  function organizeOsCollections() {
    const count = autoCreateOsCollections(fonts.list);
    notify("ok", t("scan.osOrganized", { n: count }), t("scan.osOrganizedDesc"));
  }

  async function pickAndScanFolder() {
    if (!inTauri) {
      await runScan();
      return;
    }
    const dir = await invokeCmd("sync_choose_folder").catch(() => null);
    if (dir) await runScan(dir);
  }

  async function watchFolder(folder) {
    try {
      const updated = await watchFolderByPath(folder.path);
      watchedPaths.add(folder.path);
      onFoldersChanged?.(updated);
      notify("ok", t("ok.folderAdded"), folder.label);
    } catch (e) {
      notify("err", t("err.one"), String(e));
    }
  }

  async function importAndCollect(folder) {
    if (!folder?.files?.length || importedPaths.has(folder.path)) return;
    const allUsers = settings.installFor === "all";
    const outcome = await installFontFiles(folder.files, allUsers);
    if (outcome && !outcome.cancelled) {
      upsertCollectionWithFonts(folder.label, folder.families);
      importedPaths.add(folder.path);
      notify("ok", t("scan.collected", { n: folder.families.length, c: folder.label }), "");
      onImported?.();
    }
  }

  async function importAllFolders() {
    if (!report?.discovered_folders?.length || busyAll) return;
    busyAll = true;
    try {
      let totalCols = 0;
      for (const folder of report.discovered_folders) {
        if (importedPaths.has(folder.path)) continue;
        const outcome = await installFontFiles(folder.files, settings.installFor === "all");
        if (outcome && !outcome.cancelled) {
          upsertCollectionWithFonts(folder.label, folder.families);
          importedPaths.add(folder.path);
          totalCols++;
        }
      }
      if (totalCols > 0) {
        notify("ok", t("scan.importedAll", { n: totalCols }), "");
        onImported?.();
      }
    } finally {
      busyAll = false;
    }
  }

  const LICENSE_LABELS = {
    ofl: "OFL",
    apache: "Apache",
    mit: "MIT",
    public_domain: "Public Domain",
    personal_only: "Personal Only",
    commercial: "Commercial",
    unknown: "Unknown",
  };

  let triageBusy = $state(false);
  let triageError = $state("");
  let triageTarget = $state("");
  let triageReport = $state(null);

  async function runTriage(pickCmd) {
    if (!inTauri || triageBusy) return;
    triageBusy = true;
    triageError = "";
    try {
      const p = await invokeCmd(pickCmd);
      if (!p) return;
      triageTarget = p;
      triageReport = await invokeCmd("foundry_triage_archive", { path: p });
    } catch (e) {
      triageError = String(e);
    } finally {
      triageBusy = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={t("scan.title")} tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <div class="head">
      <div class="title-wrap">
        <Search size={16} strokeWidth={2.2} />
        <div>
          <h2>{t("scan.title")}</h2>
          <p class="sub">{t("scan.subtitle")}</p>
        </div>
      </div>
      <button type="button" class="close" title={t("close")} aria-label={t("close")} onclick={onclose}>
        <X size={14} strokeWidth={2} />
      </button>
    </div>

    <div class="scroll">
      <!-- OS Default Font Environment -->
      <section class="os-card">
        <div class="os-head">
          <div class="os-meta">
            <span class="os-badge">{report ? `${report.os_name} · ${report.arch}` : t("scan.detectingOs")}</span>
            <span class="label">{t("scan.osSection")}</span>
          </div>
          <button type="button" class="btn primary" onclick={organizeOsCollections}>
            <Sparkles size={13} strokeWidth={2} />{t("scan.organizeOs")}
          </button>
        </div>

        {#if report}
          <div class="paths-grid">
            <div class="path-item">
              <span class="p-lbl">{t("scan.sysDir")} ({report.system_font_count})</span>
              <span class="p-val num">{report.system_fonts_dir}</span>
            </div>
            <div class="path-item">
              <span class="p-lbl">{t("scan.usrDir")} ({report.user_font_count})</span>
              <span class="p-val num">{report.user_fonts_dir}</span>
            </div>
          </div>
        {/if}
      </section>

      <!-- PC Discovered Folders -->
      <section class="scan-section">
        <div class="scan-head">
          <div class="scan-title-col">
            <span class="label">{t("scan.pcFolders")} · {displayFolders.length}</span>
            {#if activeDir}
              <span class="active-dir num" title={activeDir}>{activeDir}</span>
            {/if}
          </div>
          <div class="scan-actions">
            {#if activeDir}
              <button type="button" class="btn" onclick={() => runScan(null)} disabled={scanning}>
                {t("scan.rescan")}
              </button>
            {/if}
            <button type="button" class="btn" onclick={() => runScan(activeDir)} disabled={scanning}>
              <RefreshCw size={12} class={scanning ? "spin" : ""} />{activeDir ? "Rescan Folder" : t("scan.rescan")}
            </button>
            <button type="button" class="btn" onclick={pickAndScanFolder} disabled={scanning}>
              <Folder size={12} />{t("scan.pickDir")}
            </button>
            {#if scanning && inTauri}
              <button type="button" class="btn danger" onclick={requestCancelScan} disabled={cancelling}>
                <Square size={11} />{cancelling ? t("scan.cancelling") : t("scan.cancel")}
              </button>
            {/if}
            {#if report?.discovered_folders?.length && !scanning}
              <button type="button" class="btn accent" onclick={importAllFolders} disabled={busyAll || scanning}>
                <Layers size={12} />{t("scan.importAll")}
              </button>
            {/if}
          </div>
        </div>

        {#if scanning}
          <div class="scan-live">
            <span class="live-count num">{t("scan.scannedDirs", { n: liveDirs })}</span>
            <span class="live-path num" title={liveCurrentDir}>
              {liveCurrentDir || (activeDir ? `Scanning ${activeDir}…` : t("scan.scanning"))}
            </span>
          </div>
        {:else if report?.cancelled}
          <div class="scan-live warn">{t("scan.cancelledNote", { n: report.scanned_dirs ?? liveDirs })}</div>
        {/if}

        {#if error}
          <div class="err-box">{error}</div>
        {:else if !displayFolders.length}
          <div class="empty-scan">
            {#if scanning}
              {activeDir ? `Scanning ${activeDir} for font files…` : t("scan.scanning")}
            {:else}
              {activeDir ? `No font files found in ${activeDir}.` : t("scan.noneFound")}
            {/if}
          </div>
        {:else}
          <div class="folder-list">
            {#each displayFolders as folder (folder.path)}
              {@const isImported = importedPaths.has(folder.path)}
              {@const isWatched = watchedPaths.has(folder.path)}
              <div class="folder-row">
                <div class="f-info">
                  <div class="f-top">
                    <span class="f-name">{folder.label}</span>
                    <span class="f-kind">{folder.origin_kind}</span>
                    <span class="f-count num">{folder.font_count} {t("scan.files")}</span>
                  </div>
                  <div class="f-path num">{folder.path}</div>
                  {#if folder.families.length}
                    <div class="f-fams">
                      {#each folder.families.slice(0, 6) as fam (fam)}
                        <span class="fam-chip">{fam}</span>
                      {/each}
                      {#if folder.families.length > 6}
                        <span class="fam-chip more">+{folder.families.length - 6}</span>
                      {/if}
                    </div>
                  {/if}
                </div>
                <div class="f-actions">
                  <button type="button" class="btn sm" onclick={() => watchFolder(folder)} disabled={isWatched}>
                    {isWatched ? t("scan.watched") : t("scan.watch")}
                  </button>
                  <button
                    type="button"
                    class="btn sm"
                    class:primary={!isImported}
                    class:ok={isImported}
                    onclick={() => importAndCollect(folder)}
                    disabled={isImported}
                  >
                    {#if isImported}<Check size={12} strokeWidth={2.5} />{/if}
                    {isImported ? t("scan.imported") : t("scan.importCollect")}
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Safe Archive / Folder License Triage -->
      <section class="triage-card">
        <div class="triage-head">
          <div class="triage-title">
            <ShieldCheck size={14} strokeWidth={2.2} />
            <div class="triage-title-col">
              <span class="label">Safe Archive / Folder Triage</span>
              {#if triageTarget}
                <span class="triage-path num" title={triageTarget}>{triageTarget}</span>
              {/if}
            </div>
          </div>
          <div class="triage-actions">
            <button
              type="button"
              class="btn sm"
              onclick={() => runTriage("foundry_pick_triage_file")}
              disabled={!inTauri || triageBusy}
            >
              <Archive size={12} />Triage .ZIP / Font
            </button>
            <button
              type="button"
              class="btn sm"
              onclick={() => runTriage("sync_choose_folder")}
              disabled={!inTauri || triageBusy}
            >
              <Folder size={12} />Triage Folder
            </button>
          </div>
        </div>

        {#if triageBusy}
          <div class="triage-status">Scanning entries, validating archive safety, and hashing outlines…</div>
        {/if}

        {#if triageError}
          <div class="err-box">{triageError}</div>
        {:else if triageReport}
          <div class="triage-summary">
            <span class="t-pill num">Scanned: <strong>{triageReport.scanned_files}</strong></span>
            <span class="t-pill num" class:t-pill-danger={triageReport.rejected_unsafe > 0}>
              Rejected Unsafe: <strong>{triageReport.rejected_unsafe}</strong>
            </span>
            <span class="t-pill num" class:t-pill-warn={triageReport.duplicate_count > 0}>
              Outline Duplicates: <strong>{triageReport.duplicate_count}</strong>
            </span>
          </div>

          {#if !triageReport.fonts?.length}
            <div class="triage-empty">No valid font files found in target.</div>
          {:else}
            <div class="triage-list">
              {#each triageReport.fonts as font, idx (`${font.source_entry}-${idx}`)}
                {@const lic = font.license_class ?? "unknown"}
                <div class="triage-row">
                  <div class="t-main">
                    <span class="t-fam" title={font.source_entry}>{font.family}</span>
                    <span class="t-sub">{font.subfamily}</span>
                  </div>
                  <div class="t-meta">
                    <span
                      class="lic-pill"
                      class:lic-ok={lic === "ofl" || lic === "mit"}
                      class:lic-open={lic === "apache" || lic === "public_domain"}
                      class:lic-warn={lic === "personal_only"}
                      class:lic-danger={lic === "commercial"}
                      class:lic-muted={lic === "unknown"}
                      title={font.license_summary || lic}
                    >
                      {LICENSE_LABELS[lic] ?? lic}
                    </span>
                    {#if font.is_duplicate}
                      <span class="dup-tag">DUPLICATE</span>
                    {/if}
                    <span class="digest num" title={font.outline_digest}>
                      {(font.outline_digest ?? "").slice(0, 6)}
                    </span>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </section>
    </div>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; z-index: 25; background: var(--velo); backdrop-filter: blur(5px); display: flex; align-items: center; justify-content: center; animation: cf-fade 0.16s ease-out; }
  .modal { width: 760px; max-width: calc(100vw - 40px); max-height: 82vh; border-radius: 14px; background: var(--win); color: var(--ink); box-shadow: var(--float-shadow); overflow: hidden; display: flex; flex-direction: column; }
  .head { flex: none; display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; border-bottom: 1px solid var(--line); }
  .title-wrap { display: flex; align-items: center; gap: 12px; color: var(--accent-2); }
  .title-wrap h2 { margin: 0; font-size: 16px; font-weight: 700; color: var(--ink); }
  .sub { margin: 2px 0 0; font-size: 12px; color: var(--ink2); }
  .close { width: 28px; height: 28px; border: 0; border-radius: 8px; background: transparent; color: var(--ink3); display: grid; place-items: center; }
  .close:hover { background: var(--hov); color: var(--ink); }

  .scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 18px 20px 22px; display: flex; flex-direction: column; gap: 18px; }

  .os-card { padding: 14px 16px; border-radius: 12px; background: var(--side); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 12px; }
  .os-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .os-meta { display: flex; align-items: center; gap: 10px; }
  .os-badge { font: 700 10px var(--mono); padding: 3px 8px; border-radius: 999px; background: var(--cyan-soft); color: var(--accent-2); }
  .paths-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 10px; }
  .path-item { padding: 8px 10px; border-radius: 8px; background: var(--field); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .p-lbl { font-size: 11px; font-weight: 600; color: var(--ink2); }
  .p-val { font: 400 11px var(--mono); color: var(--ink3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .scan-section { display: flex; flex-direction: column; gap: 10px; }
  .scan-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .scan-title-col { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .active-dir { font: 500 11px var(--mono); color: var(--accent-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 340px; }
  .scan-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }

  .btn {
    height: 28px; padding: 0 11px; border-radius: 8px; border: 1px solid var(--line2);
    background: var(--field); color: var(--ink); font: 600 11.5px var(--sans);
    display: inline-flex; align-items: center; gap: 6px; transition: all 0.14s;
  }
  .btn:hover:not(:disabled) { background: var(--chip); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; box-shadow: var(--glow); }
  .btn.accent { background: var(--cyan-soft); border-color: var(--accent-2); color: var(--accent-2); }
  .btn.ok { background: rgba(45, 212, 191, 0.14); border-color: var(--ok); color: var(--ok); }
  .btn.danger { background: rgba(251, 113, 133, 0.14); border-color: rgba(251, 113, 133, 0.45); color: var(--danger); }
  .btn.sm { height: 26px; padding: 0 10px; font-size: 11px; }
  .btn:disabled { opacity: 0.6; cursor: default; }

  .scan-live {
    padding: 8px 12px; border-radius: 9px; background: var(--field); border: 1px solid var(--line);
    display: flex; align-items: center; gap: 10px; font-size: 11.5px; color: var(--ink2);
  }
  .scan-live.warn { border-color: rgba(251, 191, 36, 0.4); color: var(--ink); }
  .live-count { font-weight: 700; color: var(--accent-2); flex: none; }
  .live-path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink3); font-size: 11px; }

  .folder-list { display: flex; flex-direction: column; gap: 8px; }
  .folder-row {
    padding: 12px 14px; border-radius: 10px; background: var(--side); border: 1px solid var(--line);
    display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap;
  }
  .f-info { flex: 1; min-width: 240px; display: flex; flex-direction: column; gap: 4px; }
  .f-top { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .f-name { font-size: 13px; font-weight: 700; color: var(--ink); }
  .f-kind { font: 600 10px var(--mono); padding: 2px 6px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); }
  .f-count { font-size: 11px; color: var(--ink3); }
  .f-path { font: 400 11px var(--mono); color: var(--ink3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .f-fams { display: flex; flex-wrap: wrap; gap: 4px; padding-top: 2px; }
  .fam-chip { font-size: 10.5px; padding: 2px 6px; border-radius: 5px; background: var(--field); border: 1px solid var(--line); color: var(--ink2); }
  .fam-chip.more { color: var(--accent-2); }
  .f-actions { display: flex; align-items: center; gap: 6px; flex: none; }

  .empty-scan { padding: 32px 16px; text-align: center; font-size: 12.5px; color: var(--ink3); border-radius: 10px; background: var(--side); border: 1px dashed var(--line2); }
  .err-box { padding: 10px 12px; border-radius: 8px; background: rgba(251, 113, 133, 0.14); color: var(--danger); font-size: 12px; }

  .triage-card { padding: 12px 14px; border-radius: 12px; background: var(--side); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 10px; }
  .triage-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .triage-title { display: flex; align-items: center; gap: 8px; color: var(--accent-2); min-width: 0; }
  .triage-title-col { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .triage-path { font: 400 11px var(--mono); color: var(--ink3); max-width: 340px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .triage-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .triage-status { font-size: 11.5px; color: var(--ink2); padding: 6px 10px; border-radius: 8px; background: var(--field); border: 1px solid var(--line); }
  .triage-summary { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .t-pill { font: 500 11px var(--mono); padding: 3px 8px; border-radius: 999px; background: var(--field); border: 1px solid var(--line); color: var(--ink2); }
  .t-pill-danger { background: rgba(251, 113, 133, 0.14); border-color: rgba(251, 113, 133, 0.4); color: var(--danger); }
  .t-pill-warn { background: rgba(251, 191, 36, 0.14); border-color: rgba(251, 191, 36, 0.4); color: var(--warn); }
  .triage-empty { padding: 12px; text-align: center; font-size: 11.5px; color: var(--ink3); border-radius: 8px; background: var(--field); border: 1px dashed var(--line2); }
  .triage-list { max-height: 210px; overflow-y: auto; display: flex; flex-direction: column; gap: 5px; padding-right: 2px; }
  .triage-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 6px 10px; border-radius: 8px; background: var(--field); border: 1px solid var(--line); }
  .t-main { display: flex; align-items: baseline; gap: 6px; min-width: 0; flex: 1; }
  .t-fam { font-size: 12px; font-weight: 600; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t-sub { font-size: 11px; color: var(--ink3); flex: none; }
  .t-meta { display: flex; align-items: center; gap: 6px; flex: none; }
  .lic-pill { font: 700 10px var(--mono); padding: 2px 7px; border-radius: 999px; border: 1px solid transparent; text-transform: uppercase; }
  .lic-ok { background: rgba(45, 212, 191, 0.14); border-color: rgba(45, 212, 191, 0.35); color: var(--ok); }
  .lic-open { background: var(--cyan-soft); border-color: rgba(56, 189, 248, 0.35); color: var(--accent-2); }
  .lic-warn { background: rgba(251, 191, 36, 0.14); border-color: rgba(251, 191, 36, 0.4); color: var(--warn); }
  .lic-danger { background: rgba(251, 113, 133, 0.14); border-color: rgba(251, 113, 133, 0.4); color: var(--danger); }
  .lic-muted { background: var(--side); border-color: var(--line2); color: var(--ink3); }
  .dup-tag { font: 700 9.5px var(--mono); padding: 2px 6px; border-radius: 4px; background: rgba(251, 191, 36, 0.14); color: var(--warn); }
  .digest { font: 500 10.5px var(--mono); color: var(--ink3); min-width: 46px; text-align: right; }

  :global(.spin) { animation: cf-spin 0.9s linear infinite; }
  @keyframes cf-spin { to { transform: rotate(360deg); } }
</style>
