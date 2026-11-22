<script>
  import { Ghost, X } from "@lucide/svelte";

  let {
    fonts = [],
    selectedFamily = "",
    onSelectGhost = null,
  } = $props();

  const availableFonts = $derived(
    (fonts ?? []).filter((f) => Boolean(f?.file) && f.file !== f.name),
  );

  function handleChange(e) {
    const val = e.currentTarget.value;
    if (!val) {
      onSelectGhost?.(null);
      return;
    }
    const entry = availableFonts.find((f) => f.name === val) ?? null;
    onSelectGhost?.(entry);
  }

  function clearGhost() {
    onSelectGhost?.(null);
  }
</script>

<div class="ghost-picker" class:active={Boolean(selectedFamily)}>
  <span class="ghost-lbl">
    <Ghost size={12} />
    <span>Ghost</span>
  </span>
  <select
    class="ghost-select"
    value={selectedFamily}
    aria-label="Ghost underlay font"
    onchange={handleChange}
  >
    <option value="">None</option>
    {#each availableFonts as f (`${f.file ?? ""}::${f.name}`)}
      <option value={f.name}>{f.name}</option>
    {/each}
  </select>
  {#if selectedFamily}
    <button
      type="button"
      class="ghost-clear"
      title="Clear ghost underlay"
      aria-label="Clear ghost underlay"
      onclick={clearGhost}
    >
      <X size={11} />
    </button>
  {/if}
</div>

<style>
  .ghost-picker {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 7px;
    border-radius: 7px;
    background: var(--field);
    border: 1px solid var(--line2);
    color: var(--ink2);
    font: 600 11px var(--sans);
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }
  .ghost-picker.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--ink);
  }
  .ghost-lbl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: 600 10.5px var(--sans);
    color: inherit;
    white-space: nowrap;
  }
  .ghost-select {
    max-width: 132px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: 600 11px var(--sans);
    cursor: pointer;
    padding-right: 2px;
  }
  .ghost-select option {
    background: var(--field);
    color: var(--ink);
  }
  .ghost-clear {
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ink2);
    display: grid;
    place-items: center;
  }
  .ghost-clear:hover {
    background: var(--hov);
    color: var(--ink);
  }
</style>
