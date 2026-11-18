<script>
  import { ArrowDownToLine } from "@lucide/svelte";
  import { updater, installAppUpdate } from "../lib/updater.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  const pct = $derived(updater.total > 0 ? Math.min(100, Math.round((updater.downloaded / updater.total) * 100)) : 0);
  const busy = $derived(updater.status === "downloading" || updater.status === "installing");
  const visible = $derived(!updater.dismissed && (updater.status === "available" || busy || (updater.status === "error" && updater.version)));
  const badSignature = $derived(/signature/i.test(updater.error));
</script>

{#if visible}
  <div class="upd" role="status">
    <span class="ico"><ArrowDownToLine size={14} strokeWidth={2} /></span>
    <div class="txt">
      <b>{t("upd.available", { v: updater.version })}</b>
      <span>
        {#if updater.status === "installing"}{t("upd.installing")}
        {:else if updater.status === "downloading"}{t("upd.downloading", { pct })}
        {:else if updater.status === "error"}{badSignature ? t("upd.badSig") : t("upd.failed")}
        {:else if updater.portable}{t("upd.portable")}
        {:else}{t("upd.hint")}{/if}
      </span>
    </div>
    {#if !busy}
      <div class="actions">
        <button type="button" class="later" onclick={() => (updater.dismissed = true)}>{t("upd.later")}</button>
        <button type="button" class="go" onclick={installAppUpdate}>
          {updater.portable ? t("upd.download") : updater.status === "error" ? t("upd.retry") : t("upd.update")}
        </button>
      </div>
    {:else}
      <span class="bar"><span style:width="{updater.total ? pct : 100}%" class:indeterminate={!updater.total}></span></span>
    {/if}
  </div>
{/if}

<style>
  .upd {
    position: relative;
    overflow: hidden;
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    gap: 8px;
    padding: 10px;
    border-radius: 10px;
    background: var(--win);
    box-shadow: 0 0 0 1px var(--line2);
    animation: cf-up 0.25s ease-out;
  }
  .ico { color: var(--accent); display: flex; padding-top: 1px; }
  .txt { display: flex; flex-direction: column; gap: 2px; font-size: 12px; line-height: 1.35; }
  .txt b { font-weight: 600; }
  .txt span { color: var(--ink2); }
  .actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: 4px; }
  button { height: 24px; padding: 0 10px; border-radius: 999px; border: 0; font: 600 11.5px var(--sans); transition: background-color 0.15s, color 0.15s; }
  .later { background: transparent; color: var(--ink2); }
  .later:hover { background: var(--hov); color: var(--ink); }
  .go { background: var(--accent); color: #fff; }
  .bar { grid-column: 1 / -1; height: 2px; background: var(--line); border-radius: 2px; overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 0.25s linear; }
  .bar span.indeterminate { animation: cf-pulse 1.2s ease-in-out infinite; }
  @keyframes cf-pulse { 50% { opacity: 0.35; } }
</style>
