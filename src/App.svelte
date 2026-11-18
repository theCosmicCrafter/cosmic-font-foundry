<script>
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    Search, List, LayoutGrid, Folder, FolderOpen, Trash2, X, Heart,
    WifiOff, Layers, Type, LoaderCircle, Pin, Archive, ShieldCheck, Sparkles, Rss, Code2, Check,
  } from "@lucide/svelte";

  import Sidebar from "./components/Sidebar.svelte";
  import WindowControls from "./components/WindowControls.svelte";
  import Toasts from "./components/Toasts.svelte";
  import SegmentedControl from "./components/SegmentedControl.svelte";
  import VirtualList from "./components/VirtualList.svelte";
  import FontRow from "./components/FontRow.svelte";
  import FontCard from "./components/FontCard.svelte";
  import GoogleRow from "./components/GoogleRow.svelte";
  import GoogleCard from "./components/GoogleCard.svelte";
  import EmptyState from "./components/EmptyState.svelte";
  import SkeletonList from "./components/SkeletonList.svelte";
  import SelectionBar from "./components/SelectionBar.svelte";
  import SmartScanModal from "./components/SmartScanModal.svelte";
  import CustomSourcesModal from "./components/CustomSourcesModal.svelte";
  import Settings from "./views/Settings.svelte";
  import Specimen from "./views/Specimen.svelte";
  import Compare from "./views/Compare.svelte";
  import Forge from "./views/Forge.svelte";

  import { initUpdater } from "./lib/updater.svelte.js";
  import { settings, inTauri, currentTheme } from "./lib/settings.svelte.js";
  import { t, locale } from "./lib/i18n.svelte.js";
  import { notify } from "./lib/toasts.svelte.js";
  import {
    fonts, initFonts, refreshFonts, toggleFamilies, installFontFiles,
    fontStyle, applyExternalList, invokeCmd, quarantineCorruptFont,
  } from "./lib/fonts.svelte.js";
  import {
    library, initLibrary, toggleFavorite, addToFavorites, createCollection,
    renameCollection, togglePinCollection, deleteCollection, addFontsToCollection,
    tagsForFamily, autoCreateOsCollections, upsertCollectionWithFonts,
  } from "./lib/library.svelte.js";
  import { discover, loadCatalog, requestGooglePreviews, preloadDiscover, installGoogleFont } from "./lib/discover.svelte.js";
  import { drag, startDrag } from "./lib/drag.svelte.js";
  import { shortcutCombo } from "./lib/shortcuts.js";

  // section: installed | discover | favorites | diagnostics | col:<id> | dir:<path>
  let section = $state(settings.onOpen === "favorites" ? "favorites" : "installed");
  let cat = $state(null);
  let script = $state(null);
  let q = $state("");
  let customSample = $state("");
  let filter = $state("all"); // all | active | inactive | variable
  let installedOnly = $state(false);
  let discoverInstallFilter = $state("all"); // all | installed | uninstalled
  let sort = $state("alpha");
  let discoverSource = $state("all"); // all | google | fontshare | fontsource
  const sel = new SvelteSet();
  let detailName = $state(null);
  let googleDetailName = $state(null);
  let compareNames = $state([]);
  let settingsTab = $state(null);
  let scanOpen = $state(false);
  let customSourcesOpen = $state(false);
  let fmt = $state({ bold: false, italic: false, underline: false, strike: false, uppercase: false });
  let colOpen = $state(false);
  let renaming = $state(null);
  let confirmingDelete = $state(null);
  let dropping = $state(false);
  let folders = $state([]);
  let readingFolder = $state(null);
  let searchInput = $state();

  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let splash = $state(!reducedMotion);
  if (!reducedMotion) setTimeout(() => (splash = false), 760);

  const defaultSample = $derived(settings.sampleText.trim() || t("defaultSample"));
  const sample = $derived(customSample || defaultSample);
  const size = $derived(settings.size);

  const inDiscover = $derived(section === "discover");
  const inForge = $derived(section === "forge");
  const inDiagnostics = $derived(section === "diagnostics");
  const activeCollection = $derived(section.startsWith("col:") ? library.collections.find((c) => "col:" + c.id === section) : null);
  const activeFolder = $derived(section.startsWith("dir:") ? section.slice(4) : null);
  const detailFamily = $derived(detailName ? fonts.list.find((f) => f.name === detailName) : null);
  const googleDetail = $derived(
    googleDetailName && !detailFamily ? discover.list.find((g) => g.name === googleDetailName) : null,
  );
  const comparedFamilies = $derived(compareNames.map((n) => fonts.list.find((f) => f.name === n)).filter(Boolean));

  const isLocked = (f) => f.origin === "sys" && (!settings.systemUnlocked || f.essential);
  const lockReason = (f) => (f.essential ? t("lock.essential") : t("lock.system"));
  const isVisible = (f) => settings.showLocked || !isLocked(f);
  const inFolder = (f, folderPath) =>
    f.files.some((p) => p.toLowerCase().startsWith(folderPath.toLowerCase() + "\\"));
  const favSet = $derived(new Set(library.favorites));
  const activeColSet = $derived(activeCollection ? new Set(activeCollection.fonts.map((n) => n.toLowerCase())) : null);
  const activeStateMap = $derived(new Map(fonts.list.map((f) => [f.name.toLowerCase(), f.active])));

  // ---- Filtered lists ----
  const baseList = $derived.by(() => {
    const l = fonts.list;
    if (section === "favorites") return l.filter((f) => favSet.has(f.name));
    if (section === "diagnostics") return l.filter((f) => f.error || f.has_conflicts || (f.duplicates?.length ?? 0) > 0);
    if (activeColSet) return l.filter((f) => activeColSet.has(f.name.toLowerCase()));
    if (activeFolder) return l.filter((f) => inFolder(f, activeFolder));
    return l.filter((f) => f.origin !== "dir" && isVisible(f));
  });

  const ql = $derived(q.trim().toLowerCase());
  const matchesQuery = (f) =>
    !ql ||
    f.name.toLowerCase().includes(ql) ||
    tagsForFamily(f.name).some((tag) => tag.includes(ql));

  const filteredLocal = $derived(
    baseList.filter((f) => {
      if (installedOnly && f.origin === "dir") return false;
      if (cat && f.category !== cat) return false;
      if (script && !(f.scripts ?? []).includes(script)) return false;
      if (!matchesQuery(f)) return false;
      if (filter === "variable") return f.is_variable;
      if (filter === "active") return f.active || isLocked(f);
      if (filter === "inactive") return !(f.active || isLocked(f));
      return true;
    }),
  );

  const installedNamesLower = $derived(new Set(fonts.list.map((f) => f.name.toLowerCase())));
  const discoverMetaMap = $derived(new Map(discover.list.map((g) => [g.name.toLowerCase(), g])));

  const favGoogle = $derived(
    section === "favorites" && !installedOnly
      ? discover.list.filter(
          (g) =>
            favSet.has(g.name) &&
            !installedNamesLower.has(g.name.toLowerCase()) &&
            (!cat || g.category === cat) &&
            (!script || (g.scripts ?? []).includes(script)) &&
            (!ql || g.name.toLowerCase().includes(ql)) &&
            filter !== "active" &&
            (filter !== "variable" || g.is_variable),
        )
      : [],
  );

  const viewList = $derived.by(() => {
    const items = favGoogle.length
      ? [...filteredLocal, ...favGoogle.map((g) => ({ google: g, name: g.name }))]
      : [...filteredLocal];
    if (sort === "popularity") {
      return items.sort((a, b) => {
        const pa = a.google?.popularity ?? discoverMetaMap.get(a.name.toLowerCase())?.popularity ?? 999999;
        const pb = b.google?.popularity ?? discoverMetaMap.get(b.name.toLowerCase())?.popularity ?? 999999;
        return pa !== pb ? pa - pb : a.name.localeCompare(b.name);
      });
    }
    if (sort === "recent") {
      return items.sort((a, b) => {
        const da = a.google?.date ?? discoverMetaMap.get(a.name.toLowerCase())?.date ?? "";
        const db = b.google?.date ?? discoverMetaMap.get(b.name.toLowerCase())?.date ?? "";
        if (da !== db) return db > da ? 1 : -1;
        return a.name.localeCompare(b.name);
      });
    }
    return items.sort((a, b) => a.name.localeCompare(b.name));
  });

  $effect(() => {
    if (section === "favorites" && library.favorites.some((n) => !installedNamesLower.has(n.toLowerCase()))) {
      loadCatalog();
    }
  });

  const activeCustomSource = $derived(
    discoverSource.startsWith("custom:")
      ? discover.customSources.find((s) => "custom:" + s.id === discoverSource) ?? null
      : null,
  );
  const hasEnabledCustomSources = $derived(discover.customSources.some((s) => s.enabled));
  $effect(() => {
    if (discoverSource.startsWith("custom:") && !activeCustomSource) {
      discoverSource = hasEnabledCustomSources ? "custom" : "all";
    } else if (discoverSource === "custom" && !hasEnabledCustomSources) {
      discoverSource = "all";
    }
  });

  const googleList = $derived.by(() => {
    const l = discover.list.filter((g) => {
      const isInst = installedNamesLower.has(g.name.toLowerCase());
      if (installedOnly && !isInst) return false;
      if (discoverInstallFilter === "installed" && !isInst) return false;
      if (discoverInstallFilter === "uninstalled" && isInst) return false;
      return (
        (discoverSource === "all" ||
          (discoverSource === "custom"
            ? (g.source ?? "").startsWith("custom:")
            : (g.source ?? "google") === discoverSource)) &&
        (!cat || g.category === cat) &&
        (!script || (g.scripts ?? []).includes(script)) &&
        (!ql || g.name.toLowerCase().includes(ql))
      );
    });
    if (sort === "popularity") return [...l].sort((a, b) => a.popularity - b.popularity);
    if (sort === "recent") return [...l].sort((a, b) => (b.date > a.date ? 1 : b.date < a.date ? -1 : 0));
    return [...l].sort((a, b) => a.name.localeCompare(b.name));
  });

  const catCounts = $derived.by(() => {
    const c = {};
    for (const f of inDiscover ? discover.list : baseList) c[f.category] = (c[f.category] ?? 0) + 1;
    return c;
  });

  const scriptCounts = $derived.by(() => {
    const c = {};
    for (const f of inDiscover ? discover.list : baseList) {
      for (const s of f.scripts ?? []) c[s] = (c[s] ?? 0) + 1;
    }
    return c;
  });

  const installedFonts = $derived(fonts.list.filter((f) => f.origin !== "dir" && isVisible(f)));
  const activeCount = $derived(installedFonts.filter((f) => f.active || isLocked(f)).length);
  const diagCount = $derived(fonts.list.filter((f) => f.error || f.has_conflicts || (f.duplicates?.length ?? 0) > 0).length);

  const googleCountLabel = $derived(
    discover.list.length
      ? discover.list.length >= 1000
        ? (Math.floor(discover.list.length / 100) / 10).toLocaleString(locale()) + "k"
        : discover.list.length
      : "",
  );
  const counts = $derived({
    installed: installedFonts.length,
    discover: googleCountLabel,
    favorites: library.favorites.length,
    diagnostics: diagCount,
  });
  const folderCounts = $derived(folders.map((c) => ({ ...c, count: fonts.list.filter((f) => inFolder(f, c.path)).length })));
  const projectFolders = $derived(new Set((fonts.projects ?? []).map((p) => p.folder.toLowerCase())));
  const activeProject = $derived(
    activeFolder ? (fonts.projects ?? []).find((p) => p.folder.toLowerCase() === activeFolder.toLowerCase()) : null,
  );

  const heading = $derived(
    comparedFamilies.length
      ? t("compareTitle")
      : detailFamily || googleDetail
        ? t("specimenTitle")
        : cat
          ? t("cat." + cat)
          : inDiscover
            ? t("discoverCatalog")
            : inForge
              ? t("sec.forge")
              : inDiagnostics
                ? t("sec.diagnostics")
                : section === "favorites"
                  ? t("sec.favorites")
                  : activeCollection
                    ? activeCollection.name
                    : activeFolder
                      ? (folders.find((c) => c.path === activeFolder)?.name ?? "")
                      : t("sec.installed"),
  );
  const isNormalView = $derived(!detailFamily && !googleDetail && !comparedFamilies.length && !inForge);

  // ---- Discover font preview requests ----
  let visibleGoogle = [];
  function onVisibleGoogle(items) {
    visibleGoogle = items;
    requestGooglePreviews(items, sample);
  }
  function onVisibleFav(items) {
    visibleGoogle = items.filter((it) => it.google).map((it) => it.google);
    if (visibleGoogle.length) requestGooglePreviews(visibleGoogle, sample);
  }
  $effect(() => {
    const m = sample;
    if (inDiscover || section === "favorites") requestGooglePreviews(visibleGoogle, m);
  });

  // ---- Actions ----
  function goTo(s) {
    section = s;
    detailName = null;
    googleDetailName = null;
    compareNames = [];
    sel.clear();
    cat = null;
    script = null;
    colOpen = false;
    if (s === "discover") loadCatalog();
  }

  async function toggleOne(f) {
    const activate = !f.active;
    const o = await toggleFamilies([f.name], activate, settings.systemUnlocked);
    if (o?.changed) {
      notify("ok", t(activate ? "ok.activated" : "ok.deactivated", { n: f.name }), activate ? t("ok.availableNow") : "");
    }
  }

  async function toggleMany(names, activate, clearSelection = true) {
    const targets = names.map((n) => fonts.list.find((f) => f.name === n)).filter(Boolean);
    const lockedCount = targets.filter(isLocked).length;
    const toChange = targets.filter((f) => !isLocked(f) && f.active !== activate).map((f) => f.name);
    if (clearSelection) sel.clear();
    colOpen = false;
    if (toChange.length) await toggleFamilies(toChange, activate, settings.systemUnlocked);
    const n = targets.length - lockedCount;
    if (lockedCount && !activate) {
      notify("warn", t("ok.deactivatedMany", { n }), lockedCount === 1 ? t("warn.systemSkippedOne") : t("warn.systemSkipped", { n: lockedCount }));
    } else {
      notify("ok", t(activate ? "ok.activatedMany" : "ok.deactivatedMany", { n }), "");
    }
  }

  function addToCol(id, names) {
    let col = library.collections.find((x) => x.id === id);
    if (!col) {
      id = createCollection(t("col.defaultName", { n: library.collections.length + 1 }));
      col = library.collections.find((x) => x.id === id);
    }
    addFontsToCollection(id, names);
    notify("ok", t("ok.toCollection", { n: names.length === 1 ? names[0] : t("fontsN", { n: names.length }), c: col.name }), "");
  }

  function onCreateCollection(name) {
    const id = createCollection(name);
    goTo("col:" + id);
  }

  let deleteTimer;
  function onDeleteCollection() {
    if (!activeCollection) return;
    if (confirmingDelete !== activeCollection.id) {
      confirmingDelete = activeCollection.id;
      clearTimeout(deleteTimer);
      deleteTimer = setTimeout(() => (confirmingDelete = null), 3000);
      return;
    }
    deleteCollection(activeCollection.id);
    confirmingDelete = null;
    goTo("installed");
    notify("ok", t("ok.colDeleted"), t("ok.fontsUntouched"));
  }

  async function toggleCollectionFonts(activate) {
    if (!activeCollection) return;
    const names = activeCollection.fonts;
    const targets = names.map((x) => fonts.list.find((f) => f.name === x)).filter((f) => f && !isLocked(f) && f.active !== activate);
    if (targets.length) await toggleFamilies(targets.map((f) => f.name), activate, settings.systemUnlocked);
    if (activate) notify("ok", t("ok.colActivated"), t("ok.colReady", { n: names.length }));
    else notify("ok", t("ok.colDeactivated"), "");
  }

  async function exportCollectionZip() {
    if (!activeCollection || !inTauri) return;
    try {
      const res = await invokeCmd("export_collection_zip", {
        collectionName: activeCollection.name,
        fonts: activeCollection.fonts,
      });
      if (res) {
        notify("ok", t("col.exportedZip"), t("col.exportedZipDesc", { files: res.font_files }));
      }
    } catch (e) {
      notify("err", t("err.one"), String(e));
    }
  }

  async function activateProjectFonts() {
    if (!activeProject) return;
    const missingOrOff = activeProject.fonts.filter((name) => {
      const fam = fonts.list.find((f) => f.name.toLowerCase() === name.toLowerCase());
      return fam && !fam.active && !isLocked(fam);
    });
    if (missingOrOff.length) {
      await toggleFamilies(missingOrOff, true, settings.systemUnlocked);
    }
    notify("ok", t("ok.colActivated"), t("ok.colReady", { n: activeProject.fonts.length }));
  }

  async function addFolder() {
    if (!inTauri) return;
    const list = await invokeCmd("add_watched_folder");
    if (!list) return;
    const created = list.find((c) => !folders.some((x) => x.path === c.path));
    folders = list;
    if (created) goTo("dir:" + created.path);
    readingFolder = created?.path ?? null;
    try {
      await refreshFonts();
    } finally {
      readingFolder = null;
    }
    notify("ok", t("ok.folderAdded"), t("ok.folderAddedText"));
  }

  async function removeFolder() {
    if (!activeFolder) return;
    folders = await invokeCmd("remove_watched_folder", { path: activeFolder });
    await refreshFonts();
    goTo("installed");
    notify("ok", t("ok.folderRemoved"), t("ok.folderRemovedText"));
  }

  function selectFamily(f) {
    if (sel.has(f.name)) sel.delete(f.name);
    else sel.add(f.name);
    colOpen = false;
  }

  function onDragFamily(e, f) {
    startDrag(
      e,
      () => (sel.has(f.name) ? [...sel] : [f.name]),
      (id, names) => {
        addToCol(id, names);
        sel.clear();
      },
    );
  }

  // ---- Ctrl + wheel: adjust sample preview size ----
  $effect(() => {
    const onWheel = (e) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const step = e.deltaY < 0 ? 2 : -2;
      settings.size = Math.min(96, Math.max(10, Math.round(settings.size) + step));
    };
    window.addEventListener("wheel", onWheel, { passive: false });
    return () => window.removeEventListener("wheel", onWheel);
  });

  // ---- Keyboard shortcuts ----
  function onKeyDown(e) {
    if (settingsTab || splash) return;
    if (scanOpen) {
      if (e.key === "Escape") scanOpen = false;
      return;
    }
    const combo = shortcutCombo(e) ?? "";
    const tag = e.target?.tagName ?? "";
    const typing = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
    if (combo === settings.shortcuts.search) {
      e.preventDefault();
      searchInput?.focus();
      searchInput?.select();
      return;
    }
    if (e.key === "Escape") {
      if (typing) return e.target.blur();
      if (colOpen) colOpen = false;
      else if (compareNames.length) compareNames = [];
      else if (detailName || googleDetailName) {
        detailName = null;
        googleDetailName = null;
      } else sel.clear();
      return;
    }
    if (typing || !isNormalView || inDiscover) return;
    if (combo === settings.shortcuts.all) {
      e.preventDefault();
      for (const f of filteredLocal) sel.add(f.name);
      return;
    }
    if (combo === settings.shortcuts.toggle && sel.size) {
      e.preventDefault();
      const names = [...sel];
      const anyInactive = fonts.list.some((f) => sel.has(f.name) && !isLocked(f) && !f.active);
      toggleMany(names, anyInactive, false);
    }
  }

  // ---- Sync status + pinned collections to tray menu & Quick-Switcher ----
  $effect(() => {
    if (!inTauri) return;
    const n = folders.length;
    const status = n === 1 ? t("menu.folder") : n ? t("menu.folders", { n }) : t("menu.activeFonts", { a: activeCount, n: installedFonts.length });
    const pinned = library.collections
      .filter((c) => c.pinned)
      .slice(0, 4)
      .map((c) => ({
        id: c.id,
        name: c.name,
        fonts: [...c.fonts],
        all_active:
          c.fonts.length > 0 &&
          c.fonts.every((name) => activeStateMap.get(name.toLowerCase()) ?? false),
      }));
    const payload = {
      status,
      folders: n,
      theme: currentTheme(),
      language: settings.language,
      shortcut: settings.quickSwitcherShortcut ?? "Ctrl+Shift+F",
      pinned,
    };
    const timer = setTimeout(() => invokeCmd("update_tray", payload).catch(() => {}), 260);
    return () => clearTimeout(timer);
  });

  let osSeeded = false;
  $effect(() => {
    if (!osSeeded && library.ready && !fonts.loading && fonts.list.length > 0) {
      osSeeded = true;
      if (library.collections.length === 0) {
        autoCreateOsCollections(fonts.list);
      }
    }
  });

  // ---- Startup ----
  onMount(() => {
    initLibrary();
    initUpdater();
    initFonts();
    preloadDiscover(sample);

    const handleAgentUiCommand = (payload) => {
      if (!payload) return;
      if (payload.action === "navigate") {
        if (payload.section === "agent") {
          settings.showAgentTab = true;
          settingsTab = "agent";
        } else {
          settingsTab = null;
          if (payload.section) goTo(payload.section);
        }
        if (typeof payload.query === "string") q = payload.query;
        if (typeof payload.category === "string") {
          cat = payload.category === "all" ? null : payload.category;
        }
        if (typeof payload.sample === "string") customSample = payload.sample;
      } else if (payload.action === "auto_organize") {
        if (payload.mode === "os") {
          const count = autoCreateOsCollections(fonts.list);
          notify("ok", t("scan.osOrganized", { n: count }), t("scan.osOrganizedDesc"));
        } else if (payload.mode === "smart_collections" && payload.collection_name) {
          const names = payload.families ?? [];
          upsertCollectionWithFonts(payload.collection_name, names);
          notify(
            "ok",
            payload.collection_name,
            t("scan.collected", { n: names.length, c: payload.collection_name }),
          );
        }
      } else if (payload.action === "scan_report") {
        scanOpen = true;
      }
    };
    const onDemoAgentUi = (e) => handleAgentUiCommand(e.detail);
    window.addEventListener("cosmic-demo-agent-ui", onDemoAgentUi);

    if (!inTauri) {
      return () => window.removeEventListener("cosmic-demo-agent-ui", onDemoAgentUi);
    }

    invokeCmd("watched_folders").then((l) => (folders = l)).catch(() => {});

    let unlistenTray;
    let unlistenAgentUi;
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlistenTray = await listen("tray-command", ({ payload }) => {
        if (payload === "search") {
          settingsTab = null;
          if (section !== "installed") goTo("installed");
          setTimeout(() => searchInput?.focus(), 50);
        } else if (payload === "discover") {
          settingsTab = null;
          goTo("discover");
        } else if (payload === "settings") {
          settingsTab = "settings";
        }
      });
      unlistenAgentUi = await listen("agent-ui-command", ({ payload }) => {
        handleAgentUiCommand(payload);
      });
    });

    let unlistenDrop;
    import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
      unlistenDrop = await getCurrentWebview().onDragDropEvent(async ({ payload: p }) => {
        if (p.type === "enter") dropping = (p.paths ?? []).length > 0;
        else if (p.type === "leave") dropping = false;
        else if (p.type === "drop") {
          dropping = false;
          const paths = (p.paths ?? []).filter((r) => /\.(ttf|otf|ttc|otc)$/i.test(r));
          if (!paths.length) return notify("err", t("err.notFont"), t("err.notFontText"));
          const allUsers = settings.installFor === "all";
          const o = await installFontFiles(paths, allUsers);
          if (!o || !o.installed.length) return;
          const title = o.installed.length === 1 ? t("ok.installed", { n: o.installed[0] }) : t("ok.installedMany", { n: o.installed.length });
          notify("ok", title, allUsers ? t("ok.forAll") : t("ok.forUser"));
        }
      });
    });

    return () => {
      unlistenDrop?.();
      unlistenTray?.();
      unlistenAgentUi?.();
      window.removeEventListener("cosmic-demo-agent-ui", onDemoAgentUi);
    };
  });
