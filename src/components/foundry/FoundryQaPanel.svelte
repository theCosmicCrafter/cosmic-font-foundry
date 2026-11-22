<script>
  import {
    ShieldCheck,
    ShieldExclamationPoint,
    Wrench,
    Bug,
    Check,
    Play,
    Eye,
    Gauge,
  } from "@lucide/svelte";

  const FILTER_MODES = [
    { id: "all", label: "All" },
    { id: "error", label: "Errors" },
    { id: "warn", label: "Warnings" },
    { id: "fixable", label: "Fixable" },
  ];

  let {
    project = null,
    report = null,
    busy = false,
    onRunAudit = null,
    onRunAutofix = null,
    onSelectGlyph = null,
  } = $props();

  let activeFilter = $state("all");

  function normalizeSeverity(sev) {
    const s = String(sev ?? "").trim().toLowerCase();
    if (s === "error" || s === "err" || s === "fail" || s === "fatal") return "error";
    if (s === "warn" || s === "warning") return "warn";
    return "info";
  }

  function resolveGlyphBadge(glyphKey) {
    if (!glyphKey) return "";
    const g = project?.glyphs?.[glyphKey];
    const cp = g?.codepoint ?? (/^\d+$/.test(String(glyphKey)) ? Number(glyphKey) : 0);
    if (cp > 32) {
      try {
        return String.fromCodePoint(cp);
      } catch {
        return String(glyphKey);
      }
    }
    return g?.name || String(glyphKey);
  }

  const allIssues = $derived(
    Array.isArray(report?.issues) ? report.issues : [],
  );

  const errorCount = $derived.by(() => {
    if (report && Number.isFinite(Number(report.errors))) {
      return Number(report.errors);
    }
    return allIssues.filter((i) => normalizeSeverity(i?.severity) === "error").length;
  });

  const warningCount = $derived.by(() => {
    if (report && Number.isFinite(Number(report.warnings))) {
      return Number(report.warnings);
    }
    return allIssues.filter((i) => normalizeSeverity(i?.severity) === "warn").length;
  });

  const fixableCount = $derived(
    allIssues.filter((i) => Boolean(i?.fixable)).length,
  );

  const scoreValue = $derived.by(() => {
    if (!report || !Number.isFinite(Number(report.score))) return null;
    return Math.max(0, Math.min(100, Math.round(Number(report.score))));
  });

  const scoreTier = $derived.by(() => {
    if (scoreValue === null) return "idle";
    if (scoreValue >= 85) return "good";
    if (scoreValue >= 60) return "warn";
    return "bad";
  });

  const filteredIssues = $derived.by(() => {
    if (activeFilter === "all") return allIssues;
    if (activeFilter === "error") {
      return allIssues.filter((i) => normalizeSeverity(i?.severity) === "error");
    }
    if (activeFilter === "warn") {
      return allIssues.filter((i) => normalizeSeverity(i?.severity) === "warn");
    }
    if (activeFilter === "fixable") {
      return allIssues.filter((i) => Boolean(i?.fixable));
    }
    return allIssues;
  });

  function filterCount(id) {
    if (id === "all") return allIssues.length;
    if (id === "error") return errorCount;
    if (id === "warn") return warningCount;
    if (id === "fixable") return fixableCount;
    return 0;
  }

  async function handleRunAudit() {
    if (busy || !project || !onRunAudit) return;
    await onRunAudit();
  }

  async function handleRunAutofix() {
    if (busy || !project || !onRunAutofix) return;
    await onRunAutofix();
  }
</script>

