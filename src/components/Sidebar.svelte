<script>
  import { SlidersHorizontal, Sun, Moon, Plus, Pin } from "@lucide/svelte";
  import SegmentedControl from "./SegmentedControl.svelte";
  import UpdateNotice from "./UpdateNotice.svelte";
  import { currentTheme, setTheme, version } from "../lib/settings.svelte.js";
  import { drag } from "../lib/drag.svelte.js";
  import { t } from "../lib/i18n.svelte.js";

  // section: installed | discover | favorites | diagnostics | col:<id> | dir:<path>
  let {
    section,
    cat,
    script = null,
    counts,
    catCounts,
    scriptCounts = {},
    collections,
    folders,
    projectFolders = new Set(),
    footer,
    renaming = $bindable(),
    onsection,
    oncat,
    onscript = () => {},
    oncreateCollection,
    onrenameCollection,
    onaddFolder,
    onsettings,
    onabout,
  } = $props();

  const CATEGORIES = ["sans", "serif", "script", "display", "mono", "other"];
  const visibleCategories = $derived(CATEGORIES.filter((c) => (catCounts[c] ?? 0) > 0 || cat === c));
  const visibleScripts = $derived(
    Object.entries(scriptCounts)
      .filter(([name, count]) => count > 0 || script === name)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([name]) => name),
  );

  let draft = $state("");
  let renameInput = $state();

  $effect(() => {
    if (renaming && renameInput) {
      draft = collections.find((c) => c.id === renaming)?.name ?? "";
      renameInput.focus();
      renameInput.select();
    }
  });

  function finishRename(save) {
    if (save && renaming) onrenameCollection(renaming, draft);
    renaming = null;
  }

  let creating = $state(false);
  let newName = $state("");
  let newInput = $state();
  $effect(() => {
    if (creating && newInput) newInput.focus();
  });
  function startNew() {
    newName = "";
    creating = true;
  }
  function finishNew(create) {
    const clean = newName.trim();
    creating = false;
    if (create && clean) oncreateCollection(clean);
  }
</script>

