<script>
  import { ChevronRight } from "@lucide/svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";
  import HeartIcon from "./HeartIcon.svelte";
  import { t } from "../lib/i18n.svelte.js";

  let { f, style, sample, size, selected, fav, locked, reason, onselect, onopen, ontoggle, onfav, ondrag } = $props();

  const on = $derived(f.active || locked);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="row" class:selected onclick={onselect} ondblclick={onopen} onpointerdown={ondrag} data-family={f.name}>
  <ToggleSwitch active={f.active} {locked} title={locked ? reason : f.active ? t("deactivate") : t("activate")} onclick={ontoggle} />
  <div class="txt">
    <span class="name">
      <span>{f.name}</span>
      {#if f.is_variable}<span class="pill vf">VF</span>{/if}
      {#if f.has_conflicts}<span class="pill warn">!</span>{/if}
      {#if f.error}<span class="broken"> · {t("err.unreadable")}</span>{/if}
    </span>
    <span
      class="sample"
      class:off={!on}
      style:font-family={style.family}
      style:font-weight={style.weight}
      style:font-style={style.italic ? "italic" : "normal"}
      style:font-size="{size}px">{sample}</span
    >
  </div>
  <button
    type="button"
    class="ico heart"
    class:fav
    title={fav ? t("fav.remove") : t("fav.add")}
    aria-label={fav ? t("fav.remove") : t("fav.add")}
    onclick={(e) => (e.stopPropagation(), onfav())}
    ondblclick={(e) => e.stopPropagation()}
  >
    <HeartIcon filled={fav} />
  </button>
  <button
    type="button"
    class="ico open"
    title={t("specimen")}
    aria-label={t("specimen")}
    onclick={(e) => (e.stopPropagation(), onopen())}
    ondblclick={(e) => e.stopPropagation()}
  >
    <ChevronRight size={17} strokeWidth={2} />
  </button>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) 34px 34px;
    align-items: center;
    gap: 0 12px;
    padding: 8px 18px 8px 24px;
    transition: background-color 0.12s;
  }
  .row:hover { background: var(--hov); }
  .row.selected, .row.selected:hover { background: var(--sel); }
  .txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .name { display: flex; align-items: center; gap: 5px; font-size: 10.5px; color: var(--ink3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pill { font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; letter-spacing: 0.04em; flex: none; }
  .pill.vf { background: var(--accent-soft); color: var(--accent); }
  .pill.warn { background: rgba(196, 125, 23, 0.16); color: var(--warn); }
  .broken { color: var(--danger); font-weight: 600; }
  .sample {
    line-height: 1.38;
    padding-top: 0.2em;
    white-space: nowrap;
    overflow-x: clip;
    overflow-y: visible;
    text-overflow: ellipsis;
    color: var(--ink);
    transition: color 0.25s;
  }
  .sample.off { color: var(--off); text-shadow: 0 1px 0 rgba(255, 255, 255, 0.9); }
  :global([data-theme="dark"]) .sample.off { text-shadow: 0 1px 0 rgba(0, 0, 0, 0.6); }
  .ico {
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ink3);
    opacity: 0.6;
    transition: background-color 0.15s, opacity 0.15s, color 0.15s;
  }
  .ico:hover { opacity: 1; background: var(--hov); }
  .open { opacity: 0.7; }
  .open:hover { color: var(--ink); }
  .heart.fav { color: var(--accent); opacity: 1; }
</style>
