<script>
  import { SearchX } from "@lucide/svelte";

  let { icon = SearchX, title, text = "", actionLabel = "", onaction = null, spinning = false } = $props();
  const Icon = $derived(icon);
</script>

<div class="empty">
  <span class="icon" class:spinning><Icon size={36} strokeWidth={1.5} /></span>
  <div class="txt">
    <span class="title">{title}</span>
    {#if text}<span class="text">{text}</span>{/if}
  </div>
  {#if actionLabel && onaction}
    <button type="button" onclick={onaction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .empty { flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px; text-align: center; padding: 0 24px 60px; animation: cf-up 0.3s ease-out; }
  .icon { color: var(--ink3); display: flex; }
  .spinning { animation: cf-spin 1.1s linear infinite; }
  @keyframes cf-spin { to { rotate: 360deg; } }
  @media (prefers-reduced-motion: reduce) { .spinning { animation: none; } }
  .txt { display: flex; flex-direction: column; gap: 6px; max-width: 380px; }
  .title { font-size: 17px; font-weight: 700; letter-spacing: -0.01em; }
  .text { font-size: 13.5px; line-height: 1.5; color: var(--ink2); text-wrap: pretty; }
  button { height: 30px; padding: 0 14px; border-radius: 999px; border: 0; background: var(--accent); color: #fff; font: 600 12.5px var(--sans); }
  button:hover { filter: brightness(1.06); }
</style>
