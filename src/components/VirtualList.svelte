<script>
  let {
    items,
    height,
    getKey,
    row,
    minWidth = 0,
    gap = 0,
    padX = 0,
    padTop = 0,
    padBottom = 90,
    onVisible = null,
  } = $props();

  let width = $state(0);
  let viewportHeight = $state(0);
  let scrollTop = $state(0);

  const cols = $derived(minWidth ? Math.max(1, Math.floor((width - padX * 2 + gap) / (minWidth + gap))) : 1);
  const step = $derived(height + gap);
  const totalRows = $derived(Math.ceil(items.length / cols));
  const from = $derived(Math.max(0, Math.floor((scrollTop - padTop) / step) - 3));
  const to = $derived(Math.min(totalRows, Math.ceil((scrollTop + viewportHeight - padTop) / step) + 3));
  const visible = $derived(items.slice(from * cols, to * cols));

  $effect(() => {
    onVisible?.(items.slice(Math.max(0, from + 3) * cols, Math.max(0, to - 3) * cols));
  });
</script>

<div class="box" bind:clientWidth={width} bind:clientHeight={viewportHeight} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
  <div class="total" style:height="{padTop + Math.max(0, totalRows * step - gap) + padBottom}px">
    <div
      class="viewport"
      style:top="{padTop + from * step}px"
      style:left="{padX}px"
      style:right="{padX}px"
      style:grid-template-columns="repeat({cols}, minmax(0, 1fr))"
      style:grid-auto-rows="{height}px"
      style:gap="{gap}px"
    >
      {#each visible as item (getKey(item))}
        {@render row(item)}
      {/each}
    </div>
  </div>
</div>

<style>
  .box { flex: 1 1 auto; min-height: 0; overflow-y: auto; overflow-x: hidden; }
  .total { position: relative; }
  .viewport { position: absolute; display: grid; }
</style>
