<!--
  The end of a session (v1.4, prototype `modlar.html`): the same shape in every mode — a score
  ring (or a big figure), what happened in one sentence, a few figures, ONE line on what it did
  to the schedule, then "Study the missed ones again (R)", "Study again…" and "Done", and the
  list of what was missed. Everything shown is the caller's API data; this only lays it out.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { formatPercent100 } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Calendar, Icon, Info, Play, ProgressRing, Repeat, StatTile } from '$lib/ui';

  interface Row {
    term: string;
    right: string;
    yours: string;
  }

  interface Props {
    heading: string;
    sub: string;
    /** 0–100, or null (no ring). */
    percent: number | null;
    tiles: { label: string; value: string }[];
    /** The answers left the schedule alone. */
    practice: boolean;
    /** Cards whose next review was set (when not practice). */
    scheduled?: number;
    /** What was missed (with the column titles). */
    rows?: Row[];
    rowsHeading?: string;
    columns?: [string, string, string];
    /** "Study the missed ones again" (R): absent when nothing was missed. */
    repeatLabel?: string;
    onrepeat?: () => void;
    /** "Study again…" (the launcher). */
    onagain?: () => void;
    ondone: () => void;
    testid?: string;
    subTestid?: string;
  }

  let { heading, sub, percent, tiles, practice, scheduled = 0, rows = [], rowsHeading, columns, repeatLabel, onrepeat, onagain, ondone, testid, subTestid = 'summary-sub' }: Props = $props();

  let first = $state<HTMLButtonElement | null>(null);
  let again = $state<HTMLButtonElement | null>(null);
  let done = $state<HTMLButtonElement | null>(null);

  onMount(() => queueMicrotask(() => (first ?? again ?? done)?.focus()));

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented || e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    const tag = (e.target as HTMLElement | null)?.tagName ?? '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
    if (e.key.toLowerCase() === 'r' && onrepeat) {
      e.preventDefault();
      onrepeat();
    }
  }
</script>
<svelte:window onkeydown={onKey} />

<section class="summary" role="status" data-testid={testid}>
  <header class="head">
    {#if percent !== null}
      <ProgressRing label={t('modes.summary.score_label')} value={percent} size={64} stroke={6} tone={percent >= 80 ? 'success' : percent >= 50 ? 'warning' : 'danger'}>
        <span class="pct num">{formatPercent100(percent)}</span>
      </ProgressRing>
    {/if}
    <div>
      <h1>{heading}</h1>
      <p data-testid={subTestid}>{sub}</p>
    </div>
  </header>
  <div class="tiles">
    {#each tiles as tile (tile.label)}
      <StatTile size="sm" label={tile.label} value={tile.value} />
    {/each}
  </div>
  <p class="sched" data-testid="summary-schedule">
    {#if practice}
      <Icon icon={Info} size={14} />{t('modes.summary.practice')}
    {:else}
      <Icon icon={Calendar} size={14} />{t('modes.summary.scheduled', { count: scheduled })}
    {/if}
  </p>
  <div class="acts">
    {#if onrepeat && repeatLabel}
      <Button variant="primary" icon={Repeat} kbd="R" bind:element={first} data-testid="summary-repeat" onclick={onrepeat}>{repeatLabel}</Button>
    {/if}
    {#if onagain}
      <Button variant={onrepeat ? 'secondary' : 'primary'} icon={Play} bind:element={again} data-testid="summary-again" onclick={onagain}>{t('modes.summary.again')}</Button>
    {/if}
    <Button variant="ghost" bind:element={done} data-testid="summary-done" onclick={ondone}>{t('modes.summary.done')}</Button>
  </div>
  {#if rows.length && columns}
    <h2 class="sec">{rowsHeading}</h2>
    <div class="rows" role="table" data-testid="summary-missed">
      <div class="row th" role="row">
        {#each columns as c (c)}<span role="columnheader">{c}</span>{/each}
      </div>
      {#each rows as r, i (i)}
        <div class="row" role="row">
          <span role="cell"><strong>{r.term}</strong></span>
          <span role="cell" class="right">{r.right}</span>
          <span role="cell" class="yours">{r.yours}</span>
        </div>
      {/each}
    </div>
  {:else if rowsHeading !== undefined}
    <p class="none">{t('modes.summary.none_missed')}</p>
  {/if}
</section>

<style>
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 760px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
  }
  .head p {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
  }
  .pct {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: var(--space-2);
  }
  .sched {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .acts {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .sec {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .rows {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .row:first-child {
    border-top: none;
  }
  .row.th {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .right {
    color: var(--success);
  }
  .yours {
    color: var(--text-2);
  }
  .none {
    margin: 0;
    color: var(--text-2);
  }
</style>