<div class="foundry-qa-panel">
  <!-- Header Bar -->
  <header class="qa-header">
    <div class="qa-score-group">
      <div class={`qa-score-badge ${scoreTier}`} title="Fontspector Quality Score">
        <Gauge size={14} />
        <span class="qa-score-num num">
          {scoreValue !== null ? scoreValue : "--"}
        </span>
        <span class="qa-score-den">/100</span>
      </div>

      <div class="qa-counts">
        <span class="qa-count err num" title="Errors">
          <ShieldExclamationPoint size={12} />
          <span>{errorCount} {errorCount === 1 ? "error" : "errors"}</span>
        </span>
        <span class="qa-count warn num" title="Warnings">
          <Bug size={12} />
          <span>{warningCount} {warningCount === 1 ? "warning" : "warnings"}</span>
        </span>
      </div>
    </div>

    <div class="qa-actions">
      <button
        type="button"
        class="btn-inline primary"
        disabled={busy || !project || !onRunAudit}
        onclick={handleRunAudit}
      >
        <Play size={11} />
        <span>Run QA Audit</span>
      </button>

      <button
        type="button"
        class="btn-inline accent"
        disabled={!project || busy}
        onclick={handleRunAutofix}
      >
        <Wrench size={11} />
        <span>Auto-Fix All</span>
        {#if fixableCount > 0}
          <span class="qa-fix-count num">{fixableCount}</span>
        {/if}
      </button>
    </div>
  </header>

  <!-- Filter Pills -->
  <div class="qa-filters" role="tablist" aria-label="QA issue filters">
    {#each FILTER_MODES as mode (mode.id)}
      {@const count = filterCount(mode.id)}
      <button
        type="button"
        role="tab"
        aria-selected={activeFilter === mode.id}
        class="qa-filter-pill"
        class:active={activeFilter === mode.id}
        onclick={() => (activeFilter = mode.id)}
      >
        <span>{mode.label}</span>
        <span class="qa-pill-num num">{count}</span>
      </button>
    {/each}
  </div>

  <!-- Issue List / Empty States -->
  {#if !report}
    <div class="qa-empty">
      <div class="qa-empty-icon">
        <ShieldCheck size={22} />
      </div>
      <div class="qa-empty-title">No QA Audit Run Yet</div>
      <p class="qa-empty-desc">
        Click <strong>Run QA Audit</strong> to inspect contour winding, extreme points, overlaps, metrics, and glyph completeness.
      </p>
    </div>
  {:else if allIssues.length === 0}
    <div class="qa-empty pass">
      <div class="qa-empty-icon pass">
        <Check size={20} />
      </div>
      <div class="qa-empty-title">All Checks Passed</div>
      <p class="qa-empty-desc">
        Zero geometry or metadata issues detected across the current font project.
      </p>
    </div>
  {:else if filteredIssues.length === 0}
    <div class="qa-empty">
      <div class="qa-empty-title">No Matching Issues</div>
      <p class="qa-empty-desc">
        No issues match the <code>{activeFilter}</code> filter.
      </p>
      <button
        type="button"
        class="btn-inline"
        onclick={() => (activeFilter = "all")}
      >
        <span>Show All ({allIssues.length})</span>
      </button>
    </div>
  {:else}
    <div class="qa-list">
      {#each filteredIssues as issue, idx (`${issue.code ?? "ISSUE"}_${issue.glyph_key ?? ""}_${idx}`)}
        {@const sev = normalizeSeverity(issue?.severity)}
        {@const glyphBadge = resolveGlyphBadge(issue?.glyph_key)}
        <div class={`qa-issue-row ${sev}`}>
          <div class="qa-issue-main">
            <div class="qa-issue-tags">
              <span class={`qa-sev-pill ${sev}`}>
                {#if sev === "error"}
                  <ShieldExclamationPoint size={10} />
                  <span>ERROR</span>
                {:else if sev === "warn"}
                  <Bug size={10} />
                  <span>WARN</span>
                {:else}
                  <ShieldCheck size={10} />
                  <span>INFO</span>
                {/if}
              </span>

              {#if issue?.code}
                <span class="qa-code-tag">{issue.code}</span>
              {/if}

              {#if issue?.fixable}
                <span class="qa-fixable-tag">
                  <Wrench size={10} />
                  <span>Fixable</span>
                </span>
              {/if}
            </div>

            <div class="qa-issue-msg">{issue?.message ?? "Unspecified QA issue"}</div>
          </div>

          {#if issue?.glyph_key !== undefined && issue?.glyph_key !== null && issue?.glyph_key !== ""}
            <button
              type="button"
              class="btn-inline qa-jump-btn"
              disabled={!onSelectGlyph}
              onclick={() => onSelectGlyph?.(issue.glyph_key)}
            >
              <Eye size={11} />
              <span>Jump to Glyph</span>
              {#if glyphBadge}
                <span class="qa-glyph-chip">{glyphBadge}</span>
              {/if}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .foundry-qa-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--win);
    min-width: 0;
  }

  .qa-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .qa-score-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .qa-score-badge {
    display: inline-flex;
    align-items: baseline;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line2);
    background: var(--surface, var(--side));
    color: var(--ink2);
    align-items: center;
  }

  .qa-score-badge.good {
    border-color: var(--ok, #0d9488);
    background: rgba(13, 148, 136, 0.13);
    color: var(--ok, #0d9488);
  }

  .qa-score-badge.warn {
    border-color: var(--accent-2);
    background: rgba(217, 119, 6, 0.14);
    color: var(--accent-2);
  }

  .qa-score-badge.bad {
    border-color: var(--danger);
    background: rgba(220, 38, 38, 0.14);
    color: var(--danger);
  }

  .qa-score-num {
    font: 700 14px var(--mono);
  }

  .qa-score-den {
    font: 500 10.5px var(--mono);
    opacity: 0.78;
  }

  .qa-counts {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .qa-count {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--field);
    font: 600 11px var(--mono);
    color: var(--ink2);
  }

  .qa-count.err {
    color: var(--danger);
  }

  .qa-count.warn {
    color: var(--accent-2);
  }

  .qa-actions {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .btn-inline {
    height: 25px;
    padding: 0 9px;
    border: 1px solid var(--line2);
    border-radius: 7px;
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    transition: border-color 0.14s, color 0.14s, background-color 0.14s;
  }

  .btn-inline:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--accent-2);
  }

  .btn-inline.primary:not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .btn-inline.accent:not(:disabled) {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn-inline:disabled {
    opacity: 0.48;
    cursor: default;
  }

  .qa-fix-count {
    padding: 0 5px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font: 700 10px/1.5 var(--mono);
  }

  .qa-filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }

  .qa-filter-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--ink2);
    font: 600 11px var(--sans);
    transition: border-color 0.14s, background-color 0.14s, color 0.14s;
  }

  .qa-filter-pill:hover {
    border-color: var(--line2);
    color: var(--ink);
  }

  .qa-filter-pill.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--ink);
  }

  .qa-pill-num {
    font: 600 10px var(--mono);
    color: var(--ink3);
  }

  .qa-filter-pill.active .qa-pill-num {
    color: var(--accent);
  }

  .qa-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 260px;
    overflow-y: auto;
    padding: 4px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--surface, var(--side));
  }

  .qa-issue-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 9px;
    border-radius: 7px;
    border: 1px solid var(--line);
    background: var(--field);
  }

  .qa-issue-row.error {
    border-left: 3px solid var(--danger);
  }

  .qa-issue-row.warn {
    border-left: 3px solid var(--accent-2);
  }

  .qa-issue-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .qa-issue-tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }

  .qa-sev-pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: 4px;
    font: 700 9.5px var(--mono);
    letter-spacing: 0.03em;
  }

  .qa-sev-pill.error {
    background: rgba(220, 38, 38, 0.14);
    color: var(--danger);
  }

  .qa-sev-pill.warn {
    background: rgba(217, 119, 6, 0.15);
    color: var(--accent-2);
  }

  .qa-sev-pill.info {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .qa-code-tag {
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--line);
    background: var(--surface, var(--side));
    color: var(--ink2);
    font: 600 10px var(--mono);
  }

  .qa-fixable-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    font: 600 10px var(--sans);
  }

  .qa-issue-msg {
    font: 400 11.5px/1.35 var(--sans);
    color: var(--ink);
    word-break: break-word;
  }

  .qa-jump-btn {
    flex: none;
  }

  .qa-glyph-chip {
    padding: 0 4px;
    border-radius: 4px;
    background: var(--surface, var(--side));
    border: 1px solid var(--line);
    font: 700 11px var(--mono);
    color: var(--ink);
  }

  .qa-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 6px;
    padding: 20px 14px;
    border-radius: 8px;
    border: 1px dashed var(--line2);
    background: var(--surface, var(--side));
  }

  .qa-empty.pass {
    border-color: var(--ok, #0d9488);
    background: rgba(13, 148, 136, 0.06);
  }

  .qa-empty-icon {
    width: 34px;
    height: 34px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--ink2);
  }

  .qa-empty-icon.pass {
    border-color: var(--ok, #0d9488);
    color: var(--ok, #0d9488);
  }

  .qa-empty-title {
    font: 600 12px var(--sans);
    color: var(--ink);
  }

  .qa-empty-desc {
    margin: 0;
    max-width: 400px;
    font: 400 11px/1.4 var(--sans);
    color: var(--ink3);
  }

  .qa-empty-desc code {
    font: 600 10.5px var(--mono);
    color: var(--ink2);
  }
</style>
