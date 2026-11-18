<script>
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { X, RefreshCw, Sparkles, Folder, Layers, Check, Search } from "@lucide/svelte";
  import {
    fonts,
    installFontFiles,
    scanComputerFonts,
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
  const importedPaths = new SvelteSet();
  const watchedPaths = new SvelteSet();

  async function runScan(customDir = null) {
    scanning = true;
    error = "";
    try {
      report = await scanComputerFonts(customDir);
    } catch (e) {
      error = String(e);
    } finally {
      scanning = false;
    }
  }

  onMount(() => {
    runScan();
  });

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
          <span class="label">{t("scan.pcFolders")} {#if report}· {report.discovered_folders.length}{/if}</span>
          <div class="scan-actions">
            <button type="button" class="btn" onclick={() => runScan()} disabled={scanning}>
              <RefreshCw size={12} class={scanning ? "spin" : ""} />{t("scan.rescan")}
            </button>
            <button type="button" class="btn" onclick={pickAndScanFolder} disabled={scanning}>
              <Folder size={12} />{t("scan.pickDir")}
            </button>
            {#if report?.discovered_folders?.length}
              <button type="button" class="btn accent" onclick={importAllFolders} disabled={busyAll || scanning}>
                <Layers size={12} />{t("scan.importAll")}
              </button>
            {/if}
          </div>
        </div>

        {#if error}
          <div class="err-box">{error}</div>
        {:else if scanning}
          <div class="empty-scan">{t("scan.scanning")}</div>
        {:else if !report?.discovered_folders?.length}
          <div class="empty-scan">{t("scan.noneFound")}</div>
        {:else}
          <div class="folder-list">
            {#each report.discovered_folders as folder (folder.path)}
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
  .btn.sm { height: 26px; padding: 0 10px; font-size: 11px; }
  .btn:disabled { opacity: 0.6; cursor: default; }

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
  :global(.spin) { animation: cf-spin 0.9s linear infinite; }
  @keyframes cf-spin { to { transform: rotate(360deg); } }
</style>
