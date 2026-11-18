<script>
  import { X } from "@lucide/svelte";
  import { toasts, dismissToast } from "../lib/toasts.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  const COLOR = { ok: "var(--ok)", warn: "var(--warn)", err: "var(--danger)" };
</script>

<div class="stack" aria-live="polite">
  {#each toasts as toast (toast.id)}
    <div class="toast">
      <span class="dot" style:background={COLOR[toast.kind] ?? "var(--accent)"}></span>
      <span class="txt"><b>{toast.title}</b> {#if toast.text}<span>{toast.text}</span>{/if}</span>
      <button type="button" aria-label={t("close")} onclick={() => dismissToast(toast.id)}>
        <X size={11} strokeWidth={2} />
      </button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: absolute;
    right: 18px;
    bottom: 18px;
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-end;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    max-width: 350px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 10px 10px 12px;
    border-radius: 10px;
    background: var(--win);
    box-shadow: var(--float-shadow);
    animation: cf-up 0.2s ease-out;
  }
  .dot { flex: none; width: 8px; height: 8px; border-radius: 50%; }
  .txt { flex: 1; font-size: 12.5px; line-height: 1.4; }
  .txt b { font-weight: 600; }
  .txt span { color: var(--ink2); }
  button {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--ink3);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  button:hover { color: var(--ink); }
</style>
