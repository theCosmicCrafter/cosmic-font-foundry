<script>
  let {
    options,
    value,
    onselect,
    width = null,
    height = 26,
    paddingX = 14,
    font = "600 12px",
    label = "",
  } = $props();

  const index = $derived(Math.max(0, options.findIndex((o) => o.value === value)));
  const count = $derived(options.length);
</script>

<div
  class="seg"
  role="radiogroup"
  aria-label={label}
  style:grid-template-columns={width ? `repeat(${count}, ${width}px)` : `repeat(${count}, minmax(0, 1fr))`}
>
  <span
    class="pill"
    aria-hidden="true"
    style:left="calc(3px + (100% - 6px) * {index} / {count})"
    style:width="calc((100% - 6px) / {count})"
  ></span>
  {#each options as o (o.value)}
    {@const selected = o.value === value}
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={o.title ?? o.label}
      title={o.title}
      class:selected
      style:height="{height}px"
      style:padding={o.icon ? "0" : `0 ${paddingX}px`}
      style:font="{font} var(--sans)"
      onclick={() => !selected && onselect(o.value)}
    >
      {#if o.icon}
        <o.icon size={13} strokeWidth={2} />
      {:else}
        {o.label}
      {/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    position: relative;
    flex: none;
    display: grid;
    padding: 3px;
    border-radius: 999px;
    background: var(--chip);
  }
  .pill {
    position: absolute;
    top: 3px;
    bottom: 3px;
    border-radius: 999px;
    background: var(--win);
    box-shadow: var(--pill-shadow);
    transition: var(--slide), background-color 0.3s;
    pointer-events: none;
  }
  button {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    white-space: nowrap;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--ink2);
    transition: color 0.15s;
  }
  button:hover,
  button.selected { color: var(--ink); }
  button.selected { cursor: default; }
</style>
