<script>
  import { SlidersHorizontal, RefreshCw, Keyboard, Info, X, ArrowUpRight, FolderSync, Download, Upload, Sparkles } from "@lucide/svelte";
  import SegmentedControl from "../components/SegmentedControl.svelte";
  import { settings, currentTheme, setTheme, setLanguage, version, DEFAULT_SHORTCUTS, inTauri } from "../lib/settings.svelte.js";
  import { t } from "../lib/i18n.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { backedUpCount, restoreSystemFonts, fonts, invokeCmd, applyExternalList } from "../lib/fonts.svelte.js";
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

  const TABS = [
    { id: "settings", label: () => t("settings"), icon: SlidersHorizontal },
    { id: "sync", label: () => t("syncAndExport"), icon: FolderSync },
    { id: "shortcuts", label: () => t("shortcuts"), icon: Keyboard },
    { id: "about", label: () => t("about"), icon: Info },
  ];

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
    { label: t("set.language"), hint: "", value: settings.language, select: setLanguage, options: [["en", "English"], ["es", "Español"]] },
  ]);

  // ---- BYO-Sync, Portability & tuna-fonts Migration ----
  let syncBusy = $state(false);
  let unresolvedSync = $state([]);
  let tunaStatus = $state({ available: false });
  let migrating = $state(false);

  if (inTauri) {
    invokeCmd("detect_tuna_migration").then((s) => (tunaStatus = s)).catch(() => {});
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
  <div class="modal" role="dialog" aria-modal="true" aria-label={t("settings")} tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <div class="side">
      <span class="brand">Cosmic<i>.</i></span>
      {#each TABS as tb (tb.id)}
        <button type="button" class="tab" class:active={tab === tb.id} onclick={() => ((tab = tb.id), (recording = null))}>
          <tb.icon size={14} strokeWidth={1.9} />{tb.label()}
        </button>
      {/each}
    </div>

    <div class="body">
      <div class="head">
        <h2>{tab === "settings" ? t("settings") : tab === "sync" ? t("syncAndExport") : tab === "shortcuts" ? t("shortcuts") : t("about")}</h2>
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
  .modal { width: 700px; height: 550px; border-radius: 14px; background: var(--win); color: var(--ink); box-shadow: var(--float-shadow); overflow: hidden; display: grid; grid-template-columns: 192px minmax(0, 1fr); }
  .side { background: var(--side); border-right: 1px solid var(--line); padding: 18px 12px; display: flex; flex-direction: column; gap: 4px; }
  .brand { font-size: 17px; padding: 0 10px 14px; }
  .tab { height: 32px; padding: 0 10px; border: 0; border-radius: 8px; background: transparent; color: var(--ink2); font: 500 13px var(--sans); display: flex; align-items: center; gap: 9px; text-align: left; }
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
