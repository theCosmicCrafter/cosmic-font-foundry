<script>
  import HeartIcon from "./HeartIcon.svelte";
  import { t } from "../lib/i18n.svelte.js";
  import { fitText } from "../lib/fitText.js";

  let { g, pct, fav, onfav, oninstall, oninspect = null } = $props();
  const family = $derived(`"${g.name}", var(--sans)`);
  const aa = $derived(g.category === "other" ? "あ" : "Aa");
  const srcKey = $derived(g.source ?? "google");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" ondblclick={() => oninspect?.(g)}>
  <span class="pill src" class:fontshare={srcKey === "fontshare"} class:fontsource={srcKey === "fontsource"} class:adobe={srcKey === "adobe_dev"} class:squirrel={srcKey === "fontsquirrel"} class:custom={srcKey.startsWith("custom:")}>{srcKey.startsWith("custom:") ? (g.source_label || t("src.custom")) : t("src." + srcKey)}</span>
  {#if g.is_variable}<span class="pill vf">VF</span>{/if}
  <div class="aa" style:font-family={family} onclick={() => oninspect?.(g)}>
    <span use:fitText={{ text: aa, family, base: 64, margin: 12 }}>{aa}</span>
  </div>
  <div class="foot">
    {#if pct != null}
      <span class="pct num">{Math.round(pct)} %</span>
    {:else}
      <button type="button" class="install" onclick={oninstall} title={t("g.notInstalled")}>{t("g.install")}</button>
    {/if}
    <span class="name">{g.name}</span>
    <button
      type="button"
      class="heart"
      class:fav
      title={fav ? t("fav.remove") : t("fav.wishlist")}
      aria-label={fav ? t("fav.remove") : t("fav.wishlist")}
      onclick={onfav}
    >
      <HeartIcon filled={fav} size={16} />
    </button>
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 8px;
    border: 1px dashed var(--line2);
    transition: background-color 0.12s;
  }
  .card:hover { background-color: var(--hov); }
  .pill.vf { position: absolute; top: 8px; right: 8px; font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); }
  .pill.src { position: absolute; top: 8px; left: 8px; font: 600 9px var(--mono); padding: 1px 5px; border-radius: 4px; background: var(--chip); color: var(--ink2); }
  .pill.src.fontshare { background: var(--cyan-soft); color: var(--accent-2); }
  .pill.src.fontsource { background: rgba(45, 212, 191, 0.14); color: var(--ok); }
  .pill.src.adobe { background: rgba(251, 113, 133, 0.16); color: var(--danger); }
  .pill.src.squirrel { background: rgba(245, 158, 11, 0.16); color: var(--warn); }
  .pill.src.custom { background: var(--accent-soft); color: var(--accent); }
  .aa {
    height: 92px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 64px;
    line-height: 1;
    color: var(--ink2);
    overflow: hidden;
    white-space: nowrap;
    cursor: pointer;
  }
  .aa span { display: block; line-height: 1; }
  .foot { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; min-width: 0; font-size: 12px; font-weight: 500; color: var(--ink2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .install {
    flex: none;
    height: 20px;
    padding: 0 8px;
    border-radius: 999px;
    border: 0;
    background: var(--chip);
    color: var(--ink);
    font: 600 11px var(--sans);
    transition: background-color 0.15s, color 0.15s;
  }
  .install:hover { background: var(--accent); color: #fff; }
  .pct { flex: none; font-size: 11px; font-weight: 600; color: var(--ink2); }
  .heart {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--ink3);
    opacity: 0.6;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .heart:hover { opacity: 1; }
  .heart.fav { color: var(--accent); opacity: 1; }
</style>
