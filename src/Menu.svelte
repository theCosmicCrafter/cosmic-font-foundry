<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { AppWindow, Search, Compass, SlidersHorizontal, Power, Zap, Check } from "@lucide/svelte";
  import en from "./i18n/en.js";
  import es from "./i18n/es.js";

  let status = $state({ status: "", folders: 0, theme: "", language: "en", shortcut: "Ctrl+Shift+F", pinned: [] });
  const tr = (key) => (status.language === "es" ? es : en)[key] ?? en[key] ?? key;
  let openCount = $state(0);

  const act = (action) => invoke("tray_action", { action });

  async function togglePinned(col) {
    if (!col.fonts?.length) return;
    await invoke("toggle_fonts", {
      families: col.fonts,
      activate: !col.all_active,
      systemUnlocked: false,
    }).catch(() => {});
  }

  $effect(() => {
    document.documentElement.style.background = document.body.style.background = "transparent";
    const apply = (next) => {
      status = next;
      if (next.theme) document.documentElement.dataset.theme = next.theme;
      document.documentElement.lang = next.language === "es" ? "es" : "en";
    };
    invoke("get_tray_status").then(apply).catch(() => {});
    const unsubs = [
      listen("tray-status", ({ payload }) => apply(payload)),
      listen("menu-opened", () => openCount++),
    ];
    const onKey = (ev) => ev.key === "Escape" && act("close");
    window.addEventListener("keydown", onKey);
    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
      window.removeEventListener("keydown", onKey);
    };
  });

  const ITEMS = [
    { action: "open", icon: AppWindow, key: "menu.open" },
    { action: "quick", icon: Zap, key: "menu.quick" },
    { action: "search", icon: Search, key: "menu.search" },
    { action: "discover", icon: Compass, key: "menu.discover" },
    { action: "settings", icon: SlidersHorizontal, key: "settings" },
  ];
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="frame" onclick={(ev) => ev.target === ev.currentTarget && act("close")}>
  {#key openCount}
    <div class="menu" role="menu">
      <div class="head">
        <img src="/icon.svg" width="26" height="26" alt="" draggable="false" />
        <div class="texts">
          <span class="brand">Cosmic Font Foundry<i>.</i></span>
          <span class="status num"><span class="dot" class:live={status.folders > 0}></span>{status.status || "Cosmic Font Foundry"}</span>
        </div>
      </div>

      {#if status.pinned?.length}
        <div class="sep"></div>
        <span class="pinned-title">{tr("menu.pinned")}</span>
        {#each status.pinned as col (col.id)}
          <button type="button" role="menuitem" class="item" onclick={() => togglePinned(col)}>
            <span class="pin-state" class:on={col.all_active}>
              {#if col.all_active}<Check size={11} strokeWidth={2.5} />{/if}
            </span>
            <span class="pin-name">{col.name}</span>
            <span class="pin-count num">{col.fonts.length}</span>
          </button>
        {/each}
      {/if}

      <div class="sep"></div>
      {#each ITEMS as it (it.action)}
        <button type="button" role="menuitem" class="item" onclick={() => act(it.action)}>
          <it.icon size={15} strokeWidth={1.9} />
          <span class="lbl">{tr(it.key)}</span>
          {#if it.action === "quick" && status.shortcut}
            <kbd class="kbd">{status.shortcut}</kbd>
          {/if}
        </button>
      {/each}
      <div class="sep"></div>
      <button type="button" role="menuitem" class="item exit" onclick={() => act("exit")}>
        <Power size={15} strokeWidth={1.9} />{tr("menu.quit")}
      </button>
    </div>
  {/key}
</div>

<style>
  .frame { height: 100vh; display: flex; align-items: flex-end; justify-content: flex-end; padding: 8px; }
  .menu {
    width: 100%;
    padding: 5px;
    background: var(--win);
    color: var(--ink);
    border-radius: 12px;
    border: 1px solid var(--line2);
    animation: cf-enter 0.16s cubic-bezier(0.2, 0.8, 0.2, 1);
    transform-origin: bottom right;
  }
  @keyframes cf-enter { from { opacity: 0; transform: translateY(6px) scale(0.98); } }

  .head { display: flex; align-items: center; gap: 10px; padding: 8px 9px 7px; }
  .head img { display: block; flex: none; }
  .texts { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .brand { font-size: 14px; }
  .status { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--ink3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot { width: 6px; height: 6px; flex: none; border-radius: 50%; background: var(--off); }
  .dot.live { background: var(--ok); }

  .pinned-title { display: block; padding: 4px 10px 2px; font-size: 9.5px; font-weight: 700; letter-spacing: 0.08em; color: var(--ink3); }
  .pin-state { width: 15px; height: 15px; border-radius: 50%; border: 1.5px solid var(--line2); display: grid; place-items: center; flex: none; color: #fff; }
  .pin-state.on { background: var(--accent); border-color: var(--accent); }
  .pin-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pin-count { font-size: 11px; color: var(--ink3); }

  .sep { height: 1px; margin: 4px 6px; background: var(--line); }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink);
    font: 500 12.5px var(--sans);
    text-align: left;
    transition: background-color 0.12s, color 0.12s;
  }
  .lbl { flex: 1; }
  .kbd { font: 600 10px var(--mono); padding: 1px 5px; border-radius: 4px; background: var(--chip); color: var(--ink3); }
  .item :global(svg) { color: var(--ink3); transition: color 0.12s; }
  .item:hover, .item:focus-visible { background: var(--chip); outline: none; }
  .item:hover :global(svg) { color: var(--ink); }
  .exit { color: var(--ink2); }
</style>
