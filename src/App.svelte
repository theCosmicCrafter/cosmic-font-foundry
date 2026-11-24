<script>
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    Search, ScanSearch, List, LayoutGrid, Folder, FolderOpen, Trash2, X, Heart,
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
  import VisualFontFinderModal from "./components/VisualFontFinderModal.svelte";
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
    copyFamiliesToVault, installFamiliesToWindows,
  } from "./lib/fonts.svelte.js";
  import {
    library, initLibrary, toggleFavorite, addToFavorites, createCollection,
    renameCollection, togglePinCollection, deleteCollection, addFontsToCollection,
    tagsForFamily, addTagToFamily, autoCreateOsCollections, upsertCollectionWithFonts,
    createSmartCollection, deleteSmartCollection, matchesSmartCollection,
  } from "./lib/library.svelte.js";
  import { discover, loadCatalog, requestGooglePreviews, preloadDiscover, installGoogleFont } from "./lib/discover.svelte.js";
  import { drag, startDrag } from "./lib/drag.svelte.js";
  import { shortcutCombo } from "./lib/shortcuts.js";
  import { parseSearchQuery, scoreFontMatch, QUICK_SEARCH_THEMES } from "./lib/smart-search.js";

  // section: installed | discover | favorites | diagnostics | col:<id> | dir:<path>
  let section = $state(settings.onOpen === "favorites" ? "favorites" : "installed");
  let cat = $state(null);
  let script = $state(null);
  let q = $state("");
  let customSample = $state("");
  let filter = $state("all"); // all | active | inactive | variable
  let installedOnly = $state(false);
  let commercialOnly = $state(false);
  let discoverInstallFilter = $state("all"); // all | installed | uninstalled
  let localSort = $state("alpha");
  let discoverSort = $state("popularity");
  let discoverSource = $state("all"); // all | google | fontshare | fontsource
  const sel = new SvelteSet();
  let detailName = $state(null);
  let googleDetailName = $state(null);
  let forgeForkTarget = $state(null);
  let compareNames = $state([]);
  let settingsTab = $state(null);
  let scanOpen = $state(false);
  let customSourcesOpen = $state(false);
  let visualFinderOpen = $state(false);
  let outlineDupGroups = $state([]);
  let outlineDupBusy = $state(false);
  let outlineDupScanned = $state(false);

  async function scanRenamedOutlineDuplicates() {
    if (!inTauri || outlineDupBusy) return;
    outlineDupBusy = true;
    try {
      const groups = await invokeCmd("foundry_find_outline_duplicates", {});
      outlineDupGroups = Array.isArray(groups) ? groups : [];
      outlineDupScanned = true;
    } catch (err) {
      notify("err", t("err.one"), String(err));
    } finally {
      outlineDupBusy = false;
    }
  }
  let fmt = $state({ bold: false, italic: false, underline: false, strike: false, uppercase: false });
  let colOpen = $state(false);
  let tagOpen = $state(false);
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
  const inAllCollections = $derived(section === "col:all");
  const activeSmartCollection = $derived(
    section.startsWith("smart:")
      ? library.smartCollections.find((sc) => "smart:" + sc.id === section)
      : null,
  );
  const activeCollection = $derived(
    section.startsWith("col:") && !inAllCollections
      ? library.collections.find((c) => "col:" + c.id === section)
      : null,
  );
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
  const allCollectionFontsSet = $derived.by(() => {
    const set = new Set();
    for (const c of library.collections) {
      for (const n of c.fonts) set.add(n.toLowerCase());
    }
    return set;
  });
  const activeColSet = $derived(activeCollection ? new Set(activeCollection.fonts.map((n) => n.toLowerCase())) : null);
  const activeStateMap = $derived(new Map(fonts.list.map((f) => [f.name.toLowerCase(), f.active])));

  // ---- Smart Search (Fuzzy + Style + Theme + Geometric Vibe DNA + All Locations) ----
  const ql = $derived(q.trim().toLowerCase());
  const parsedQuery = $derived(parseSearchQuery(q));
  let vibeScoreMap = $state(new Map());
  let vibeReqSeq = 0;

  $effect(() => {
    const queryStr = parsedQuery.norm;
    const shouldRun = Boolean(queryStr && queryStr.length >= 3 && !inDiscover && !inForge && inTauri);
    if (!shouldRun) {
      if (vibeScoreMap.size) vibeScoreMap = new Map();
      return;
    }
    const seq = ++vibeReqSeq;
    const timer = setTimeout(async () => {
      try {
        const hits = await invokeCmd("foundry_vibe_search", { query: queryStr, topK: 24 });
        if (seq !== vibeReqSeq || !Array.isArray(hits)) return;
        const next = new Map();
        for (const hit of hits) {
          if (hit?.family && Number(hit.score) >= 0.48) {
            next.set(String(hit.family).toLowerCase(), Number(hit.score));
          }
        }
        vibeScoreMap = next;
      } catch {
        if (seq === vibeReqSeq && vibeScoreMap.size) vibeScoreMap = new Map();
      }
    }, 220);
    return () => clearTimeout(timer);
  });

  // When searching from the default Installed view (without Installed Only locked),
  // automatically search across ALL local font locations (Windows, User, Vault, Exports, Watched Folders).
  const searchingAllLocations = $derived(Boolean(ql && section === "installed" && !installedOnly));

  // ---- Filtered lists ----
  const baseList = $derived.by(() => {
    const l = fonts.list;
    if (section === "all" || searchingAllLocations) return l;
    if (section === "favorites") return l.filter((f) => favSet.has(f.name));
    if (section === "diagnostics") return l.filter((f) => f.error || f.has_conflicts || (f.duplicates?.length ?? 0) > 0);
    if (activeSmartCollection) return l.filter((f) => matchesSmartCollection(f, activeSmartCollection.rules, tagsForFamily));
    if (inAllCollections) return l.filter((f) => allCollectionFontsSet.has(f.name.toLowerCase()));
    if (activeColSet) return l.filter((f) => activeColSet.has(f.name.toLowerCase()));
    if (activeFolder) return l.filter((f) => inFolder(f, activeFolder));
    return l.filter((f) => f.origin !== "dir" && isVisible(f));
  });

  const localScored = $derived.by(() => {
    const out = [];
    for (const f of baseList) {
      if (installedOnly && f.origin === "dir") continue;
      if (commercialOnly && f.license_tier !== "free_commercial") continue;
      if (cat && f.category !== cat) continue;
      if (script && !(f.scripts ?? []).includes(script)) continue;
      if (filter === "variable" && !f.is_variable) continue;
      if (filter === "active" && !(f.active || isLocked(f))) continue;
      if (filter === "inactive" && (f.active || isLocked(f))) continue;
      const score = ql
        ? scoreFontMatch(f, parsedQuery, { tags: tagsForFamily(f.name), vibeScoreMap })
        : 1;
      if (score <= 0) continue;
      out.push({ item: f, score });
    }
    return out;
  });

  const filteredLocal = $derived(localScored.map((e) => e.item));

  const installedNamesLower = $derived(
    new Set(fonts.list.filter((f) => f.origin !== "dir").map((f) => f.name.toLowerCase())),
  );
  const discoverMetaMap = $derived(new Map(discover.list.map((g) => [g.name.toLowerCase(), g])));

  const favGoogle = $derived.by(() => {
    if (section !== "favorites" || installedOnly || filter === "active") return [];
    const out = [];
    for (const g of discover.list) {
      if (!favSet.has(g.name) || installedNamesLower.has(g.name.toLowerCase())) continue;
      if (cat && g.category !== cat) continue;
      if (script && !(g.scripts ?? []).includes(script)) continue;
      if (filter === "variable" && !g.is_variable) continue;
      const score = ql ? scoreFontMatch(g, parsedQuery) : 1;
      if (score <= 0) continue;
      out.push({ item: g, score });
    }
    return out;
  });

  const viewList = $derived.by(() => {
    const entries = favGoogle.length
      ? [...localScored, ...favGoogle.map(({ item: g, score }) => ({ item: { google: g, name: g.name }, score }))]
      : [...localScored];

    const tieBreak = (a, b) => {
      if (localSort === "popularity") {
        const pa = a.google?.popularity ?? discoverMetaMap.get(a.name.toLowerCase())?.popularity ?? 999999;
        const pb = b.google?.popularity ?? discoverMetaMap.get(b.name.toLowerCase())?.popularity ?? 999999;
        if (pa !== pb) return pa - pb;
      } else if (localSort === "recent") {
        const da = a.google?.date ?? discoverMetaMap.get(a.name.toLowerCase())?.date ?? "";
        const db = b.google?.date ?? discoverMetaMap.get(b.name.toLowerCase())?.date ?? "";
        if (da !== db) return db > da ? 1 : -1;
      }
      return a.name.localeCompare(b.name);
    };

    entries.sort((a, b) => {
      if (ql && b.score !== a.score) return b.score - a.score;
      return tieBreak(a.item, b.item);
    });
    return entries.map((e) => e.item);
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
    const scored = [];
    for (const g of discover.list) {
      const isInst = installedNamesLower.has(g.name.toLowerCase());
      if (installedOnly && !isInst) continue;
      if (discoverInstallFilter === "installed" && !isInst) continue;
      if (discoverInstallFilter === "uninstalled" && isInst) continue;
      const srcOk =
        discoverSource === "all" ||
        (discoverSource === "custom"
          ? (g.source ?? "").startsWith("custom:")
          : (g.source ?? "google") === discoverSource);
      if (!srcOk) continue;
      if (cat && g.category !== cat) continue;
      if (script && !(g.scripts ?? []).includes(script)) continue;
      const score = ql ? scoreFontMatch(g, parsedQuery) : 1;
      if (score <= 0) continue;
      scored.push({ item: g, score });
    }
    scored.sort((a, b) => {
      if (ql && b.score !== a.score) return b.score - a.score;
      if (discoverSort === "popularity") return a.item.popularity - b.item.popularity;
      if (discoverSort === "recent") return b.item.date > a.item.date ? 1 : b.item.date < a.item.date ? -1 : 0;
      return a.item.name.localeCompare(b.item.name);
    });
    return scored.map((e) => e.item);
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
  const collectionCounts = $derived.by(() => {
    const out = {};
    for (const c of library.collections) {
      const set = new Set(c.fonts.map((n) => n.toLowerCase()));
      out[c.id] = fonts.list.filter((f) => set.has(f.name.toLowerCase())).length;
    }
    return out;
  });
  const smartCounts = $derived.by(() => {
    const out = {};
    for (const sc of library.smartCollections) {
      out[sc.id] = fonts.list.filter((f) => matchesSmartCollection(f, sc.rules, tagsForFamily)).length;
    }
    return out;
  });
  const canInstallSelectionToWindows = $derived(
    [...sel].some((name) => fonts.list.find((f) => f.name === name)?.origin === "dir"),
  );
  const allCollectionsCount = $derived(
    fonts.list.filter((f) => allCollectionFontsSet.has(f.name.toLowerCase())).length,
  );
  const counts = $derived({
    all: fonts.list.length,
    installed: installedFonts.length,
    discover: googleCountLabel,
    favorites: library.favorites.length,
    diagnostics: diagCount,
    allCollections: allCollectionsCount,
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
                  : section === "all"
                    ? t("sec.all")
                    : activeSmartCollection
                      ? activeSmartCollection.name
                      : inAllCollections
                        ? t("col.all")
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
    tagOpen = false;
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
  function confirmAction(id, onConfirm) {
    if (confirmingDelete !== id) {
      confirmingDelete = id;
      clearTimeout(deleteTimer);
      deleteTimer = setTimeout(() => (confirmingDelete = null), 3000);
      return;
    }
    confirmingDelete = null;
    onConfirm();
  }

  function onDeleteCollection() {
    if (!activeCollection) return;
    confirmAction(activeCollection.id, () => {
      deleteCollection(activeCollection.id);
      goTo("installed");
      notify("ok", t("ok.colDeleted"), t("ok.fontsUntouched"));
    });
  }

  function onDeleteSmartCollection() {
    if (!activeSmartCollection) return;
    confirmAction(activeSmartCollection.id, () => {
      deleteSmartCollection(activeSmartCollection.id);
      goTo("installed");
      notify("ok", t("smart.deleted"), "");
    });
  }

  const hasActiveSmartRules = $derived(
    isNormalView && !inDiscover && !inDiagnostics && !activeSmartCollection &&
    Boolean(cat || script || commercialOnly || installedOnly || filter !== "all" || ql),
  );

  function saveCurrentAsSmartView() {
    const parts = [];
    if (commercialOnly) parts.push("Commercial");
    if (filter !== "all") parts.push(t("filter." + filter));
    if (cat) parts.push(t("cat." + cat));
    if (script) parts.push(script);
    if (ql) parts.push(`“${q.trim()}”`);
    if (installedOnly) parts.push(t("filter.installedOnly"));
    const rules = {
      ...(cat ? { category: cat } : {}),
      ...(script ? { script } : {}),
      ...(commercialOnly ? { commercialOnly: true } : {}),
      ...(installedOnly ? { installedOnly: true } : {}),
      ...(filter !== "all" ? { filter } : {}),
      ...(ql ? { query: q.trim() } : {}),
      ...(activeFolder ? { origin: "dir" } : {}),
    };
    const id = createSmartCollection(parts.join(" · ") || "Smart View", rules);
    if (id) {
      notify("ok", t("smart.saved"), parts.join(" · "));
      goTo("smart:" + id);
    }
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
          const paths = (p.paths ?? []).filter(Boolean);
          if (!paths.length) return;
          const allUsers = settings.installFor === "all";
          const o = await installFontFiles(paths, allUsers);
          if (!o) return;
          if (!o.installed.length && !o.errors?.length) {
            return notify("err", t("err.notFont"), t("err.notFontText"));
          }
          if (!o.installed.length) return;
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
    {collectionCounts}
    smartCollections={library.smartCollections}
    {smartCounts}
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
        <label class="field search" title="Fuzzy name, style (bold, serif, mono, variable), or theme (halloween, retro, modern, coding)">
          <Search size={13} strokeWidth={2} />
          <input bind:this={searchInput} bind:value={q} placeholder={inDiscover ? t("searchGoogle") : t("search")} spellcheck="false" />
          {#if q}
            <button type="button" class="reset" title="Clear search" aria-label="Clear search" onclick={() => (q = "")}><X size={9} strokeWidth={3} /></button>
          {/if}
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
      {#if isNormalView}
        <span class="count num">{inDiscover ? googleList.length : viewList.length}</span>
        {#if searchingAllLocations}
          <span class="scope-badge" title="Searching Windows Fonts, User Fonts, Vault, Exports, and all Watched Folders">All locations</span>
        {/if}
      {/if}
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
          {#if !inDiscover}
            <button
              type="button"
              class="pill"
              class:accent={commercialOnly}
              aria-pressed={commercialOnly}
              onclick={() => (commercialOnly = !commercialOnly)}
            >
              <ShieldCheck size={11} strokeWidth={2.2} />{t("filter.commercialOnly")}
            </button>
          {/if}
        {/if}
        {#if isNormalView && !inDiscover && !inDiagnostics}
          <button type="button" class="pill scan-btn" onclick={() => (visualFinderOpen = true)}>
            <ScanSearch size={12} strokeWidth={2} />{t("visual.btn")}
          </button>
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
            value={localSort}
            onselect={(v) => (localSort = v)}
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
              "bunny",
              "fontshare",
              "fontsource",
              "adobe_dev",
              "fontsquirrel",
              "nerdfonts",
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
            value={discoverSort}
            onselect={(v) => (discoverSort = v)}
            width={68}
            height={22}
            font="600 11px"
          />
        {/if}

        {#if hasActiveSmartRules}
          <button type="button" class="pill scan-btn" onclick={saveCurrentAsSmartView}>
            <Sparkles size={11} strokeWidth={2} />{t("smart.save")}
          </button>
        {/if}

        {#if activeSmartCollection && isNormalView}
          <button
            type="button"
            class="pill del"
            class:confirm={confirmingDelete === activeSmartCollection.id}
            onclick={onDeleteSmartCollection}
          >
            <Trash2 size={12} strokeWidth={2} />{confirmingDelete === activeSmartCollection.id ? t("col.confirmDelete") : t("smart.delete")}
          </button>
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

    {#if isNormalView && !inDiagnostics}
      <div class="theme-strip" aria-label="Quick style and theme search">
        <span class="theme-strip-lbl">Themes:</span>
        {#each QUICK_SEARCH_THEMES as th (th.query)}
          {@const active = ql === th.query}
          <button
            type="button"
            class="theme-chip"
            class:active
            onclick={() => (q = active ? "" : th.query)}
          >
            {th.label}
          </button>
        {/each}
        {#if ql && parsedQuery.themes.length}
          <span class="theme-match-hint">
            Smart match: {parsedQuery.themes.map((th) => th.label).join(" + ")}
          </span>
        {/if}
      </div>
    {/if}

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
        <button
          type="button"
          class="pill scan-btn"
          disabled={outlineDupBusy || !inTauri}
          onclick={scanRenamedOutlineDuplicates}
        >
          <Layers size={12} strokeWidth={2} />
          {outlineDupBusy ? t("diag.outlineDupScanning") : t("diag.outlineDupBtn")}
        </button>
      </div>
      {#if outlineDupScanned}
        <div class="dup-panel">
          {#if outlineDupGroups.length === 0}
            <span class="dup-empty">{t("diag.outlineDupNone")}</span>
          {:else}
            <span class="dup-title">{t("diag.outlineDupFound", { n: outlineDupGroups.length })}</span>
            <div class="dup-list">
              {#each outlineDupGroups.slice(0, 12) as grp (grp.digest)}
                <div class="dup-group">
                  <span class="num dup-hash">{grp.digest.slice(0, 10)} · {grp.glyphs_hashed}g</span>
                  <div class="dup-files">
                    {#each grp.files as item (item.path)}
                      <button
                        type="button"
                        class="dup-chip"
                        title={item.path}
                        onclick={() => (detailName = item.family_hint)}
                      >
                        {item.family_hint || item.path}
                      </button>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    {#if detailFamily}
      <Specimen
        f={detailFamily}
        {sample}
        {fmt}
        fav={favSet.has(detailFamily.name)}
        locked={isLocked(detailFamily)}
        backLabel={section === "favorites" ? t("favorites") : section === "all" ? t("sec.all") : activeSmartCollection ? activeSmartCollection.name : inAllCollections ? t("col.all") : activeCollection ? activeCollection.name : inDiagnostics ? t("sec.diagnostics") : t("sec.installed")}
        onback={() => (detailName = null)}
        ontoggle={() => toggleOne(detailFamily)}
        onfav={() => toggleFavorite(detailFamily.name)}
        onopenfamily={(name) => (detailName = name)}
        onfork={(fam) => {
          forgeForkTarget = { path: fam.file, name: fam.name, license_tier: fam.license_tier };
          detailName = null;
          goTo("forge");
        }}
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
      <Forge {sample} forkFont={forgeForkTarget} onForkConsumed={() => (forgeForkTarget = null)} onInstalled={() => goTo("installed")} />
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
        bind:tagOpen={tagOpen}
        canInstallToWindows={canInstallSelectionToWindows}
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
        ontag={(tagStr) => {
          const names = [...sel];
          for (const n of names) addTagToFamily(n, tagStr);
          sel.clear();
          tagOpen = false;
          notify("ok", t("sel.tagged", { n: names.length, t: tagStr }), "");
        }}
        onvault={async () => {
          const names = [...sel];
          sel.clear();
          const res = await copyFamiliesToVault(names);
          if (res?.report) {
            notify("ok", t("sel.vaultDone", { n: res.report.imported, dup: res.report.skipped_duplicates }), "");
          }
        }}
        oninstall={async () => {
          const names = [...sel];
          sel.clear();
          const allUsers = settings.installFor === "all";
          const o = await installFamiliesToWindows(names, allUsers);
          if (o?.installed?.length) {
            const title = o.installed.length === 1 ? t("ok.installed", { n: o.installed[0] }) : t("ok.installedMany", { n: o.installed.length });
            notify("ok", title, allUsers ? t("ok.forAll") : t("ok.forUser"));
          }
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

  <VisualFontFinderModal
    open={visualFinderOpen}
    fonts={fonts.list}
    onClose={() => (visualFinderOpen = false)}
    onSelectFamily={(name) => (detailName = name)}
    onForkInFoundry={(target) => {
      forgeForkTarget = target;
      goTo("forge");
    }}
  />

  <Toasts />

  {#if splash}
    <div class="splash">
      <img src="/icon.svg" width="136" height="136" alt="" draggable="false" />
      <div class="splash-brand">
        <span class="brand-kicker">COSMIC</span>
        <span class="brand">Font Foundry<i>.</i></span>
        <span class="brand-sub">ORGANIZE · DOWNLOAD · GENERATE</span>
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
  .search { flex: 0 1 255px; min-width: 155px; }
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

  .title-bar { flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 10px 24px 6px; min-height: 44px; }
  .count { font-size: 10.5px; color: var(--ink3); }
  .scope-badge {
    font: 600 10px var(--mono);
    letter-spacing: 0.04em;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--cyan-soft);
    color: var(--accent-2);
  }
  .theme-strip {
    flex: none;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 24px 8px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .theme-strip::-webkit-scrollbar { display: none; }
  .theme-strip-lbl {
    font: 700 9.5px var(--mono);
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--ink3);
    margin-right: 2px;
    flex: none;
  }
  .theme-chip {
    flex: none;
    height: 22px;
    padding: 0 9px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--field);
    color: var(--ink2);
    font: 500 11px var(--sans);
    transition: all 0.14s;
  }
  .theme-chip:hover { color: var(--ink); border-color: var(--line2); background: var(--chip); }
  .theme-chip.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .theme-match-hint {
    margin-left: auto;
    flex: none;
    font: 500 10.5px var(--mono);
    color: var(--accent-2);
  }
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
  .diag-summary { gap: 18px; flex-wrap: wrap; }
  .diag-stat { font-size: 12px; color: var(--ink2); }
  .diag-stat b { color: var(--ink); font-weight: 700; }
  .diag-hint { margin-left: auto; font-size: 11.5px; color: var(--ink3); }
  .dup-panel {
    flex: none;
    margin: 0 24px 10px;
    padding: 10px 14px;
    border-radius: 9px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dup-empty { font-size: 12px; color: var(--ink3); }
  .dup-title { font-size: 12px; font-weight: 700; color: var(--ink); }
  .dup-list { display: flex; flex-direction: column; gap: 6px; max-height: 150px; overflow-y: auto; }
  .dup-group { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .dup-hash { font-size: 11px; color: var(--ink3); }
  .dup-files { display: flex; gap: 6px; flex-wrap: wrap; }
  .dup-chip {
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 600 11px var(--sans);
  }
  .dup-chip:hover { border-color: var(--accent); background: var(--accent-soft); }

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
  .splash .brand-kicker {
    font: 700 10.5px/1 var(--mono);
    letter-spacing: 0.28em;
    background: linear-gradient(90deg, #ffd166, #ff7b47, #d946ef);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .splash .brand { font-size: 28px; }
  .splash .brand-sub { font: 600 9.5px/1 var(--mono); letter-spacing: 0.22em; color: var(--ink3); margin-top: 4px; }
</style>
