<div class="skel" aria-hidden="true">
  {#each [58, 44, 66, 52, 61, 38, 55, 47] as w, i (i)}
    <div class="row">
      <span class="dot"></span>
      <span class="txt"><span class="name"></span><span class="sample" style:width="{w}%"></span></span>
    </div>
  {/each}
</div>

<style>
  @keyframes cf-shim { 0% { background-position: -320px 0; } 100% { background-position: 320px 0; } }
  .skel { display: flex; flex-direction: column; }
  .row { display: grid; grid-template-columns: 24px minmax(0, 1fr); gap: 0 12px; align-items: end; padding: 10px 24px; }
  .dot { width: 15px; height: 15px; border-radius: 50%; background: var(--hov); margin-bottom: 6px; }
  .txt { display: flex; flex-direction: column; gap: 7px; }
  .name { width: 90px; height: 7px; border-radius: 4px; background: var(--hov); }
  .sample { height: 24px; border-radius: 5px; background: linear-gradient(90deg, var(--hov) 0%, var(--line2) 50%, var(--hov) 100%) 0 0 / 320px 100% repeat-x; animation: cf-shim 1.2s linear infinite; }
</style>