{#snippet item(text, active, go, count, extra = {})}
  <button
    type="button"
    class="item"
    class:active
    class:target={extra.target}
    onclick={go}
    ondblclick={extra.dbl}
    title={extra.title}
    data-collection={extra.collection}
    aria-current={active ? "page" : undefined}
  >
    <span class="dot" class:accent={extra.accent} class:warn={extra.warn}></span>
    <span class="text">{text}</span>
    {#if extra.badge}<span class="mini-badge">{extra.badge}</span>{/if}
    {#if extra.pinned}<span class="pin" aria-hidden="true"><Pin size={11} strokeWidth={2} /></span>{/if}
    {#if count != null && count !== ""}<span class="count num">{count}</span>{/if}
  </button>
{/snippet}

{#snippet header(text, title, onclick)}
  <div class="group-title with-plus">
    <span>{text}</span>
    <button type="button" class="plus" {onclick} {title} aria-label={title}>
      <Plus size={13} strokeWidth={2} />
    </button>
  </div>
{/snippet}

<aside>
  <div class="head" data-tauri-drag-region>
    <img src="/icon.svg" width="24" height="24" alt="" draggable="false" />
    <span class="brand" data-tauri-drag-region>Cosmic<i>.</i></span>
  </div>

  <nav>
    <div class="group">
      <span class="group-title">{t("group.library")}</span>
      {@render item(t("sec.installed"), section === "installed", () => onsection("installed"), counts.installed)}
      {@render item(t("sec.discover"), section === "discover", () => onsection("discover"), counts.discover)}
      {#if counts.favorites || section === "favorites"}
        {@render item(t("sec.favorites"), section === "favorites", () => onsection("favorites"), counts.favorites, { accent: true })}
      {/if}
      {#if counts.diagnostics > 0 || section === "diagnostics"}
        {@render item(t("sec.diagnostics"), section === "diagnostics", () => onsection("diagnostics"), counts.diagnostics, { warn: true })}
      {/if}
    </div>

    <div class="group">
      {@render header(t("group.collections"), t("col.new"), startNew)}
      {#each collections as c (c.id)}
        {#if renaming === c.id}
          <input
            class="rename"
            bind:this={renameInput}
            bind:value={draft}
            onkeydown={(e) => {
              if (e.key === "Enter") finishRename(true);
              if (e.key === "Escape") (e.stopPropagation(), finishRename(false));
            }}
            onblur={() => finishRename(true)}
          />
        {:else}
          {@render item(c.name, section === "col:" + c.id, () => onsection("col:" + c.id), c.fonts.length, {
            dbl: () => (renaming = c.id),
            collection: c.id,
            target: drag.active && drag.over === c.id,
            pinned: c.pinned,
            title: t("col.hint"),
          })}
        {/if}
      {/each}
      {#if creating}
        <input
          class="rename"
          bind:this={newInput}
          bind:value={newName}
          placeholder={t("col.newName")}
          aria-label={t("col.new")}
          onkeydown={(e) => {
            if (e.key === "Enter") finishNew(true);
            if (e.key === "Escape") (e.stopPropagation(), finishNew(false));
          }}
          onblur={() => finishNew(false)}
        />
      {/if}
    </div>

    <div class="group">
      {@render header(t("group.folders"), t("folder.add"), onaddFolder)}
      {#each folders as f (f.path)}
        {@render item(f.name, section === "dir:" + f.path, () => onsection("dir:" + f.path), f.count, {
          title: f.path,
          badge: projectFolders.has(f.path.toLowerCase()) ? "PROJ" : "",
        })}
      {/each}
    </div>

    {#if visibleCategories.length}
      <div class="group">
        <span class="group-title">{t("group.categories")}</span>
        {#each visibleCategories as c (c)}
          {@render item(t("cat." + c), cat === c, () => oncat(cat === c ? null : c), catCounts[c] ?? 0)}
        {/each}
      </div>
    {/if}

    {#if visibleScripts.length > 1}
      <div class="group">
        <span class="group-title">{t("group.scripts")}</span>
        {#each visibleScripts as s (s)}
          {@render item(s, script === s, () => onscript(script === s ? null : s), scriptCounts[s] ?? 0)}
        {/each}
      </div>
    {/if}
  </nav>

  <div class="foot">
    <UpdateNotice />
    <span class="note num">{footer}</span>
    <div class="row">
      <button type="button" class="settings" onclick={onsettings}>
        <SlidersHorizontal size={15} strokeWidth={1.9} />{t("settings")}
      </button>
      <SegmentedControl
        label={t("set.theme")}
        options={[
          { value: "light", icon: Sun, title: t("light") },
          { value: "dark", icon: Moon, title: t("dark") },
        ]}
        value={currentTheme()}
        onselect={setTheme}
        width={30}
        height={22}
      />
    </div>
    <button type="button" class="version" onclick={onabout}>v{version} · {t("about")}</button>
  </div>
</aside>

<style>
  aside {
    background: var(--side);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 0 10px 14px;
    transition: background-color 0.3s;
  }
  .head { flex: none; height: 52px; display: flex; align-items: center; gap: 8px; padding: 0 8px; }
  .head img { display: block; flex: none; }
  .head .brand { display: flex; align-items: center; height: 24px; font-size: 18px; transform: translateY(1px); }
  nav { flex: 1 1 auto; min-height: 0; overflow-y: auto; margin: 0 -6px; padding: 0 6px 6px; }
  .group { display: flex; flex-direction: column; gap: 1px; margin-top: 14px; }
  .group:first-child { margin-top: 2px; }
  .group-title { padding: 0 10px 6px; font-size: 10.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--ink3); }
  .item {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: 500 13px var(--sans);
    text-align: left;
    transition: background-color 0.15s, color 0.15s, box-shadow 0.15s;
  }
  .item:hover { background: var(--hov); }
  .item.active { background: var(--chip); color: var(--ink); font-weight: 600; }
  .item.target { box-shadow: inset 0 0 0 1.5px var(--accent); background: var(--accent-soft); }
  .dot { width: 7px; height: 7px; flex: none; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--line2); }
  .active .dot { background: var(--ink); box-shadow: none; }
  .dot.accent { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .active .dot.accent { background: var(--accent); box-shadow: none; }
  .dot.warn { box-shadow: inset 0 0 0 1.5px var(--warn); }
  .active .dot.warn { background: var(--warn); box-shadow: none; }
  .with-plus { display: flex; align-items: center; justify-content: space-between; height: 20px; padding: 0 2px 0 10px; margin-bottom: 4px; }
  .plus {
    flex: none; width: 20px; height: 20px; padding: 0; display: grid; place-items: center;
    border: 0; border-radius: 5px; background: transparent; color: var(--ink3);
    transition: background-color 0.15s, color 0.15s;
  }
  .plus :global(svg) { display: block; }
  .plus:hover { background: var(--hov); color: var(--ink); }
  .text { flex: 1 1 auto; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding-right: 1px; }
  .mini-badge { font: 700 9px var(--mono); padding: 1px 4px; border-radius: 4px; background: var(--accent-soft); color: var(--accent); }
  .pin { display: flex; color: var(--accent); opacity: 0.85; }
  .count { margin-left: auto; font-size: 11.5px; color: var(--ink3); }
  .rename {
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: var(--field);
    box-shadow: 0 0 0 1.5px var(--accent);
    font: 600 13px var(--sans);
    color: var(--ink);
    outline: 0;
    width: 100%;
  }
  .rename::placeholder { color: var(--ink3); font-weight: 500; }
  .foot { flex: none; display: flex; flex-direction: column; gap: 10px; padding: 10px 4px 0; }
  .note { font-size: 11.5px; color: var(--ink3); padding: 0 6px; }
  .row { display: flex; align-items: center; gap: 6px; }
  .settings {
    flex: 1;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--ink2);
    font: 600 12.5px var(--sans);
    transition: background-color 0.15s, color 0.15s;
  }
  .settings:hover { background: var(--hov); color: var(--ink); }
  .version { align-self: flex-start; padding: 0 6px; border: 0; background: transparent; color: var(--ink3); font: 500 11px var(--mono); transition: color 0.15s; }
  .version:hover { color: var(--ink); }
</style>
