<script>
  import { Check, ChevronRight } from "@lucide/svelte";
  import HeartIcon from "./HeartIcon.svelte";
  import { t } from "../lib/i18n.svelte.js";

  let { g, sample, size, installed, pct, fav, onfav, oninstall, oninspect = null, notInstalled = false } = $props();
  const srcKey = $derived(g.source ?? "google");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="row" ondblclick={() => oninspect?.(g)}>
  <div class="txt" onclick={() => oninspect?.(g)}>
    <span class="name">
      <span>{g.name} · {g.styles} {g.styles === 1 ? t("style") : t("styles")}</span>
      <span class="pill src" class:fontshare={srcKey === "fontshare"} class:fontsource={srcKey === "fontsource"} class:adobe={srcKey === "adobe_dev"} class:squirrel={srcKey === "fontsquirrel"}>{t("src." + srcKey)}</span>
      {#if g.is_variable}<span class="pill vf">VF</span>{/if}
      {#if notInstalled}<span class="missing"> · {t("g.notInstalled")}</span>{/if}
    </span>
    <span class="sample" style:font-family={`"${g.name}", var(--sans)`} style:font-size="{size}px">{sample}</span>
  </div>
  <button
    type="button"
    class="ico heart"
    class:fav
    title={fav ? t("fav.remove") : t("fav.wishlist")}
    aria-label={fav ? t("fav.remove") : t("fav.wishlist")}
    onclick={onfav}
  >
    <HeartIcon filled={fav} />
  </button>
  <div class="action">
    {#if pct != null}
      <span class="pct num">{Math.round(pct)} %</span>
    {:else if installed}
      <span class="done"><Check size={12} strokeWidth={2} />{t("g.installed")}</span>
    {:else}
      <button type="button" class="install" onclick={oninstall}>{t("g.install")}</button>
    {/if}
  </div>
  {#if oninspect}
    <button type="button" class="ico inspect" title={t("specimen")} aria-label={t("specimen")} onclick={() => oninspect(g)}>
      <ChevronRight size={17} strokeWidth={2} />
    </button>
  {/if}
  {#if pct != null}
    <span class="bar"><span style:width="{pct}%"></span></span>
  {/if}
</div>

<style>
  .row {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 34px 106px 34px;
    align-items: center;
    gap: 0 10px;
    padding: 8px 18px 10px 60px;
    transition: background-color 0.12s;
  }
  .row:hover { background: var(--hov); }
  .txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; cursor: pointer; }
  .name { display: flex; align-items: center; gap: 5px; font-size: 10.5px; color: var(--ink3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pill.vf { font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); flex: none; }
  .pill.src { font: 600 9px var(--mono); padding: 1px 5px; border-radius: 4px; background: var(--chip); color: var(--ink2); flex: none; }
  .pill.src.fontshare { background: var(--cyan-soft); color: var(--accent-2); }
  .pill.src.fontsource { background: rgba(45, 212, 191, 0.14); color: var(--ok); }
  .pill.src.adobe { background: rgba(251, 113, 133, 0.16); color: var(--danger); }
  .pill.src.squirrel { background: rgba(245, 158, 11, 0.16); color: var(--warn); }
  .sample { line-height: 1.38; padding-top: 0.2em; white-space: nowrap; overflow-x: clip; overflow-y: visible; text-overflow: ellipsis; }
  .missing { color: var(--ink2); font-weight: 600; }
  .action { justify-self: end; }
  .ico {
    width: 32px; height: 32px; border: 0; border-radius: 8px; background: transparent;
    display: flex; align-items: center; justify-content: center;
    color: var(--ink3); opacity: 0.6; transition: background-color 0.15s, opacity 0.15s, color 0.15s;
  }
  .ico:hover { opacity: 1; background: var(--hov); color: var(--ink); }
  .heart.fav { color: var(--accent); opacity: 1; }
  .install {
    height: 26px;
    padding: 0 12px;
    border-radius: 999px;
    border: 0;
    background: var(--chip);
    color: var(--ink);
    font: 600 12px var(--sans);
    transition: background-color 0.15s, color 0.15s;
  }
  .install:hover { background: var(--accent); color: #fff; }
  .pct { font-size: 12px; font-weight: 600; color: var(--ink2); }
  .done { display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 600; color: var(--ok); }
  .bar { position: absolute; left: 60px; right: 24px; bottom: 0; height: 2px; background: var(--line); }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 0.3s linear; }
</style>
