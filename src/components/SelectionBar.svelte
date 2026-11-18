<script>
  import { CircleCheck, Circle, Heart, FolderPlus, Columns2, X, Tag, Archive, Download } from "@lucide/svelte";
  import { t } from "../lib/i18n.svelte.js";

  let {
    n,
    collections,
    open = $bindable(),
    tagOpen = $bindable(false),
    canInstallToWindows = false,
    onactivate,
    ondeactivate,
    onfavorites,
    oncollection,
    ontag,
    onvault,
    oninstall,
    oncompare,
    onclear,
  } = $props();
  const comparable = $derived(n >= 2 && n <= 4);
  const compareTitle = $derived(n < 2 ? t("cmp.min") : n > 4 ? t("cmp.max") : t("compare"));
  let tagDraft = $state("");

  function submitTag() {
    const clean = tagDraft.trim();
    if (!clean) return;
    ontag?.(clean);
    tagDraft = "";
    tagOpen = false;
  }
</script>

<div class="bar">
  <span class="count">
    <span class="n num">{n}</span>
    <span>{n === 1 ? t("selectedOne") : t("selectedMany")}</span>
  </span>
  <span class="sep"></span>
  <button type="button" class="primary" onclick={onactivate}><CircleCheck size={15} strokeWidth={1.9} />{t("activate")}</button>
  <button type="button" onclick={ondeactivate}><Circle size={15} strokeWidth={1.9} />{t("deactivate")}</button>
  <span class="sep"></span>
  <button type="button" onclick={onfavorites}><Heart size={15} strokeWidth={1.9} />{t("favorites")}</button>
  <span class="pop">
    <button type="button" onclick={() => ((open = !open), (tagOpen = false))} aria-expanded={open}><FolderPlus size={15} strokeWidth={1.9} />{t("collection")}</button>
    {#if open}
      <span class="menu">
        <span class="title">{t("addTo")}</span>
        {#each collections as c (c.id)}
          <button type="button" class="op" onclick={() => oncollection(c.id)}>{c.name}</button>
        {/each}
        <button type="button" class="op new" onclick={() => oncollection(null)}>+ {t("col.new")}</button>
      </span>
    {/if}
  </span>
  <span class="pop">
    <button type="button" onclick={() => ((tagOpen = !tagOpen), (open = false))} aria-expanded={tagOpen}><Tag size={15} strokeWidth={1.9} />{t("sel.tag")}</button>
    {#if tagOpen}
      <span class="menu tag-menu">
        <span class="title">{t("sel.tag")}</span>
        <span class="tag-row">
          <input
            class="tag-input"
            bind:value={tagDraft}
            placeholder={t("sel.tagPlaceholder")}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submitTag();
              } else if (e.key === "Escape") {
                tagOpen = false;
              }
            }}
          />
          <button type="button" class="op apply" onclick={submitTag}>{t("sel.tagAdd")}</button>
        </span>
      </span>
    {/if}
  </span>
  <button type="button" onclick={onvault}><Archive size={15} strokeWidth={1.9} />{t("sel.vault")}</button>
  {#if canInstallToWindows}
    <button type="button" onclick={oninstall}><Download size={15} strokeWidth={1.9} />{t("sel.installWin")}</button>
  {/if}
  <button type="button" onclick={oncompare} disabled={!comparable} title={compareTitle} style:opacity={comparable ? 1 : 0.4}>
    <Columns2 size={15} strokeWidth={1.9} />{t("compare")}
  </button>
  <span class="sep"></span>
  <button type="button" class="clear" title={t("clearSel")} aria-label={t("clearSel")} onclick={onclear}><X size={12} strokeWidth={2.2} /></button>
</div>

<style>
  .bar {
    position: absolute;
    left: 50%;
    bottom: 20px;
    translate: -50% 0;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 5px;
    border-radius: 14px;
    background: var(--win);
    color: var(--ink);
    box-shadow: var(--float-shadow);
    animation: cf-up 0.22s cubic-bezier(0.2, 0.9, 0.3, 1);
    white-space: nowrap;
  }
  .count { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 12px 0 8px; border-radius: 9px; background: var(--chip); font: 600 12.5px var(--sans); }
  .n { min-width: 20px; height: 20px; padding: 0 5px; border-radius: 999px; background: var(--accent); color: #fff; font: 700 11.5px var(--sans); display: flex; align-items: center; justify-content: center; }
  .sep { width: 1px; height: 20px; background: var(--line2); margin: 0 4px; flex: none; }
  button {
    height: 32px;
    padding: 0 11px 0 9px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--ink);
    font: 600 12.5px var(--sans);
    display: flex;
    align-items: center;
    gap: 7px;
    transition: background-color 0.12s;
  }
  button:hover:not(:disabled) { background: var(--hov); }
  button:disabled { cursor: default; }
  .primary, .primary:hover:not(:disabled) { background: var(--accent); color: #fff; padding: 0 13px 0 10px; }
  .clear { width: 32px; padding: 0; justify-content: center; color: var(--ink3); }
  .clear:hover { color: var(--ink); }
  .pop { position: relative; display: flex; }
  .menu {
    position: absolute;
    bottom: 42px;
    left: 0;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    padding: 5px;
    border-radius: 12px;
    background: var(--win);
    box-shadow: var(--float-shadow);
    animation: cf-up 0.15s ease-out;
  }
  .title { padding: 6px 10px 4px; font: 600 10.5px var(--sans); letter-spacing: 0.08em; color: var(--ink3); }
  .op { height: 30px; padding: 0 10px; border-radius: 7px; font-weight: 500; text-align: left; }
  .new { color: var(--ink2); }
  .tag-menu { min-width: 230px; }
  .tag-row { display: flex; align-items: center; gap: 6px; padding: 4px 6px 6px; }
  .tag-input {
    flex: 1; min-width: 0; height: 28px; padding: 0 8px; border: 1px solid var(--line2); border-radius: 7px;
    background: var(--field); color: var(--ink); font: 500 12px var(--sans);
  }
  .op.apply { height: 28px; background: var(--accent); color: #fff; padding: 0 10px; }
</style>
