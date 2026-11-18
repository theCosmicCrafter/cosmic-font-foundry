<script>
  import { inTauri } from "../lib/settings.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  let win = null;
  let maximized = $state(false);

  $effect(() => {
    if (!inTauri) return;
    let unlisten;
    import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      win = getCurrentWindow();
      const sync = () => win.isMaximized().then((m) => (maximized = m));
      sync();
      unlisten = await win.onResized(sync);
    });
    return () => unlisten?.();
  });
</script>

<div class="controls">
  <button type="button" title={t("win.minimize")} aria-label={t("win.minimize")} onclick={() => win?.minimize()}>
    <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1.1" /></svg>
  </button>
  <button
    type="button"
    title={maximized ? t("win.restore") : t("win.maximize")}
    aria-label={maximized ? t("win.restore") : t("win.maximize")}
    onclick={() => win?.toggleMaximize()}
  >
    {#if maximized}
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1">
        <rect x="1" y="3" width="6" height="6" rx="1" /><path d="M3 3V2a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H7" />
      </svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.1" /></svg>
    {/if}
  </button>
  <button type="button" class="close" title={t("close")} aria-label={t("close")} onclick={() => win?.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" stroke-width="1.1" /></svg>
  </button>
</div>

<style>
  .controls { display: flex; align-items: center; gap: 2px; margin-right: 10px; }
  button {
    width: 34px;
    height: 28px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink3);
    transition: background-color 0.15s, color 0.15s;
  }
  button:hover { background: var(--chip); color: var(--ink); }
  button.close:hover { background: var(--danger); color: #fff; }
  button:active { background: var(--line2); }
</style>
