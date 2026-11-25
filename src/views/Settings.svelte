<script>
  import { SlidersHorizontal, RefreshCw, Keyboard, Info, X, ArrowUpRight, FolderSync, Download, Upload, Sparkles, Bot, HardDrive, FolderOpen, Trash2 } from "@lucide/svelte";
  import SegmentedControl from "../components/SegmentedControl.svelte";
  import AgentHub from "./AgentHub.svelte";
  import { settings, currentTheme, setTheme, setLanguage, version, DEFAULT_SHORTCUTS, inTauri } from "../lib/settings.svelte.js";
  import { t } from "../lib/i18n.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { backedUpCount, restoreSystemFonts, fonts, invokeCmd, applyExternalList, revealInExplorer } from "../lib/fonts.svelte.js";
  import { snapshotLibrary, applyLibrarySnapshot } from "../lib/library.svelte.js";
  import { shortcutCombo } from "../lib/shortcuts.js";
  import { updater, checkForUpdates } from "../lib/updater.svelte.js";

  let { tab = $bindable(), onclose, counts, onMigrated = null } = $props();

  const openWeb = async (target) => {
    if (!inTauri) return;
    invokeCmd("open_web", { target }).catch(() => {});
  };

  const updateHint = $derived(
    updater.status === "checking"
      ? t("upd.checking")
      : updater.status === "latest"
        ? t("upd.upToDate")
        : updater.status === "available" || updater.status === "downloading" || updater.status === "installing"
          ? t("upd.available", { v: updater.version })
          : updater.status === "error"
            ? t("upd.unreachable")
            : t("set.updatesHint"),
  );

  const TABS = $derived([
    { id: "settings", label: () => t("settings"), icon: SlidersHorizontal },
    { id: "sync", label: () => t("syncAndExport"), icon: FolderSync },
    { id: "shortcuts", label: () => t("shortcuts"), icon: Keyboard },
    { id: "about", label: () => t("about"), icon: Info },
    ...(settings.showAgentTab || tab === "agent"
      ? [{ id: "agent", label: () => t("sec.agent"), icon: Bot, badge: "MCP" }]
      : []),
  ]);

  // Windows Startup
  let winStartup = $state(false);
  if (inTauri) {
    invokeCmd("windows_startup").then((v) => (winStartup = v)).catch(() => {});
  }
  async function toggleStartup(enable) {
    if (!inTauri) return;
    try {
      winStartup = await invokeCmd("set_windows_startup", { enable });
      if (winStartup !== enable) notify("err", t("err.startup"), "");
    } catch (e) {
      notify("err", t("err.startup"), String(e));
    }
  }

  let backedUp = $state(0);
  backedUpCount().then((n) => (backedUp = n)).catch(() => {});

  function unlockSystem(v) {
    settings.systemUnlocked = v;
    if (v) notify("warn", t("warn.unlocked"), t("warn.unlockedText"));
    else notify("ok", t("ok.locked"), t("ok.lockedText"));
  }

  async function restoreAllSystem() {
    const o = await restoreSystemFonts();
    backedUp = await backedUpCount().catch(() => 0);
    if (o && o.changed) notify("ok", t("ok.restored"), o.reboot_required ? t("warn.rebootText") : "");
  }

  const rows = $derived([
    { label: t("set.theme"), hint: t("set.themeHint"), value: currentTheme(), select: setTheme, options: [["light", t("set.light")], ["dark", t("set.dark")]] },
    { label: t("set.install"), hint: t("set.installHint"), value: settings.installFor, select: (v) => (settings.installFor = v), options: [["user", t("set.justMe")], ["all", t("set.everyone")]] },
    { label: t("set.onOpen"), hint: t("set.onOpenHint"), value: settings.onOpen, select: (v) => (settings.onOpen = v), options: [["installed", t("sec.installed")], ["favorites", t("sec.favorites")]] },
    { label: t("set.onClose"), hint: t("set.onCloseHint"), value: settings.onClose, select: (v) => (settings.onClose = v), options: [["tray", t("set.tray")], ["exit", t("set.quit")]] },
    { label: t("set.startup"), hint: t("set.startupHint"), value: winStartup, select: toggleStartup, options: [[true, t("set.yes")], [false, t("set.no")]] },
    { label: t("set.system"), hint: t("set.systemHint"), value: settings.systemUnlocked, select: unlockSystem, options: [[false, t("set.locked")], [true, t("set.unlock")]] },
    { label: t("set.showLocked"), hint: t("set.showLockedHint"), value: settings.showLocked, select: (v) => (settings.showLocked = v), options: [[false, t("set.hidden")], [true, t("set.shown")]] },
    { label: t("set.checkUpdates"), hint: t("set.checkUpdatesHint"), value: settings.checkUpdates, select: (v) => (settings.checkUpdates = v), options: [[true, t("set.yes")], [false, t("set.no")]] },
    { label: t("set.agentTab"), hint: t("set.agentTabHint"), value: settings.showAgentTab, select: (v) => (settings.showAgentTab = v), options: [[false, t("set.hidden")], [true, t("set.shown")]] },
    { label: t("set.language"), hint: "", value: settings.language, select: setLanguage, options: [["en", "English"], ["es", "Español"]] },
  ]);

  // ---- BYO-Sync, Portability & tuna-fonts Migration ----
  let syncBusy = $state(false);
  let unresolvedSync = $state([]);
  let tunaStatus = $state({ available: false });
  let migrating = $state(false);

  let vaultStats = $state(null);
  let vaultBusy = $state(false);
  let cleanupScan = $state(null);
  let cleanupSelected = $state(new Set());
  let artCraftStatus = $state(null);
  let artCraftBusy = $state(false);
  let syncTabLoaded = false;

  $effect(() => {
    if (inTauri && tab === "sync" && !syncTabLoaded) {
      syncTabLoaded = true;
      invokeCmd("detect_tuna_migration").then((s) => (tunaStatus = s)).catch(() => {});
      invokeCmd("vault_stats").then((s) => (vaultStats = s)).catch(() => {});
      invokeCmd("artcraft_store_status").then((s) => (artCraftStatus = s)).catch(() => {});
    }
  });

  async function watchArtCraftStore() {
    if (!inTauri || artCraftBusy) return;
    artCraftBusy = true;
    try {
      const res = await invokeCmd("watch_artcraft_store");
      if (res?.families) applyExternalList(res.families, res.diagnostics);
      artCraftStatus = await invokeCmd("artcraft_store_status");
      onMigrated?.();
      notify("ok", "ArtCraft Store Linked", res?.store_path || "Watching Craft Fonts folder.");
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      artCraftBusy = false;
    }
  }

  function formatBytes(bytes) {
    if (!bytes) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    let v = bytes;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i += 1;
    }
    return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
  }

  async function changeVaultFolder() {
    if (!inTauri || vaultBusy) return;
    vaultBusy = true;
    try {
      const stats = await invokeCmd("choose_vault_folder");
      if (stats) {
        vaultStats = stats;
        onMigrated?.();
      }
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      vaultBusy = false;
    }
  }

  async function importFolderToVault(recycleOriginals = false) {
    if (!inTauri || vaultBusy) return;
    vaultBusy = true;
    try {
      const res = await invokeCmd("import_folder_to_vault", {
        moveFiles: false,
        recycleOriginals,
      });
      if (!res) return;
      vaultStats = res.stats;
      applyExternalList(res.families, res.diagnostics);
      onMigrated?.();
      notify(
        "ok",
        t("vault.imported"),
        t("vault.importedDesc", {
          n: res.report.imported,
          dup: res.report.skipped_duplicates,
          size: formatBytes(res.report.bytes_copied),
        }) + (res.report.recycled ? ` · ${res.report.recycled} recycled` : ""),
      );
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      vaultBusy = false;
    }
  }

  async function scanCleanup(customFolder = false) {
    if (!inTauri || vaultBusy) return;
    vaultBusy = true;
    try {
      const res = await invokeCmd("scan_vault_cleanup", { customFolder });
      if (!res) return;
      cleanupScan = res;
      cleanupSelected = new Set(res.candidates.map((c) => c.source_path));
      if (!res.candidates.length) {
        notify("ok", t("vault.cleanupNone"), "");
      }
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      vaultBusy = false;
    }
  }

  function toggleCleanupPath(p) {
    const next = new Set(cleanupSelected);
    if (next.has(p)) next.delete(p);
    else next.add(p);
    cleanupSelected = next;
  }

  async function recycleCheckedDuplicates() {
    if (!inTauri || vaultBusy || !cleanupSelected.size) return;
    vaultBusy = true;
    try {
      const paths = [...cleanupSelected];
      const report = await invokeCmd("recycle_vault_duplicates", { paths });
      notify(
        "ok",
        t("vault.recycled"),
        t("vault.recycledDesc", {
          n: report.recycled,
          size: formatBytes(report.reclaimed_bytes),
        }),
      );
      cleanupScan = null;
      cleanupSelected = new Set();
      vaultStats = await invokeCmd("vault_stats").catch(() => vaultStats);
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      vaultBusy = false;
    }
  }

  async function chooseSyncFolder() {
    if (!inTauri) return;
    const dir = await invokeCmd("sync_choose_folder").catch(() => null);
    if (!dir) return;
    settings.syncFolder = dir;
    await runSync(true);
  }

  async function runSync(pushLocal = true) {
    if (!inTauri || !settings.syncFolder) return;
    syncBusy = true;
    try {
      const res = await invokeCmd("sync_manifest_now", {
        syncDir: settings.syncFolder,
        library: snapshotLibrary(),
        pushLocal,
      });
      applyLibrarySnapshot(res.library);
      settings.lastSyncAt = res.report.updated_at;
      unresolvedSync = res.report.unresolved_fonts ?? [];
      notify("ok", t("sync.ok"), t("sync.okDesc", { fav: res.report.matched_favorites, col: res.report.matched_collections }));
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      syncBusy = false;
    }
  }

  async function exportProfile() {
    if (!inTauri) return;
    try {
      const saved = await invokeCmd("export_profile_bundle", { library: snapshotLibrary() });
      if (saved) notify("ok", t("sync.ok"), saved);
    } catch (e) {
      notify("err", t("err.one"), String(e));
    }
  }

  async function importProfile() {
    if (!inTauri) return;
    try {
      const res = await invokeCmd("import_profile_bundle", { library: snapshotLibrary() });
      if (!res) return;
      applyLibrarySnapshot(res.library);
      unresolvedSync = res.report.unresolved_fonts ?? [];
      notify("ok", t("sync.ok"), t("sync.okDesc", { fav: res.report.matched_favorites, col: res.report.matched_collections }));
    } catch (e) {
      notify("err", t("err.one"), String(e));
    }
  }

  async function runTunaMigration() {
    if (!inTauri || migrating) return;
    migrating = true;
    try {
      const res = await invokeCmd("run_tuna_migration");
      applyLibrarySnapshot(res.library);
      applyExternalList(res);
      onMigrated?.();
      tunaStatus = { ...tunaStatus, available: false };
      notify(
        "ok",
        t("sync.migrateDone"),
        t("sync.migrateDoneDesc", {
          fav: res.report.imported_favorites,
          col: res.report.imported_collections,
          dir: res.report.imported_folders,
        }),
      );
    } catch (e) {
      notify("err", t("err.one"), String(e));
    } finally {
      migrating = false;
    }
  }

  // ---- Shortcuts ----
  let recording = $state(null);
  const prettyKey = (k) => (k === "Space" ? t("keys.space") : k);
  const shortcutRows = $derived([
    { id: "search", desc: t("keys.search") },
    { id: "all", desc: t("keys.all") },
    { id: "toggle", desc: t("keys.toggle") },
    { id: "quickSwitcher", desc: t("keys.quick"), global: true },
    { fixed: t("keys.doubleClick"), desc: t("keys.double") },
    { fixed: "Esc", desc: t("keys.esc") },
  ]);
  const shortcutsChanged = $derived(JSON.stringify(settings.shortcuts) !== JSON.stringify(DEFAULT_SHORTCUTS));

  async function applyShortcutCombo(id, combo) {
    if (id === "quickSwitcher") {
      settings.quickSwitcherShortcut = combo;
      if (inTauri) {
        try {
          await invokeCmd("set_global_shortcut", { shortcut: combo });
        } catch (e) {
          notify("err", t("err.one"), String(e));
        }
      }
    }
  }

  function onKeyCapture(e) {
    if (recording) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return (recording = null);
      const combo = shortcutCombo(e);
      if (!combo) return;
      const collision = Object.keys(settings.shortcuts).find((k) => k !== recording && settings.shortcuts[k] === combo);
      const prev = settings.shortcuts[recording];
      settings.shortcuts = { ...settings.shortcuts, [recording]: combo, ...(collision ? { [collision]: prev } : {}) };
      const targetId = recording;
      recording = null;
      applyShortcutCombo(targetId, combo);
      if (collision) applyShortcutCombo(collision, prev);
      notify("ok", t("ok.shortcut", { c: combo.split("+").map(prettyKey).join("+") }), collision ? t("ok.shortcutSwapped") : "");
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }

  function resetShortcuts() {
    settings.shortcuts = { ...DEFAULT_SHORTCUTS };
    applyShortcutCombo("quickSwitcher", DEFAULT_SHORTCUTS.quickSwitcher);
    recording = null;
    notify("ok", t("ok.shortcutsReset"), "");
  }

  const highlights = $derived([t("hl.1"), t("hl.2"), t("hl.3"), t("hl.4")]);
