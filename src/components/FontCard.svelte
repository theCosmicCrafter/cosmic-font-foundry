<script>
  import ToggleSwitch from "./ToggleSwitch.svelte";
  import HeartIcon from "./HeartIcon.svelte";
  import { t } from "../lib/i18n.svelte.js";
  import { fitText } from "../lib/fitText.js";

  let { f, style, selected, fav, locked, reason, onselect, onopen, ontoggle, onfav, ondrag } = $props();
  const on = $derived(f.active || locked);
  const aa = $derived(f.category === "other" ? "あ" : "Aa");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" class:selected onclick={onselect} ondblclick={onopen} onpointerdown={ondrag} data-family={f.name}>
  <div class="badges">
    {#if f.is_variable}<span class="pill vf">VF</span>{/if}
    {#if f.has_conflicts}<span class="pill warn">!</span>{/if}
  </div>
  <div
    class="aa"
    class:off={!on}
    style:font-family={style.family}
    style:font-weight={style.weight}
    style:font-style={style.italic ? "italic" : "normal"}
  >
    <span use:fitText={{ text: aa, family: style.family, weight: style.weight, italic: style.italic, base: 64, margin: 12 }}>{aa}</span>
  </div>
  <div class="foot">
    <ToggleSwitch active={f.active} {locked} title={locked ? reason : f.active ? t("deactivate") : t("activate")} onclick={ontoggle} size={16} />
    <span class="name">{f.name}</span>
    <button
      type="button"
      class="heart"
      class:fav
      title={fav ? t("fav.remove") : t("fav.add")}
      aria-label={fav ? t("fav.remove") : t("fav.add")}
      onclick={(e) => (e.stopPropagation(), onfav())}
      ondblclick={(e) => e.stopPropagation()}
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
    box-shadow: inset 0 0 0 1px var(--line);
    transition: background-color 0.12s, box-shadow 0.12s;
  }
  .card:hover { background: var(--hov); }
  .card.selected, .card.selected:hover { background: var(--sel); box-shadow: inset 0 0 0 1.5px var(--accent); }
  .badges { position: absolute; top: 8px; right: 8px; display: flex; gap: 4px; pointer-events: none; }
  .pill { font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; }
  .pill.vf { background: var(--accent-soft); color: var(--accent); }
  .pill.warn { background: rgba(196, 125, 23, 0.16); color: var(--warn); }
  .aa {
    height: 92px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 64px;
    line-height: 1;
    color: var(--ink);
    overflow: hidden;
    white-space: nowrap;
  }
  .aa span { display: block; line-height: 1; }
  .aa.off { color: var(--off); }
  .foot { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; min-width: 0; font-size: 12px; font-weight: 500; color: var(--ink2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
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
