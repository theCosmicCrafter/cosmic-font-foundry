<script>
  import { Rss, Plus, RefreshCw, Trash2, Eye, Check, X, Sparkles, Globe } from "@lucide/svelte";
  import {
    discover,
    COMMUNITY_PRESETS,
    loadCustomSources,
    addCustomSource,
    removeCustomSource,
    toggleCustomSource,
    syncCustomSource,
  } from "../lib/discover.svelte.js";
  import { notify } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let { onclose, onSelectSource = null } = $props();

  let nameInput = $state("");
  let urlInput = $state("");
  let kindInput = $state("auto");
  let adding = $state(false);
  let syncingId = $state(null);
  let formError = $state("");

  loadCustomSources();

  const trackedUrls = $derived(new Set((discover.customSources ?? []).map((s) => s.url.toLowerCase())));

  async function submitCustomForm(e) {
    e.preventDefault();
    if (adding) return;
    formError = "";
    adding = true;
    try {
      await addCustomSource({ name: nameInput, url: urlInput, kind: kindInput });
      notify("ok", t("cs.addedTitle"), nameInput.trim());
      nameInput = "";
      urlInput = "";
      kindInput = "auto";
    } catch (err) {
      formError = String(err?.message || err);
    } finally {
      adding = false;
    }
  }

  async function addPreset(preset) {
    if (adding) return;
    formError = "";
    adding = true;
    try {
      await addCustomSource({ name: preset.name, url: preset.url, kind: preset.kind });
      notify("ok", t("cs.addedTitle"), preset.name);
    } catch (err) {
      formError = String(err?.message || err);
    } finally {
      adding = false;
    }
  }

  async function onSync(id) {
    syncingId = id;
    try {
      await syncCustomSource(id);
      notify("ok", t("cs.synced"), "");
    } catch (err) {
      notify("err", t("err.one"), String(err?.message || err));
    } finally {
      syncingId = null;
    }
  }

  async function onToggle(src, enabled) {
    await toggleCustomSource(src.id, enabled);
  }

  async function onRemove(id) {
    await removeCustomSource(id);
    notify("ok", t("cs.removed"), "");
  }

  function browseSource(src) {
    if (!src.enabled) {
      toggleCustomSource(src.id, true);
    }
    onSelectSource?.(`custom:${src.id}`);
    onclose?.();
  }

  function onKeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose?.();
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={t("cs.title")}
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="head">
      <div class="head-txt">
        <span class="head-ico"><Rss size={16} strokeWidth={2} /></span>
        <div>
          <h2>{t("cs.title")}</h2>
          <p class="sub">{t("cs.subtitle")}</p>
        </div>
      </div>
      <button type="button" class="close" title={t("close")} aria-label={t("close")} onclick={onclose}>
        <X size={14} strokeWidth={2} />
      </button>
    </div>

    <div class="scroll">
      <!-- Add Custom Library Form -->
      <section class="card">
        <span class="sec-kicker">{t("cs.addSection")}</span>
        <form class="add-form" onsubmit={submitCustomForm}>
          <div class="row-inputs">
            <label class="field-col">
              <span>{t("cs.nameLabel")}</span>
              <input
                type="text"
                bind:value={nameInput}
                placeholder={t("cs.namePlaceholder")}
                required
              />
            </label>
            <label class="field-col kind-col">
              <span>{t("cs.formatLabel")}</span>
              <select bind:value={kindInput}>
                <option value="auto">{t("cs.fmtAuto")}</option>
                <option value="rss">{t("cs.fmtRss")}</option>
                <option value="json">{t("cs.fmtJson")}</option>
                <option value="github">{t("cs.fmtGithub")}</option>
              </select>
            </label>
          </div>
          <label class="field-col">
            <span>{t("cs.urlLabel")}</span>
            <div class="url-row">
              <input
                type="url"
                bind:value={urlInput}
                placeholder={t("cs.urlPlaceholder")}
                required
              />
              <button type="submit" class="btn-accent" disabled={adding || !nameInput.trim() || !urlInput.trim()}>
                <Plus size={13} strokeWidth={2.2} />
                {adding ? t("cs.adding") : t("cs.addBtn")}
              </button>
            </div>
          </label>
          {#if formError}
            <p class="err-banner">{formError}</p>
          {/if}
        </form>
      </section>

      <!-- One-Click Community Presets -->
      <section class="card">
        <div class="sec-row">
          <Sparkles size={13} class="ico-accent" />
          <span class="sec-kicker">{t("cs.presetsTitle")}</span>
        </div>
        <div class="preset-grid">
          {#each COMMUNITY_PRESETS as preset (preset.key)}
            {@const added = trackedUrls.has(preset.url.toLowerCase())}
            <div class="preset-card" class:added>
              <div class="p-info">
                <strong>{preset.name}</strong>
                <span>{t(preset.descKey)}</span>
              </div>
              <button
                type="button"
                class="btn-sm"
                class:done={added}
                disabled={added || adding}
                onclick={() => addPreset(preset)}
              >
                {#if added}
                  <Check size={12} /> {t("cs.tracked")}
                {:else}
                  <Plus size={12} /> {t("cs.quickAdd")}
                {/if}
              </button>
            </div>
          {/each}
        </div>
      </section>

      <!-- Tracked Sources -->
      <section class="card">
        <div class="sec-row">
          <Globe size={13} class="ico-cyan" />
          <span class="sec-kicker">{t("cs.trackedTitle")} ({discover.customSources.length})</span>
        </div>
        {#if !discover.customSources.length}
          <p class="empty">{t("cs.empty")}</p>
        {:else}
          <div class="src-list">
            {#each discover.customSources as src (src.id)}
              <div class="src-row" class:disabled={!src.enabled}>
                <div class="src-main">
                  <div class="src-title-line">
                    <strong>{src.name}</strong>
                    <span class="kind-pill">{src.kind.toUpperCase()}</span>
                    <span class="count-pill num">{src.font_count} {t("cs.fontsUnit")}</span>
                  </div>
                  <div class="src-url" title={src.url}>{src.url}</div>
                  {#if src.last_error}
                    <div class="src-err">{src.last_error}</div>
                  {/if}
                </div>

                <div class="src-actions">
                  <label class="switch-chk" title={t("cs.enableHint")}>
                    <input
                      type="checkbox"
                      checked={src.enabled}
                      onchange={(e) => onToggle(src, e.currentTarget.checked)}
                    />
                    <span>{src.enabled ? t("cs.on") : t("cs.off")}</span>
                  </label>

                  <button type="button" class="btn-sm" onclick={() => browseSource(src)}>
                    <Eye size={12} /> {t("cs.browse")}
                  </button>

                  <button
                    type="button"
                    class="ico-btn"
                    title={t("cs.syncNow")}
                    disabled={syncingId === src.id}
                    onclick={() => onSync(src.id)}
                  >
                    <RefreshCw size={13} class={syncingId === src.id ? "spin" : ""} />
                  </button>

                  <button
                    type="button"
                    class="ico-btn danger"
                    title={t("cs.remove")}
                    onclick={() => onRemove(src.id)}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    </div>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 25;
    background: var(--velo);
    backdrop-filter: blur(5px);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: cf-fade 0.16s ease-out;
  }

  .modal {
    width: min(740px, 92vw);
    max-height: 86vh;
    border-radius: 14px;
    background: var(--win);
    color: var(--ink);
    border: 1px solid var(--line2);
    box-shadow: var(--float-shadow);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--side);
  }

  .head-txt {
    display: flex;
    align-items: center;
    gap: 11px;
  }

  .head-ico {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    flex: none;
  }

  h2 {
    margin: 0;
    font-size: 15.5px;
    font-weight: 700;
  }

  .sub {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--ink2);
  }

  .close {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink3);
    display: grid;
    place-items: center;
  }

  .close:hover {
    background: var(--hov);
    color: var(--ink);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 22px 22px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .card {
    background: var(--side);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sec-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .sec-row :global(.ico-accent) {
    color: var(--accent);
  }

  .sec-row :global(.ico-cyan) {
    color: var(--accent-2);
  }

  .sec-kicker {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink3);
  }

  .add-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row-inputs {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 170px;
    gap: 10px;
  }

  .field-col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11.5px;
    color: var(--ink2);
  }

  .field-col input,
  .field-col select {
    height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--win);
    color: var(--ink);
    font-size: 12.5px;
  }

  .field-col input:focus,
  .field-col select:focus {
    outline: none;
    border-color: var(--accent);
  }

  .url-row {
    display: flex;
    gap: 8px;
  }

  .url-row input {
    flex: 1;
    min-width: 0;
    font-family: var(--mono);
    font-size: 12px;
  }

  .btn-accent {
    height: 32px;
    padding: 0 14px;
    border-radius: 8px;
    border: 0;
    background: var(--accent);
    color: #fff;
    font: 600 12px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    white-space: nowrap;
  }

  .btn-accent:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .err-banner {
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: rgba(251, 113, 133, 0.14);
    border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
    color: var(--danger);
    font-size: 12px;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 9px;
  }

  @media (max-width: 680px) {
    .preset-grid,
    .row-inputs {
      grid-template-columns: 1fr;
    }
  }

  .preset-card {
    padding: 10px;
    border-radius: 9px;
    background: var(--win);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
  }

  .preset-card.added {
    border-color: color-mix(in srgb, var(--ok) 35%, var(--line));
  }

  .p-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .p-info strong {
    font-size: 12px;
  }

  .p-info span {
    font-size: 11px;
    color: var(--ink2);
    line-height: 1.35;
  }

  .btn-sm {
    height: 26px;
    padding: 0 10px;
    border-radius: 7px;
    border: 1px solid var(--line2);
    background: var(--field);
    color: var(--ink);
    font: 600 11.5px var(--sans);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    white-space: nowrap;
  }

  .btn-sm:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .btn-sm.done {
    color: var(--ok);
    border-color: color-mix(in srgb, var(--ok) 35%, transparent);
    background: color-mix(in srgb, var(--ok) 10%, transparent);
    cursor: default;
  }

  .empty {
    margin: 2px 0;
    font-size: 12px;
    color: var(--ink3);
  }

  .src-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .src-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 9px;
    background: var(--win);
    border: 1px solid var(--line);
  }

  .src-row.disabled {
    opacity: 0.6;
  }

  .src-main {
    min-width: 0;
    flex: 1 1 260px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .src-title-line {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .src-title-line strong {
    font-size: 12.5px;
  }

  .kind-pill {
    font: 700 9.5px var(--mono);
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .count-pill {
    font-size: 11px;
    color: var(--accent-2);
    background: var(--cyan-soft);
    padding: 1px 6px;
    border-radius: 4px;
  }

  .src-url {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .src-err {
    font-size: 11px;
    color: var(--danger);
  }

  .src-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .switch-chk {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--side);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--ink2);
    cursor: pointer;
  }

  .switch-chk input {
    accent-color: var(--accent);
    margin: 0;
  }

  .ico-btn {
    width: 26px;
    height: 26px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--side);
    color: var(--ink2);
    display: grid;
    place-items: center;
  }

  .ico-btn:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--line2);
  }

  .ico-btn.danger:hover {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 40%, var(--line));
  }

  :global(.spin) {
    animation: cf-spin 0.9s linear infinite;
  }
</style>
