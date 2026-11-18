<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Search, Layers, Check, Circle } from "@lucide/svelte";
  import en from "./i18n/en.js";
  import es from "./i18n/es.js";

  let lang = $state(window.__COSMIC__?.settings?.language ?? "en");
  let families = $state([]);
  let collections = $state([]);
  let q = $state("");
  let activeIndex = $state(0);
  let inputEl = $state();

  const tr = (key) => (lang === "es" ? es : en)[key] ?? en[key] ?? key;

  async function refreshData() {
    try {
      const [fams, lib] = await Promise.all([invoke("saved_fonts"), invoke("read_library_store")]);
      families = Array.isArray(fams) ? fams : [];
      collections = Array.isArray(lib?.collections) ? lib.collections : [];
    } catch {}
  }

  function hideSelf() {
    getCurrentWindow().hide().catch(() => {});
  }

  const results = $derived.by(() => {
    const query = q.trim().toLowerCase();
    const out = [];
    for (const c of collections) {
      if (!query || c.name.toLowerCase().includes(query)) {
        const allActive =
          c.fonts.length > 0 &&
          c.fonts.every((name) => families.find((f) => f.name === name)?.active ?? false);
        out.push({
          kind: "collection",
          id: "col:" + c.id,
          title: c.name,
          subtitle: `${c.fonts.length} · ${tr("quick.collections")}`,
          active: allActive,
          fonts: c.fonts,
        });
      }
    }
    for (const f of families) {
      if (!query || f.name.toLowerCase().includes(query)) {
        out.push({
          kind: "family",
          id: "fam:" + f.name,
          title: f.name,
          subtitle: `${f.styles.length} · ${tr("origin." + f.origin)}`,
          active: f.active,
          essential: f.essential,
          fonts: [f.name],
        });
      }
      if (out.length >= 24) break;
    }
    return out;
  });

  $effect(() => {
    if (activeIndex >= results.length) activeIndex = Math.max(0, results.length - 1);
  });

  async function toggleItem(item) {
    if (!item || !item.fonts.length || item.essential) return;
    try {
      const res = await invoke("toggle_fonts", {
        families: item.fonts,
        activate: !item.active,
        systemUnlocked: false,
      });
      if (Array.isArray(res?.families)) families = res.families;
    } catch {}
  }

  function onKeyDown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      hideSelf();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (results.length) activeIndex = (activeIndex + 1) % results.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (results.length) activeIndex = (activeIndex - 1 + results.length) % results.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (results[activeIndex]) toggleItem(results[activeIndex]);
    }
  }

  $effect(() => {
    document.documentElement.style.background = document.body.style.background = "transparent";
    refreshData();
    setTimeout(() => inputEl?.focus(), 20);
    const unsubs = [
      listen("quick-opened", () => {
        q = "";
        activeIndex = 0;
        refreshData();
        setTimeout(() => inputEl?.focus(), 20);
      }),
      listen("tray-status", ({ payload }) => {
        if (payload?.theme) document.documentElement.dataset.theme = payload.theme;
        if (payload?.language) {
          lang = payload.language;
          document.documentElement.lang = lang === "es" ? "es" : "en";
        }
      }),
      listen("inventory-updated", ({ payload }) => {
        if (Array.isArray(payload)) families = payload;
      }),
    ];
    return () => unsubs.forEach((p) => p.then((u) => u()));
  });
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="shell">
  <div class="box">
    <div class="search-row">
      <Search size={16} strokeWidth={2} />
      <input
        bind:this={inputEl}
        bind:value={q}
        placeholder={tr("quick.placeholder")}
        spellcheck="false"
      />
    </div>

    <div class="list">
      {#if !results.length}
        <div class="empty">{tr("quick.empty")}</div>
      {:else}
        {#each results as item, i (item.id)}
          <button
            type="button"
            class="row"
            class:selected={i === activeIndex}
            onmouseenter={() => (activeIndex = i)}
            onclick={() => toggleItem(item)}
          >
            <span class="state" class:on={item.active}>
              {#if item.active}<Check size={12} strokeWidth={2.5} />{:else}<Circle size={12} strokeWidth={1.8} />{/if}
            </span>
            <span class="info">
              <span class="title">
                {#if item.kind === "collection"}<Layers size={12} strokeWidth={2} />{/if}
                {item.title}
              </span>
              <span class="sub">{item.subtitle}</span>
            </span>
            <span class="action">{item.active ? tr("deactivate") : tr("activate")}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .shell { height: 100vh; padding: 10px; display: flex; align-items: stretch; }
  .box {
    width: 100%;
    border-radius: 14px;
    background: var(--win);
    color: var(--ink);
    border: 1px solid var(--line2);
    box-shadow: 0 18px 44px rgba(0, 0, 0, 0.28);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .search-row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
    color: var(--accent);
  }
  .search-row input {
    flex: 1;
    border: 0;
    outline: 0;
    background: transparent;
    font: 500 14.5px var(--sans);
    color: var(--ink);
  }
  .list { flex: 1; overflow-y: auto; padding: 6px; display: flex; flex-direction: column; gap: 2px; }
  .empty { padding: 48px 16px; text-align: center; color: var(--ink3); font-size: 13px; }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    text-align: left;
    transition: background-color 0.1s;
  }
  .row.selected { background: var(--sel); }
  .state {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--ink3);
    background: var(--chip);
    flex: none;
  }
  .state.on { background: var(--accent); color: #fff; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .title { display: flex; align-items: center; gap: 6px; font: 600 13.5px var(--sans); color: var(--ink); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { font-size: 11px; color: var(--ink3); }
  .action { font: 600 11px var(--sans); color: var(--accent); opacity: 0.85; }
</style>
