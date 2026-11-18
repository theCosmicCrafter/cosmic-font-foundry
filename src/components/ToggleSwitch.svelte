<script>
  let { active, locked, title, onclick, size = 18, marginBottom = "0px" } = $props();
</script>

{#if locked}
  <span class="circle locked" {title} style:width="{size}px" style:height="{size}px" style:margin-bottom={marginBottom}>
    <svg width={size * 0.5} height={size * 0.56} viewBox="0 0 8 9" aria-hidden="true">
      <rect x="1" y="4" width="6" height="4.5" rx="1" fill="var(--win)" />
      <path d="M2.4 4V2.8a1.6 1.6 0 0 1 3.2 0V4" fill="none" stroke="var(--win)" stroke-width="1.1" />
    </svg>
  </span>
{:else}
  <button
    type="button"
    class="circle"
    class:active
    {title}
    aria-label={title}
    aria-pressed={active}
    style:width="{size}px"
    style:height="{size}px"
    style:margin-bottom={marginBottom}
    onclick={(e) => {
      e.stopPropagation();
      onclick();
    }}
    ondblclick={(e) => e.stopPropagation()}
  >
    {#if active}
      <svg width={size * 0.56} height={size * 0.56} viewBox="0 0 12 12" fill="none" stroke="var(--win)" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M2.3 6.4 4.9 8.9 9.7 3.4" />
      </svg>
    {/if}
  </button>
{/if}

<style>
  .circle {
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--line2);
    transition: background-color 0.15s, box-shadow 0.15s;
  }
  button.circle:hover { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .circle.active { background: var(--accent); box-shadow: none; }
  .circle.locked { background: var(--line2); box-shadow: none; }
</style>