</script>

<svelte:window onkeydowncapture={onKeyCapture} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" class:wide={tab === "agent"} role="dialog" aria-modal="true" aria-label={t("settings")} tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <div class="side">
      <span class="brand">Font Foundry<i>.</i></span>
      {#each TABS as tb (tb.id)}
        <button type="button" class="tab" class:active={tab === tb.id} onclick={() => ((tab = tb.id), (recording = null))}>
          <tb.icon size={14} strokeWidth={1.9} />
          <span class="tab-lbl">{tb.label()}</span>
          {#if tb.badge}<span class="tab-badge">{tb.badge}</span>{/if}
        </button>
      {/each}
      <div class="side-spacer"></div>
      {#if !(settings.showAgentTab || tab === "agent")}
        <button
          type="button"
          class="secret-tab-btn"
          title={t("set.agentTabHint")}
          onclick={() => {
            settings.showAgentTab = true;
            tab = "agent";
            recording = null;
          }}
        >
          <Bot size={12} strokeWidth={1.8} />
          <span>{t("set.mcpSecretBtn")}</span>
        </button>
      {/if}
    </div>

    <div class="body">
      <div class="head">
        <h2>{tab === "settings" ? t("settings") : tab === "sync" ? t("syncAndExport") : tab === "shortcuts" ? t("shortcuts") : tab === "agent" ? t("sec.agent") : t("about")}</h2>
        <button type="button" class="close" title={t("close")} aria-label={t("close")} onclick={onclose}>
          <X size={13} strokeWidth={2} />
        </button>
      </div>

      <div class="scroll">
        {#if tab === "settings"}
          {#each rows as r (r.label)}
            <div class="row">
              <span class="txt">
                <span class="lbl">{r.label}</span>
                {#if r.hint}<span class="hint">{r.hint}</span>{/if}
              </span>
              <SegmentedControl label={r.label} options={r.options.map(([value, label]) => ({ value, label }))} value={r.value} onselect={r.select} />
            </div>
          {/each}

          <div class="row">
            <span class="txt">
              <span class="lbl">{t("set.updates")}</span>
              <span class="hint">{updateHint}</span>
            </span>
            {#if updater.status === "available" && !updater.dismissed}
              <button type="button" class="btn" onclick={onclose}>{t("upd.seeNotice")}</button>
            {:else}
              <button type="button" class="btn" disabled={updater.status === "checking" || !inTauri} onclick={() => checkForUpdates(true)}>
                {t("upd.checkNow")}
              </button>
            {/if}
          </div>

          {#if backedUp > 0}
            <div class="row">
              <span class="txt">
                <span class="lbl">{t("set.restoreAll")}</span>
                <span class="hint">{t("set.restoreAllHint", { n: backedUp })}</span>
              </span>
              <button type="button" class="btn" disabled={fonts.busy} onclick={restoreAllSystem}>{t("set.restoreAll")}</button>
            </div>
          {/if}

          <div class="sample-field">
            <span class="lbl">{t("defSample")}</span>
            <input bind:value={settings.sampleText} placeholder={t("defaultSample")} spellcheck="false" />
          </div>
        {:else if tab === "sync"}
          <div class="card-block">
            <div class="card-top">
              <div>
                <div class="lbl row-title"><HardDrive size={14} />{t("vault.title")}</div>
                <p class="hint">{t("vault.hint")}</p>
              </div>
            </div>
            <div class="sync-path num">{vaultStats?.path || "—"}</div>
            {#if vaultStats}
              <div class="vault-metrics num">
                <span>{t("vault.files", { n: vaultStats.total_files })}</span>
                <span>·</span>
                <span>{t("vault.families", { n: vaultStats.total_families })}</span>
                <span>·</span>
                <span>{formatBytes(vaultStats.total_bytes)}</span>
                {#if vaultStats.disk_free_bytes != null}
                  <span>·</span>
                  <span>{t("vault.free", { size: formatBytes(vaultStats.disk_free_bytes) })}</span>
                {/if}
              </div>
            {/if}
            <div class="btn-row">
              <button type="button" class="btn accent" disabled={vaultBusy} onclick={() => importFolderToVault(false)}>
                <Download size={12} />{t("vault.importCopy")}
              </button>
              <button type="button" class="btn" disabled={vaultBusy} onclick={() => importFolderToVault(true)}>
                <Trash2 size={12} />{t("vault.importRecycle")}
              </button>
              <button type="button" class="btn" disabled={vaultBusy} onclick={changeVaultFolder}>{t("vault.change")}</button>
              {#if vaultStats?.exists}
                <button type="button" class="btn ghost" onclick={() => revealInExplorer(vaultStats.path)}>
                  <FolderOpen size={12} />{t("vault.open")}
                </button>
              {/if}
            </div>
            <div class="btn-row">
              <button type="button" class="btn" disabled={vaultBusy} onclick={() => scanCleanup(false)}>
                <Trash2 size={12} />{t("vault.cleanupDefault")}
              </button>
              <button type="button" class="btn ghost" disabled={vaultBusy} onclick={() => scanCleanup(true)}>
                {t("vault.cleanupPick")}
              </button>
            </div>
            {#if cleanupScan?.candidates?.length}
              <div class="cleanup-box">
                <div class="cleanup-head">
                  <span>{t("vault.cleanupFound", { n: cleanupScan.candidates.length, size: formatBytes(cleanupScan.total_reclaimable_bytes) })}</span>
                  <button type="button" class="btn accent" disabled={vaultBusy || !cleanupSelected.size} onclick={recycleCheckedDuplicates}>
                    <Trash2 size={12} />{t("vault.recycleSelected", { n: cleanupSelected.size })}
                  </button>
                </div>
                <div class="cleanup-list">
                  {#each cleanupScan.candidates.slice(0, 60) as c (c.source_path)}
                    <label class="cleanup-row">
                      <input type="checkbox" checked={cleanupSelected.has(c.source_path)} onchange={() => toggleCleanupPath(c.source_path)} />
                      <span class="cleanup-fam">{c.family}</span>
                      <span class="cleanup-path num" title={c.source_path}>{c.source_path}</span>
                      <span class="num">{formatBytes(c.size_bytes)}</span>
                    </label>
                  {/each}
                </div>
              </div>
            {/if}
          </div>

          <div class="card-block">
            <div class="card-top">
              <div>
                <div class="lbl">{t("sync.title")}</div>
                <p class="hint">{t("sync.hint")}</p>
              </div>
            </div>
            <div class="sync-path num">{settings.syncFolder || t("sync.none")}</div>
            {#if settings.lastSyncAt}
              <div class="hint">{t("sync.last", { time: new Date(settings.lastSyncAt * 1000).toLocaleString() })}</div>
            {/if}
            <div class="btn-row">
              <button type="button" class="btn accent" onclick={chooseSyncFolder}>{t("sync.choose")}</button>
              {#if settings.syncFolder}
                <button type="button" class="btn" disabled={syncBusy} onclick={() => runSync(true)}>
                  <RefreshCw size={12} />{t("sync.now")}
                </button>
                <button type="button" class="btn ghost" onclick={() => (settings.syncFolder = "")}>{t("sync.clear")}</button>
              {/if}
            </div>
            {#if unresolvedSync.length}
              <div class="warn-note">
                <b>{t("sync.unresolved", { n: unresolvedSync.length })}</b>
                <span>{unresolvedSync.slice(0, 6).join(", ")}{unresolvedSync.length > 6 ? "…" : ""}</span>
              </div>
            {/if}
          </div>

          <div class="card-block">
            <div class="card-top">
              <div>
                <div class="lbl row-title"><Sparkles size={14} />ArtCraft Suite Font Bridge</div>
                <p class="hint">
                  Sync fonts directly with ArtCraft Launcher &amp; Craft apps ({artCraftStatus?.artcraft_installed ? "ArtCraft detected" : "shared Craft Fonts folder"}).
                </p>
              </div>
            </div>
            <div class="sync-path num">{artCraftStatus?.path || "—"}</div>
            {#if artCraftStatus}
              <div class="vault-metrics num">
                <span>{artCraftStatus.font_files} font file(s)</span>
                <span>·</span>
                <span>{artCraftStatus.watched ? "Watched by Foundry" : "Not watched yet"}</span>
              </div>
            {/if}
            <div class="btn-row">
              <button
                type="button"
                class="btn accent"
                disabled={artCraftBusy || artCraftStatus?.watched}
                onclick={watchArtCraftStore}
              >
                <FolderOpen size={12} />{artCraftStatus?.watched ? "Watching Craft Fonts" : "Watch Craft Fonts"}
              </button>
              {#if artCraftStatus?.exists}
                <button type="button" class="btn ghost" onclick={() => revealInExplorer(artCraftStatus.path)}>
                  <FolderOpen size={12} />{t("vault.open")}
                </button>
              {/if}
            </div>
          </div>

          <div class="card-block">
            <div class="lbl">{t("sync.portableTitle")}</div>
            <p class="hint">{t("sync.portableHint")}</p>
            <div class="btn-row">
              <button type="button" class="btn" onclick={exportProfile}><Download size={12} />{t("sync.exportBtn")}</button>
              <button type="button" class="btn" onclick={importProfile}><Upload size={12} />{t("sync.importBtn")}</button>
            </div>
          </div>

          {#if tunaStatus.available}
            <div class="card-block highlight">
              <div class="lbl row-title"><Sparkles size={14} />{t("sync.migrateTitle")}</div>
              <p class="hint">
                {t("sync.migrateDesc", {
                  fav: tunaStatus.favorites_count ?? 0,
                  col: tunaStatus.collections_count ?? 0,
                  dir: tunaStatus.folders_count ?? 0,
                })}
              </p>
              <div class="btn-row">
                <button type="button" class="btn accent" disabled={migrating} onclick={runTunaMigration}>
                  {t("sync.migrateBtn")}
                </button>
              </div>
            </div>
          {/if}
        {:else if tab === "shortcuts"}
          <span class="hint">{t("keys.hint")}</span>
          <div class="shortcuts">
            {#each shortcutRows as s (s.desc)}
              {#if s.id}
                <button
                  type="button"
                  class="keys"
                  class:recording={recording === s.id}
                  title={t("keys.change")}
                  onclick={() => (recording = recording === s.id ? null : s.id)}
                >
                  {#if recording === s.id}
                    <kbd>{t("keys.press")}</kbd>
                  {:else}
                    {#each (settings.shortcuts[s.id] ?? DEFAULT_SHORTCUTS[s.id]).split("+") as k (k)}
                      <kbd>{prettyKey(k)}</kbd>
                    {/each}
                  {/if}
                </button>
              {:else}
                <span class="keys fixed" title={t("keys.fixed")}><kbd>{s.fixed}</kbd></span>
              {/if}
              <span class="desc">{s.desc}</span>
            {/each}
          </div>
          {#if shortcutsChanged}
            <button type="button" class="btn self-start" onclick={resetShortcuts}>{t("keys.reset")}</button>
          {/if}
        {:else if tab === "agent"}
          <AgentHub {notify} embedded />
        {:else}
          <div class="about-head">
            <img src="/icon.svg" width="64" height="64" alt="" draggable="false" />
            <div>
              <span class="brand big">Cosmic Font Foundry<i>.</i></span>
              <span class="ver">{t("version", { v: version })}</span>
            </div>
          </div>
          <p class="tagline">{t("tagline")}</p>

          <div class="metrics">
            {#each [[counts.installed, t("about.installed")], [counts.active, t("about.active")], [counts.favorites, t("about.favorites")]] as [val, lbl] (lbl)}
              <div class="metric"><span class="n num">{val}</span><span class="l">{lbl}</span></div>
            {/each}
          </div>

          <div class="highlights">
            <span class="label">{t("whatsNew", { v: version })}</span>
            {#each highlights as h (h)}
              <div class="hl-row"><span class="bullet"></span><span>{h}</span></div>
            {/each}
          </div>

          <div class="links">
            <div class="link-row">
              <span>{t("about.repo")}</span>
              <button type="button" class="link" onclick={() => openWeb("repo")}>theCosmicCrafter/cosmic-font-foundry<ArrowUpRight size={12} strokeWidth={2} /></button>
            </div>
            <div class="link-row">
              <span>{t("about.issue")}</span>
              <button type="button" class="link" onclick={() => openWeb("issue")}>GitHub Issues<ArrowUpRight size={12} strokeWidth={2} /></button>
            </div>
            <div class="link-row">
              <span>{t("about.upstream")}</span>
              <button type="button" class="link" onclick={() => openWeb("upstream")}>atunfrito/tuna-fonts<ArrowUpRight size={12} strokeWidth={2} /></button>
            </div>
          </div>

          <div class="credit-foot">
            <span>{t("about.forkCredit")}</span>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; z-index: 20; background: var(--velo); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; animation: cf-fade 0.16s ease-out; }
  .modal { width: 700px; height: 550px; border-radius: 14px; background: var(--win); color: var(--ink); box-shadow: var(--float-shadow); overflow: hidden; display: grid; grid-template-columns: 192px minmax(0, 1fr); transition: width 0.16s ease, height 0.16s ease; }
  .modal.wide { width: min(960px, 94vw); height: min(640px, 90vh); }
  .side { background: var(--side); border-right: 1px solid var(--line); padding: 18px 12px; display: flex; flex-direction: column; gap: 4px; }
  .brand { font-size: 15.5px; padding: 0 10px 14px; white-space: nowrap; }
  .tab { height: 32px; padding: 0 10px; border: 0; border-radius: 8px; background: transparent; color: var(--ink2); font: 500 13px var(--sans); display: flex; align-items: center; gap: 9px; text-align: left; }
  .tab-lbl { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tab-badge { font: 700 9px var(--mono); padding: 1px 5px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); }
  .side-spacer { flex: 1; }
  .secret-tab-btn {
    border: 0; border-radius: 7px; padding: 6px 9px; background: transparent;
    color: var(--ink3); opacity: 0.6; font: 500 11px var(--mono);
    display: flex; align-items: center; gap: 6px; transition: opacity 0.15s, background-color 0.15s, color 0.15s;
  }
  .secret-tab-btn:hover { opacity: 1; background: var(--hov); color: var(--accent); }
  .tab:hover { background: var(--hov); color: var(--ink); }
  .tab.active { background: var(--chip); color: var(--ink); font-weight: 600; }

  .body { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .head { flex: none; display: flex; align-items: center; justify-content: space-between; padding: 18px 22px 12px; }
  .head h2 { margin: 0; font-size: 16px; font-weight: 700; }
  .close { width: 28px; height: 28px; border: 0; border-radius: 8px; background: transparent; color: var(--ink3); display: grid; place-items: center; }
  .close:hover { background: var(--hov); color: var(--ink); }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 22px 24px; display: flex; flex-direction: column; gap: 14px; }

  .row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 4px 0; }
  .txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .lbl { font-size: 13px; font-weight: 600; color: var(--ink); }
  .hint { font-size: 11.5px; color: var(--ink3); line-height: 1.4; margin: 0; }
  .sample-field { display: flex; flex-direction: column; gap: 6px; padding-top: 6px; }
  .sample-field input { height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--line2); background: var(--field); color: var(--ink); font-size: 13px; }

  .btn { height: 28px; padding: 0 12px; border-radius: 8px; border: 0; background: var(--chip); color: var(--ink); font: 600 12px var(--sans); display: inline-flex; align-items: center; gap: 6px; flex: none; }
  .btn:hover:not(:disabled) { background: var(--line2); }
  .btn.accent { background: var(--accent); color: #fff; }
  .btn.accent:hover:not(:disabled) { filter: brightness(1.06); }
  .btn.ghost { background: transparent; color: var(--ink2); }
  .btn.self-start { align-self: flex-start; }

  .card-block { padding: 14px; border-radius: 12px; border: 1px solid var(--line); background: var(--field); display: flex; flex-direction: column; gap: 10px; }
  .card-block.highlight { border-color: var(--accent); background: var(--accent-soft); }
  .row-title { display: flex; align-items: center; gap: 6px; color: var(--accent); }
  .sync-path { padding: 7px 10px; border-radius: 7px; background: var(--side); font: 400 11.5px var(--mono); color: var(--ink2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vault-metrics { display: flex; flex-wrap: wrap; gap: 6px; font-size: 11.5px; color: var(--ink2); }
  .cleanup-box { display: flex; flex-direction: column; gap: 8px; padding: 10px; border-radius: 10px; background: var(--side); border: 1px solid var(--line); }
  .cleanup-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 12px; font-weight: 600; color: var(--ink); }
  .cleanup-list { max-height: 180px; overflow: auto; display: flex; flex-direction: column; gap: 4px; }
  .cleanup-row { display: grid; grid-template-columns: auto auto minmax(0, 1fr) auto; align-items: center; gap: 8px; font-size: 11.5px; color: var(--ink2); }
  .cleanup-fam { font-weight: 600; color: var(--ink); white-space: nowrap; }
  .cleanup-path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink3); }
  .btn-row { display: flex; flex-wrap: wrap; gap: 8px; }
  .warn-note { font-size: 11.5px; color: var(--warn); display: flex; flex-direction: column; gap: 2px; }

  .shortcuts { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 10px 14px; align-items: center; }
  .keys { display: inline-flex; gap: 4px; padding: 4px 6px; border: 1px solid var(--line2); border-radius: 8px; background: var(--field); justify-self: start; }
  .keys.recording { border-color: var(--accent); background: var(--accent-soft); }
  .keys.fixed { border-color: transparent; background: transparent; padding-left: 0; }
  kbd { font: 600 11px var(--mono); padding: 2px 6px; border-radius: 5px; background: var(--chip); color: var(--ink); }
  .desc { font-size: 12.5px; color: var(--ink2); }

  .about-head { display: flex; align-items: center; gap: 14px; }
  .brand.big { display: block; font-size: 20px; padding: 0; }
  .ver { font-size: 12px; color: var(--ink3); font-family: var(--mono); }
  .tagline { margin: 0; font-size: 13px; color: var(--ink2); line-height: 1.5; }
  .metrics { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .metric { padding: 10px 12px; border-radius: 10px; background: var(--side); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 2px; }
  .metric .n { font-size: 18px; font-weight: 700; }
  .metric .l { font-size: 11px; color: var(--ink3); }
  .highlights { display: flex; flex-direction: column; gap: 7px; }
  .hl-row { display: flex; align-items: baseline; gap: 8px; font-size: 12.5px; color: var(--ink2); }
  .bullet { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); flex: none; }
  .links { display: flex; flex-direction: column; gap: 6px; border-top: 1px solid var(--line); padding-top: 12px; }
  .link-row { display: flex; align-items: center; justify-content: space-between; font-size: 12.5px; color: var(--ink2); }
  .link { border: 0; background: transparent; color: var(--accent); font: 600 12.5px var(--sans); display: inline-flex; align-items: center; gap: 4px; padding: 2px 4px; }
  .credit-foot { font-size: 11.5px; color: var(--ink3); padding-top: 4px; }
</style>
