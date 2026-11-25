<script>
  import HeartIcon from "./HeartIcon.svelte";
  import { t } from "../lib/i18n.svelte.js";
  import { fitText } from "../lib/fitText.js";

  let { g, pct, fav, onfav, oninstall, onopen = null, oninspect = null } = $props();
  const family = $derived(`"${g.name}", var(--sans)`);
  const aa = $derived(g.category === "other" ? "あ" : "Aa");
  const srcKey = $derived(g.source ?? "google");
  const isOriginLink = $derived(g.download_mode === "origin_link" && Boolean(g.homepage));
  const metaTip = $derived(
    [g.license, g.description, g.tags?.slice(0, 5).join(", ")].filter(Boolean).join(" · ") || g.name,
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" title={metaTip} ondblclick={() => oninspect?.(g)}>
  <span
    class="pill src"
    class:fontshare={srcKey === "fontshare"}
    class:fontsource={srcKey === "fontsource"}
    class:adobe={srcKey === "adobe_dev"}
    class:squirrel={srcKey === "fontsquirrel"}
    class:homebrew={srcKey === "homebrew"}
    class:scoop={srcKey === "scoop"}
    class:foundries={srcKey === "open_foundries"}
    class:fontesk={srcKey === "fontesk"}
    class:nerdfonts={srcKey === "nerdfonts"}
    class:custom={srcKey.startsWith("custom:")}
  >
    {srcKey.startsWith("custom:") ? (g.source_label || t("src.custom")) : (t("src." + srcKey) || g.source_label || srcKey)}
  </span>
  {#if g.is_variable}<span class="pill vf">VF</span>{/if}
  <div class="aa" style:font-family={family} onclick={() => oninspect?.(g)}>
    <span use:fitText={{ text: aa, family, base: 64, margin: 12 }}>{aa}</span>
  </div>
  <div class="foot">
    {#if pct != null}
      <span class="pct num">{Math.round(pct)} %</span>
    {:else if isOriginLink}
      <button type="button" class="install origin" onclick={() => onopen?.(g)} title={t("g.openOriginTip")}>{t("g.openOrigin")}</button>
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
    padding: 13px;
    border-radius: 12px;
    background: var(--field);
    border: 1px dashed var(--line2);
    box-shadow: var(--card-shadow);
    transition: transform 0.16s ease, background-color 0.15s, border-color 0.16s ease, box-shadow 0.16s ease;
  }
  .card:hover {
    background: var(--elevated);
    border-color: color-mix(in srgb, var(--accent) 42%, var(--line2));
    transform: translateY(-2px);
    box-shadow: var(--glow);
  }
  .pill.vf { position: absolute; top: 8px; right: 8px; font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); }
  .pill.src { position: absolute; top: 8px; left: 8px; font: 600 9px var(--mono); padding: 1px 5px; border-radius: 4px; background: var(--chip); color: var(--ink2); }
  .pill.src.fontshare { background: var(--cyan-soft); color: var(--accent-2); }
  .pill.src.fontsource { background: rgba(45, 212, 191, 0.14); color: var(--ok); }
  .pill.src.adobe { background: rgba(251, 113, 133, 0.16); color: var(--danger); }
  .pill.src.squirrel { background: rgba(245, 158, 11, 0.16); color: var(--warn); }
  .pill.src.homebrew { background: rgba(234, 88, 12, 0.16); color: #ea580c; }
  .pill.src.scoop { background: rgba(56, 189, 248, 0.16); color: #0284c7; }
  .pill.src.foundries { background: rgba(168, 85, 247, 0.15); color: #a855f7; }
  .pill.src.fontesk { background: rgba(236, 72, 153, 0.15); color: #db2777; }
  .pill.src.nerdfonts { background: rgba(132, 204, 22, 0.15); color: #65a30d; }
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