</script>

<svelte:window onkeydown={onKeyDown} />

{#snippet rowSnippet(f)}
  <FontRow
    {f}
    style={fontStyle(f)}
    {sample}
    {size}
    {fmt}
    selected={sel.has(f.name)}
    fav={favSet.has(f.name)}
    locked={isLocked(f)}
    reason={lockReason(f)}
    onselect={() => selectFamily(f)}
    onopen={() => ((detailName = f.name), (googleDetailName = null), sel.clear())}
    ontoggle={() => toggleOne(f)}
    onfav={() => toggleFavorite(f.name)}
    ondrag={(e) => onDragFamily(e, f)}
  />
{/snippet}

{#snippet cardSnippet(f)}
  <FontCard
    {f}
    style={fontStyle(f)}
    selected={sel.has(f.name)}
    fav={favSet.has(f.name)}
    locked={isLocked(f)}
    reason={lockReason(f)}
    onselect={() => selectFamily(f)}
    onopen={() => ((detailName = f.name), (googleDetailName = null), sel.clear())}
    ontoggle={() => toggleOne(f)}
    onfav={() => toggleFavorite(f.name)}
    ondrag={(e) => onDragFamily(e, f)}
  />
{/snippet}

{#snippet googleRowSnippet(g)}
  <GoogleRow
    {g}
    {sample}
    {size}
    {fmt}
    installed={installedNamesLower.has(g.name.toLowerCase())}
    pct={discover.downloads[g.name]}
    fav={favSet.has(g.name)}
    onfav={() => toggleFavorite(g.name)}
    oninstall={() => installGoogleFont(g.name, applyExternalList)}
    oninspect={(entry) => ((googleDetailName = entry.name), (detailName = null))}
  />
{/snippet}

{#snippet favRowSnippet(item)}
  {#if item.google}
    <GoogleRow
      g={item.google}
      {sample}
      {size}
      {fmt}
      notInstalled
      installed={false}
      pct={discover.downloads[item.name]}
      fav={favSet.has(item.name)}
      onfav={() => toggleFavorite(item.name)}
      oninstall={() => installGoogleFont(item.name, applyExternalList)}
      oninspect={(entry) => ((googleDetailName = entry.name), (detailName = null))}
    />
  {:else}
    {@render rowSnippet(item)}
  {/if}
{/snippet}

{#snippet favCardSnippet(item)}
  {#if item.google}
    <GoogleCard
      g={item.google}
      pct={discover.downloads[item.name]}
      fav={favSet.has(item.name)}
      onfav={() => toggleFavorite(item.name)}
      oninstall={() => installGoogleFont(item.name, applyExternalList)}
      oninspect={(entry) => ((googleDetailName = entry.name), (detailName = null))}
    />
  {:else}
    {@render cardSnippet(item)}
  {/if}
{/snippet}

<div class="window">
  <Sidebar
    {section}
    {cat}
    {script}
    {counts}
    {catCounts}
    {scriptCounts}
    collections={library.collections}
    folders={folderCounts}
    {projectFolders}
    footer={t("activeSummary", { n: activeCount, total: installedFonts.length })}
    bind:renaming
    onsection={goTo}
    oncat={(c) => ((cat = c), (detailName = null), (googleDetailName = null), (compareNames = []))}
    onscript={(s) => ((script = s), (detailName = null), (googleDetailName = null), (compareNames = []))}
    oncreateCollection={onCreateCollection}
    onrenameCollection={renameCollection}
    onaddFolder={addFolder}
    onscan={() => (scanOpen = true)}
    onsettings={() => (settingsTab = "settings")}
    onabout={() => (settingsTab = "about")}
  />

  <main>
    <header data-tauri-drag-region>
      {#if isNormalView}
        <label class="field search">
          <Search size={13} strokeWidth={2} />
          <input bind:this={searchInput} bind:value={q} placeholder={inDiscover ? t("searchGoogle") : t("search")} spellcheck="false" />
        </label>
        <label class="field sample-box">
          <span class="a">A</span>
          <input bind:value={customSample} placeholder={t("typeSomething")} spellcheck="false" />
          {#if customSample}
            <button type="button" class="reset" title={t("resetText")} aria-label={t("resetText")} onclick={() => (customSample = "")}><X size={9} strokeWidth={3} /></button>
          {/if}
        </label>
        <div class="size-ctrl" title={t("size")}>
          <span class="a sm">A</span>
          <input class="range" type="range" min="10" max="96" bind:value={settings.size} aria-label={t("size")} />
          <span class="a lg">A</span>
          <span class="num n">{size}</span>
        </div>
        <div class="fmt-group" role="group" aria-label={t("spec.formatSampler")}>
          <button type="button" class="fmt-tog b" class:on={fmt.bold} title={t("fmt.bold")} onclick={() => (fmt.bold = !fmt.bold)}>B</button>
          <button type="button" class="fmt-tog i" class:on={fmt.italic} title={t("fmt.italic")} onclick={() => (fmt.italic = !fmt.italic)}>I</button>
          <button type="button" class="fmt-tog u" class:on={fmt.underline} title={t("fmt.underline")} onclick={() => (fmt.underline = !fmt.underline)}>U</button>
          <button type="button" class="fmt-tog s" class:on={fmt.strike} title={t("fmt.strike")} onclick={() => (fmt.strike = !fmt.strike)}>S</button>
          <button type="button" class="fmt-tog" class:on={fmt.uppercase} title={t("fmt.upper")} onclick={() => (fmt.uppercase = !fmt.uppercase)}>AA</button>
        </div>
      {/if}
      <div class="spacer" data-tauri-drag-region></div>
      {#if isNormalView && !inDiscover}
        <div class="view-mode">
          <SegmentedControl
            label={t("list")}
            options={[
              { value: "list", icon: List, title: t("list") },
              { value: "grid", icon: LayoutGrid, title: t("grid") },
            ]}
            value={settings.view}
            onselect={(v) => (settings.view = v)}
            width={34}
            height={24}
          />
        </div>
      {/if}
      <WindowControls />
    </header>

    <div class="title-bar">
      <span class="label">{heading}</span>
      {#if isNormalView}<span class="count num">{inDiscover ? googleList.length : viewList.length}</span>{/if}
      <div class="right">
        {#if isNormalView && !inDiagnostics}
          <button
            type="button"
            class="pill"
            class:accent={installedOnly}
            aria-pressed={installedOnly}
            onclick={() => (installedOnly = !installedOnly)}
          >
            <Check size={11} strokeWidth={2.2} />{t("filter.installedOnly")}
          </button>
        {/if}
        {#if isNormalView && !inDiscover && !inDiagnostics}
          <button type="button" class="pill scan-btn" onclick={() => (scanOpen = true)}>
            <Search size={12} strokeWidth={2} />{t("scan.sidebarBtn")}
          </button>
          <SegmentedControl
            label={t("filter.all")}
            options={["all", "active", "inactive", "variable"].map((v) => ({ value: v, label: t("filter." + v) }))}
            value={filter}
            onselect={(v) => (filter = v)}
            width={66}
            height={22}
            font="600 11.5px"
          />
          <SegmentedControl
            label={t("sort.popularity")}
            options={["popularity", "recent", "alpha"].map((v) => ({ value: v, label: t("sort." + v) }))}
            value={sort}
            onselect={(v) => (sort = v)}
            width={64}
            height={22}
            font="600 11px"
          />
        {:else if isNormalView && inDiscover}
          {#if activeCustomSource}
            <button
              type="button"
              class="pill accent"
              title={t("empty.clear")}
              onclick={() => (discoverSource = "all")}
            >
              <Rss size={11} strokeWidth={2} />
              <span>{activeCustomSource.name}</span>
              <X size={11} strokeWidth={2.2} />
            </button>
          {/if}
          <button
            type="button"
            class="pill scan-btn"
            title={t("cs.title")}
            onclick={() => (customSourcesOpen = true)}
          >
            <Rss size={11} strokeWidth={2} />
            <span>+{t("src.manage")}</span>
            {#if discover.customSources.length}
              <span class="num">({discover.customSources.length})</span>
            {/if}
          </button>
          <SegmentedControl
            label={t("src.all")}
            options={[
              "all",
              "google",
              "fontshare",
              "fontsource",
              "adobe_dev",
              "fontsquirrel",
              ...(hasEnabledCustomSources ? ["custom"] : []),
            ].map((v) => ({ value: v, label: t("src." + v) }))}
            value={discoverSource.startsWith("custom:") ? "custom" : discoverSource}
            onselect={(v) => (discoverSource = v)}
            height={22}
            paddingX={8}
            font="600 10.5px"
          />
          <SegmentedControl
            label={t("filter.all")}
            options={["all", "installed", "uninstalled"].map((v) => ({ value: v, label: t("filter." + v) }))}
            value={discoverInstallFilter}
            onselect={(v) => (discoverInstallFilter = v)}
            height={22}
            paddingX={8}
            font="600 10.5px"
          />
          <SegmentedControl
            label={t("sort.popularity")}
            options={["popularity", "recent", "alpha"].map((v) => ({ value: v, label: t("sort." + v) }))}
            value={sort}
            onselect={(v) => (sort = v)}
            width={68}
            height={22}
            font="600 11px"
          />
        {/if}

        {#if activeCollection && isNormalView}
          <span class="sep"></span>
          <button type="button" class="pill primary" onclick={() => toggleCollectionFonts(true)}>{t("activateAll")}</button>
          <button type="button" class="pill" onclick={() => toggleCollectionFonts(false)}>{t("deactivate")}</button>
          <button
            type="button"
            class="pill"
            class:accent={activeCollection.pinned}
            title={activeCollection.pinned ? t("col.unpin") : t("col.pin")}
            onclick={() => togglePinCollection(activeCollection.id)}
          >
            <Pin size={12} strokeWidth={2} />
          </button>
          <button type="button" class="pill" title={t("col.exportZip")} onclick={exportCollectionZip}>
            <Archive size={12} strokeWidth={2} />{t("col.exportZip")}
          </button>
          <button
            type="button"
            class="pill"
            title={t("col.copyCss")}
            onclick={async () => {
              const names = activeCollection.fonts ?? [];
              const css = [
                `/* Cosmic Font Foundry — Collection: ${activeCollection.name} */`,
                ":root {",
                ...names.map((n, i) => `  --font-${i + 1}: "${n}", sans-serif;`),
                "}",
                "",
                "/* Tailwind CSS theme.extend.fontFamily */",
                JSON.stringify(
                  Object.fromEntries(names.map((n, i) => [`font-${i + 1}`, [`"${n}"`, "sans-serif"]])),
                  null,
                  2,
                ),
              ].join("\n");
              try {
                await navigator.clipboard.writeText(css);
                notify("ok", t("col.copiedCss"), `${names.length} fonts · ${activeCollection.name}`);
              } catch {
                notify("err", t("err.one"), "");
              }
            }}
          >
            <Code2 size={12} strokeWidth={2} />{t("col.copyCss")}
          </button>
          <button
            type="button"
            class="pill del"
            class:confirm={confirmingDelete === activeCollection.id}
            title={t("col.deleteHint")}
            onclick={onDeleteCollection}
          >
            <Trash2 size={12} strokeWidth={2} />{confirmingDelete === activeCollection.id ? t("col.confirmDelete") : t("col.delete")}
          </button>
        {/if}
      </div>
    </div>

    {#if activeFolder && isNormalView}
      <div class="folder-banner">
        <Folder size={16} strokeWidth={1.9} />
        <span>{t("folderInfo")}</span>
        <button type="button" class="pill white" onclick={addFolder}>{t("addAnother")}</button>
        <button type="button" class="pill ghost" onclick={removeFolder}>{t("stopWatching")}</button>
      </div>
    {/if}

    {#if activeProject && isNormalView}
      <div class="project-banner">
        <Sparkles size={15} strokeWidth={2} />
        <span>{t("proj.banner", { name: activeProject.name, n: activeProject.fonts.length })}</span>
        <button type="button" class="pill primary" onclick={activateProjectFonts}>{t("proj.activate")}</button>
      </div>
    {/if}

    {#if inDiagnostics && isNormalView}
      <div class="diag-summary">
        <span class="diag-stat"><b>{fonts.diagnostics.exact_duplicates}</b> {t("diag.exact")}</span>
        <span class="diag-stat"><b>{fonts.diagnostics.style_conflicts}</b> {t("diag.conflicts")}</span>
        <span class="diag-stat"><b>{fonts.diagnostics.corrupt_files}</b> {t("diag.corrupt")}</span>
        <span class="diag-hint">{t("diag.quarantineHint")}</span>
      </div>
    {/if}

    {#if detailFamily}
      <Specimen
        f={detailFamily}
        {sample}
        {fmt}
        fav={favSet.has(detailFamily.name)}
        locked={isLocked(detailFamily)}
        backLabel={section === "favorites" ? t("favorites") : activeCollection ? activeCollection.name : inDiagnostics ? t("sec.diagnostics") : t("sec.installed")}
        onback={() => (detailName = null)}
        ontoggle={() => toggleOne(detailFamily)}
        onfav={() => toggleFavorite(detailFamily.name)}
      />
    {:else if googleDetail}
      <Specimen
        googleFont={googleDetail}
        {sample}
        {fmt}
        fav={favSet.has(googleDetail.name)}
        installed={installedNamesLower.has(googleDetail.name.toLowerCase())}
        backLabel={inDiscover ? t("discoverCatalog") : t("favorites")}
        onback={() => (googleDetailName = null)}
        onfav={() => toggleFavorite(googleDetail.name)}
        onInstallGoogle={(name) => installGoogleFont(name, applyExternalList)}
      />
    {:else if comparedFamilies.length}
      <Compare
        families={comparedFamilies}
        {sample}
        {size}
        onback={() => (compareNames = [])}
        onremove={(n) => (compareNames = compareNames.length <= 2 ? [] : compareNames.filter((x) => x !== n))}
      />
    {:else if inForge}
      <Forge {sample} onInstalled={() => goTo("installed")} />
    {:else if inDiscover}
      {#if discover.list.length && googleList.length}
        <VirtualList items={googleList} height={Math.ceil(34 + size * 1.58)} getKey={(g) => `${g.source ?? "google"}::${g.source_id ?? g.name}::${g.name}`} row={googleRowSnippet} onVisible={onVisibleGoogle} padBottom={60} />
      {:else if discover.list.length}
        <EmptyState title={t("empty.none", { q: q.trim() })} text={t("empty.noneGoogle")} actionLabel={t("empty.clear")} onaction={() => (q = "")} />
      {:else if discover.error}
        <EmptyState icon={WifiOff} title={t("g.offline")} text={discover.error} actionLabel={t("g.retry")} onaction={() => loadCatalog(true)} />
      {:else}
        <SkeletonList />
      {/if}
    {:else if fonts.loading}
      {#if fonts.skeleton}<SkeletonList />{/if}
    {:else if !viewList.length}
      {#if inDiagnostics}
        <EmptyState icon={ShieldCheck} title={t("empty.diagTitle")} text={t("empty.diagText")} actionLabel={t("empty.favBtn")} onaction={() => goTo("installed")} />
      {:else if section === "favorites" && !ql && !baseList.length && !library.favorites.some((n) => discover.list.some((g) => g.name === n))}
        <EmptyState icon={Heart} title={t("empty.favTitle")} text={t("empty.favText")} actionLabel={t("empty.favBtn")} onaction={() => goTo("installed")} />
      {:else if ql}
        <EmptyState
          title={t("empty.none", { q: q.trim() })}
          text={t("empty.noneText")}
          actionLabel={t("empty.searchGoogle")}
          onaction={() => {
            const query = q;
            goTo("discover");
            q = query;
          }}
        />
      {:else if activeFolder && readingFolder === activeFolder}
        <EmptyState icon={LoaderCircle} spinning title={t("empty.reading")} text={t("empty.readingText")} />
      {:else}
        <EmptyState
          icon={activeCollection ? Layers : activeFolder ? FolderOpen : Type}
          title={t("empty.list")}
          text={activeCollection ? t("empty.colText") : activeFolder ? t("empty.folderText") : ""}
        />
      {/if}
    {:else if settings.view === "grid"}
      {#key section + cat + script}
        <VirtualList items={viewList} height={146} minWidth={176} gap={10} padX={24} padTop={4} getKey={(f) => (f.google ? "g:" : "") + f.name} row={favCardSnippet} onVisible={onVisibleFav} />
      {/key}
    {:else}
      {#key section + cat + script}
        <VirtualList items={viewList} height={Math.ceil(32 + size * 1.58)} getKey={(f) => (f.google ? "g:" : "") + f.name} row={favRowSnippet} onVisible={onVisibleFav} padBottom={80} />
      {/key}
    {/if}

    {#if sel.size && isNormalView && !inDiscover}
      <SelectionBar
        n={sel.size}
        collections={library.collections}
        bind:open={colOpen}
        onactivate={() => toggleMany([...sel], true)}
        ondeactivate={() => toggleMany([...sel], false)}
        onfavorites={() => {
          const names = [...sel];
          addToFavorites(names);
          sel.clear();
          notify("ok", t("ok.toFavorites", { n: names.length }), "");
        }}
        oncollection={(id) => {
          addToCol(id, [...sel]);
          sel.clear();
          colOpen = false;
        }}
        oncompare={() => {
          if (sel.size < 2 || sel.size > 4) return;
          compareNames = [...sel];
          sel.clear();
        }}
        onclear={() => sel.clear()}
      />
    {/if}

    {#if dropping}
      <div class="drop-veil">
        <div class="frame">
          <img src="/icon.svg" width="92" height="92" alt="" draggable="false" />
          <span class="dt">{t("dropTitle")}</span>
          <span class="dx">{settings.installFor === "all" ? t("dropTextAll") : t("dropText")}</span>
        </div>
      </div>
    {/if}
  </main>

  {#if drag.active}
    <div class="ghost-pill" style:left="{drag.x + 12}px" style:top="{drag.y + 10}px">
      {drag.names.length === 1 ? drag.names[0] : t("fontsN", { n: drag.names.length })}
    </div>
  {/if}

  {#if settingsTab}
    <Settings
      bind:tab={settingsTab}
      onclose={() => (settingsTab = null)}
      counts={{ installed: installedFonts.length, active: activeCount, favorites: library.favorites.length }}
      onMigrated={() => {
        if (inTauri) invokeCmd("watched_folders").then((l) => (folders = l)).catch(() => {});
      }}
    />
  {/if}

  {#if scanOpen}
    <SmartScanModal
      onclose={() => (scanOpen = false)}
      onImported={() => {}}
      onFoldersChanged={(l) => (folders = l)}
    />
  {/if}

  {#if customSourcesOpen}
    <CustomSourcesModal
      onclose={() => (customSourcesOpen = false)}
      onSelectSource={(srcTag) => {
        discoverSource = srcTag;
        if (section !== "discover") goTo("discover");
      }}
    />
  {/if}

  <Toasts />

  {#if splash}
    <div class="splash">
      <img src="/icon.svg" width="136" height="136" alt="" draggable="false" />
      <div class="splash-brand">
        <span class="brand-kicker">COSMIC</span>
        <span class="brand">Font Foundry<i>.</i></span>
      </div>
    </div>
  {/if}
</div>

<style>
  .window { position: relative; height: 100vh; overflow: hidden; display: grid; grid-template-columns: 224px minmax(0, 1fr); }
  main { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  header {
    flex: none;
    height: 54px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding-left: 24px;
    background: color-mix(in srgb, var(--win) 78%, transparent);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--line);
  }
  .spacer { flex: 1; align-self: stretch; }
  .field {
    height: 30px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px;
    border-radius: 8px;
    background: var(--field);
    box-shadow: 0 0 0 1px var(--line);
    color: var(--ink3);
    transition: box-shadow 0.15s, background-color 0.15s;
  }
  .field:focus-within {
    box-shadow: 0 0 0 1.5px var(--accent), var(--glow);
    color: var(--accent);
  }
  .search { flex: 0 1 175px; min-width: 120px; }
  .sample-box { flex: 1 1 180px; min-width: 110px; max-width: 280px; }
  .field input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; font: 500 12.5px var(--sans); color: var(--ink); }
  .a { font: 700 12px var(--sans); color: var(--ink3); }
  .reset { width: 16px; height: 16px; border: 0; border-radius: 50%; background: var(--line2); color: var(--win); padding: 0; display: flex; align-items: center; justify-content: center; flex: none; }
  .size-ctrl { flex: none; display: flex; align-items: center; gap: 6px; margin-left: 4px; }
  .size-ctrl .sm { font: 600 10px var(--sans); }
  .size-ctrl .lg { font: 600 15px var(--sans); }
  .size-ctrl .n { width: 22px; font-size: 11.5px; color: var(--ink3); }
  .range { -webkit-appearance: none; appearance: none; width: 72px; height: 2px; background: var(--line2); border-radius: 2px; outline: none; }
  .range::-webkit-slider-thumb { -webkit-appearance: none; width: 12px; height: 12px; border-radius: 50%; background: var(--accent); border: 0; cursor: pointer; }
  .fmt-group { flex: none; display: flex; align-items: center; gap: 3px; padding: 2px; border-radius: 7px; background: var(--chip); }
  .fmt-tog { height: 22px; min-width: 23px; padding: 0 6px; border: 0; border-radius: 5px; background: transparent; color: var(--ink2); font: 600 11px var(--sans); transition: background-color 0.14s, color 0.14s; }
  .fmt-tog.b { font-weight: 800; }
  .fmt-tog.i { font-style: italic; }
  .fmt-tog.u { text-decoration: underline; }
  .fmt-tog.s { text-decoration: line-through; }
  .fmt-tog:hover { color: var(--ink); background: var(--hov); }
  .fmt-tog.on { background: var(--accent); color: #fff; box-shadow: var(--pill-shadow); }
  .scan-btn { background: var(--cyan-soft); color: var(--accent-2); }
  .scan-btn:hover { background: var(--accent-2); color: #090b12; }
  .view-mode { margin-right: 12px; display: flex; }

  .title-bar { flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 10px 24px 8px; min-height: 46px; }
  .count { font-size: 10.5px; color: var(--ink3); }
  .right { margin-left: auto; display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 6px; }
  .sep { width: 1px; height: 16px; background: var(--line2); margin: 0 4px; }
  .pill {
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--chip);
    color: var(--ink);
    font: 600 11.5px var(--sans);
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
    transition: background-color 0.15s, color 0.15s;
  }
  .pill:hover { background: var(--line2); }
  .pill.primary { background: var(--accent); color: #fff; }
  .pill.primary:hover { filter: brightness(1.06); }
  .pill.accent { background: var(--accent-soft); color: var(--accent); }
  .pill.del { background: transparent; color: var(--ink2); padding: 0 10px 0 8px; }
  .pill.del:hover { background: var(--hov); }
  .pill.del.confirm { background: var(--danger); color: #fff; }
  .pill.white { height: 26px; padding: 0 12px; background: var(--win); box-shadow: 0 0 0 1px var(--line2); font-size: 12px; }
  .pill.ghost { height: 26px; padding: 0 12px; background: transparent; color: var(--ink2); font-size: 12px; }
  .pill.ghost:hover { background: var(--hov); color: var(--ink); }

  .folder-banner, .project-banner, .diag-summary {
    flex: none;
    margin: 0 24px 8px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--chip);
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--ink2);
  }
  .folder-banner span, .project-banner span { flex: 1; }
  .project-banner { background: var(--accent-soft); color: var(--ink); border: 1px solid rgba(91, 77, 247, 0.25); }
  .diag-summary { gap: 18px; }
  .diag-stat { font-size: 12px; color: var(--ink2); }
  .diag-stat b { color: var(--ink); font-weight: 700; }
  .diag-hint { margin-left: auto; font-size: 11.5px; color: var(--ink3); }

  .drop-veil { position: absolute; inset: 0; z-index: 9; padding: 14px; background: var(--win); animation: cf-fade 0.15s ease-out; }
  .frame { height: 100%; border: 1.5px dashed var(--accent); border-radius: 12px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; background: var(--accent-soft); }
  .frame img { display: block; }
  .dt { font-size: 17px; font-weight: 700; }
  .dx { font-size: 13.5px; color: var(--ink2); }

  .ghost-pill {
    position: fixed;
    z-index: 50;
    pointer-events: none;
    padding: 6px 10px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font: 600 12px var(--sans);
    white-space: nowrap;
  }

  .splash { position: absolute; inset: 0; z-index: 40; background-color: var(--win); background-image: var(--win-glow); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 20px; }
  .splash img { display: block; animation: cf-key 0.55s cubic-bezier(0.3, 1.2, 0.5, 1) both; filter: drop-shadow(0 18px 34px rgba(242, 107, 33, 0.34)); }
  .splash-brand { display: flex; flex-direction: column; align-items: center; gap: 5px; animation: cf-fade 0.3s ease-out 0.35s both; }
  .splash .brand-kicker { font: 700 10.5px/1 var(--mono); letter-spacing: 0.24em; color: var(--accent-2); }
  .splash .brand { font-size: 28px; }
</style>
