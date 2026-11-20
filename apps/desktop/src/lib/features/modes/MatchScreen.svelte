<script lang="ts">
  /**
   * Match (v1.1, docs/11 §3): six terms and six definitions as shuffled tiles; pick a term and
   * its definition — right pairs leave the board, a wrong pair shakes (a colour flash under
   * reduced motion). The backend deals the round, checks every pick, times it on its own clock
   * and keeps the deck's best time (`modes.match.*`); this screen only shows and reports.
   * Keys: Enter starts / plays again, Esc ends.
   */
  import { onDestroy, onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { MatchResultDto, MatchRoundDto, MatchTileDto } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Card,
    Chip,
    EmptyState,
    Icon,
    IconButton,
    Input,
    Kbd,
    PageHeader,
    Shuffle,
    Skeleton,
    Timer,
    Trophy,
    X,
  } from '$lib/ui';
  import { moduleOn, trackModules } from '../cards/modules.svelte';
  import { tzOffsetMins } from '../study/format';
  import type { ScreenCtx } from '../types';
  import { plainKey } from './modes';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let round = $state<MatchRoundDto | null>(null);
  let result = $state<MatchResultDto | null>(null);
  let resumable = $state<string | null>(null);
  let loading = $state(true);
  let tooFew = $state(false);
  let query = $state('');
  let selected = $state<number | null>(null);
  /** Tiles of the last wrong pair (shake / flash for a moment). */
  let wrong = $state<number[]>([]);
  let now = $state(Date.now());
  /** When this screen got the board (display only; the backend times the round). */
  let shownAt = 0;
  let busy = false;
  let tick: ReturnType<typeof setInterval> | null = null;
  let wrongTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    trackModules();
    void ctx
      .guarded(async () => {
        const s = (await api.call('study.active', {})).session;
        resumable = s?.kind === 'match' ? s.id : null;
      })
      .finally(() => (loading = false));
  });

  onDestroy(() => {
    stopClock();
    if (wrongTimer) clearTimeout(wrongTimer);
  });

  function startClock() {
    stopClock();
    tick = setInterval(() => (now = Date.now()), 100);
  }

  function stopClock() {
    if (tick) clearInterval(tick);
    tick = null;
  }

  const playing = $derived(round !== null && round.tiles.length > 0 && result === null);
  // A running round switches the shell to the focus layout (no picker, no tabs).
  $effect(() => ctx.setFocus?.(playing));
  const elapsed = $derived(playing ? Math.max(0, now - shownAt) : (result?.time_ms ?? 0));

  const fmt = $derived(new Intl.NumberFormat(i18n.locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }));
  const secs = (ms: number) => fmt.format(Math.floor(ms / 100) / 10);

  function show(r: MatchRoundDto) {
    round = r;
    result = null;
    selected = null;
    wrong = [];
    tooFew = r.tiles.length === 0;
    shownAt = Date.now();
    now = shownAt;
    if (tooFew) stopClock();
    else startClock();
  }

  async function start() {
    const q = query.trim();
    result = null;
    const s = await api.call('study.start', {
      kind: 'match',
      mode: 'scheduled',
      deck_id: ctx.selectedDeck,
      filter: q ? { attributes: [], states: null, due_only: null, limit: null, query: q, saved_query_id: null } : null,
      tz_offset_mins: tzOffsetMins(),
    });
    const r = await api.call('modes.match.start', { session_id: s.id });
    if (r.tiles.length === 0) await api.call('study.end', { session_id: s.id });
    resumable = null;
    show(r);
  }

  async function resume(id: string) {
    resumable = null;
    show(await api.call('modes.match.start', { session_id: id }));
  }

  async function again() {
    if (!round) return start();
    show(await api.call('modes.match.start', { session_id: round.session_id }));
  }

  async function end() {
    stopClock();
    if (round && round.tiles.length > 0) await api.call('study.end', { session_id: round.session_id });
    round = null;
    result = null;
    tooFew = false;
  }

  async function pick(tile: MatchTileDto) {
    if (!round || tile.matched || result) return;
    if (selected === null || selected === tile.id) {
      selected = selected === tile.id ? null : tile.id;
      return;
    }
    const first = round.tiles.find((x) => x.id === selected);
    // Same half: the new tile becomes the selection.
    if (!first || first.side === tile.side) {
      selected = tile.id;
      return;
    }
    const r = await api.call('modes.match.pick', { session_id: round.session_id, first: first.id, second: tile.id });
    round = r.round;
    selected = null;
    if (!r.correct) {
      wrong = [first.id, tile.id];
      if (wrongTimer) clearTimeout(wrongTimer);
      wrongTimer = setTimeout(() => (wrong = []), 450);
    }
    if (r.complete) {
      stopClock();
      result = await api.call('modes.match.finish', { session_id: round.session_id });
      round = result.round;
    }
  }

  async function run(fn: () => Promise<unknown>) {
    if (busy) return;
    busy = true;
    try {
      await ctx.guarded(fn);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    const tag = (e.target as HTMLElement | null)?.tagName ?? '';
    if (tag === 'BUTTON' && (e.key === 'Enter' || e.key === ' ')) return;
    if (e.key === 'Escape' && round) {
      e.preventDefault();
      void run(end);
    } else if (e.key === 'Enter' && plainKey(e) && !playing && !tooFew) {
      e.preventDefault();
      void run(result ? again : start);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<section class="match">
  {#if playing && round}
    <div class="bar">
      <div class="clock" role="timer" aria-label={t('modes.match.time_label')}>
        <Icon icon={Timer} size={20} />
        <span class="secs" data-testid="match-clock">{t('modes.match.seconds', { s: secs(elapsed) })}</span>
      </div>
      <div class="facts">
        <span>{t('modes.match.pairs_left', { n: round.pairs - round.matched })}</span>
        {#if round.mistakes > 0}<Chip size="sm" tone="warning" label={t('modes.match.mistakes', { n: round.mistakes })} />{/if}
        {#if round.best_ms !== null}
          <span class="best"><Icon icon={Trophy} size={16} />{t('modes.match.best', { s: secs(round.best_ms) })}</span>
        {/if}
      </div>
      <IconButton icon={X} label={t('modes.match.end')} onclick={() => void run(end)} />
    </div>
    <div class="board" role="group" aria-label={t('modes.match.board')} data-testid="match-board">
      {#each round.tiles as tile (tile.id)}
        <button
          type="button"
          class="tile {tile.side}"
          class:selected={selected === tile.id}
          class:wrong={wrong.includes(tile.id)}
          class:gone={tile.matched}
          disabled={tile.matched}
          aria-hidden={tile.matched || undefined}
          aria-pressed={selected === tile.id}
          aria-label={t(tile.side === 'term' ? 'modes.match.term' : 'modes.match.definition', { text: tile.text })}
          data-testid="match-tile"
          onclick={() => void run(() => pick(tile))}
        >
          <span class="text">{tile.text}</span>
        </button>
      {/each}
    </div>
    <p class="keys">{t('modes.match.how')}</p>
  {:else if result && round}
    <div class="done">
      <Card padding="lg" elevated>
        <div class="result" role="status" data-testid="match-result">
          <span class="trophy" class:best={result.new_best}><Icon icon={Trophy} size={24} /></span>
          <p class="time">{t('modes.match.seconds', { s: secs(result.time_ms) })}</p>
          {#if result.new_best}
            <Chip tone="success" label={t('modes.match.new_best')} />
          {:else if round.best_ms !== null}
            <p class="sub">{t('modes.match.best', { s: secs(round.best_ms) })}</p>
          {/if}
          <p class="sub">
            {t('modes.match.summary', { pairs: result.pairs, mistakes: result.mistakes })}
          </p>
          <div class="actions">
            <Button onclick={() => void run(end)}>{t('modes.match.done')}</Button>
            <Button variant="primary" icon={Shuffle} onclick={() => void run(again)}>
              {t('modes.match.again')} <Kbd keys={['Enter']} class="on-accent" />
            </Button>
          </div>
        </div>
      </Card>
    </div>
  {:else if tooFew}
    <PageHeader title={t('modes.match.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')} />
    <Card padding="lg">
      <EmptyState
        icon={Shuffle}
        title={t('modes.match.too_few')}
        description={t('modes.match.too_few_hint')}
        action={{ label: t('modes.match.back'), onclick: () => void run(end) }}
      />
    </Card>
  {:else}
    <PageHeader title={t('modes.match.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="200px" />
    {:else}
      <Card padding="lg" class="start-card">
        <div class="start">
          <div class="preview" aria-hidden="true">
            {#each [0, 1, 2, 3, 4, 5] as i (i)}<span class="mini" class:alt={i % 3 === 1}></span>{/each}
          </div>
          <div class="start-text">
            <h2>{t('modes.match.start_heading')}</h2>
            <p>{t('modes.match.intro')}</p>
          </div>
        </div>
        {#if moduleOn('browse')}
          <div class="query">
            <Input label={t('study.query_label')} bind:value={query} placeholder={t('study.query_placeholder')} spellcheck="false" />
          </div>
        {/if}
        <div class="actions">
          {#if resumable}
            <Button onclick={() => resumable && void run(() => resume(resumable!))}>{t('study.resume')}</Button>
          {/if}
          <Button variant="primary" size="lg" icon={Shuffle} data-testid="study-start" onclick={() => void run(start)}>
            {t('modes.match.start')} <Kbd keys={['Enter']} class="on-accent" />
          </Button>
        </div>
      </Card>
    {/if}
  {/if}
</section>

<style>
  .match {
    max-width: 960px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  :global(.start-card) {
    max-width: 720px;
  }
  .start {
    display: flex;
    align-items: center;
    gap: var(--space-6);
  }
  .preview {
    flex: none;
    display: grid;
    grid-template-columns: repeat(3, 28px);
    gap: 6px;
  }
  .mini {
    height: 20px;
    border-radius: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .mini.alt {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .start-text h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .start-text p {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    max-width: 48ch;
  }
  .query {
    margin-top: var(--space-6);
    max-width: 420px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-6);
    flex-wrap: wrap;
  }
  :global(.on-accent kbd) {
    background: transparent;
    border-color: color-mix(in srgb, var(--accent-fg) 45%, transparent);
    color: var(--accent-fg);
  }
  /* Board. */
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .clock {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-1);
  }
  .secs {
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
    min-width: 6ch;
  }
  .facts {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .best {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .board {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-3);
  }
  .tile {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 120px;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    box-shadow: var(--shadow-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    line-height: var(--leading-tight);
    text-align: center;
    overflow-wrap: anywhere;
    cursor: pointer;
    transition:
      border-color var(--motion-fast) var(--ease),
      background var(--motion-fast) var(--ease),
      opacity var(--motion) var(--ease),
      transform var(--motion) var(--ease);
  }
  .tile.term .text {
    font-weight: var(--weight-semibold);
  }
  .tile:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .tile:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .tile.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .tile.wrong {
    border-color: var(--danger);
    background: var(--danger-soft);
    animation: shake 0.36s var(--ease);
  }
  .tile.gone {
    opacity: 0;
    transform: scale(0.94);
    pointer-events: none;
    cursor: default;
  }
  @keyframes shake {
    20%,
    60% {
      transform: translateX(-6px);
    }
    40%,
    80% {
      transform: translateX(6px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tile {
      transition: none;
    }
    .tile.wrong {
      animation: none;
    }
    .tile.gone {
      transform: none;
    }
  }
  .keys {
    margin: 0;
    text-align: center;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  /* Result. */
  .done {
    max-width: 480px;
    width: 100%;
    margin-top: var(--space-4);
  }
  .result {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    text-align: center;
  }
  .trophy {
    display: inline-flex;
    padding: var(--space-3);
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .trophy.best {
    background: var(--success-soft);
    color: var(--success);
  }
  .time {
    margin: var(--space-2) 0 0;
    font-size: var(--text-3xl);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
  }
  .sub {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .result .actions {
    justify-content: center;
  }
  @media (max-width: 1000px) {
    .board {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .tile {
      min-height: 96px;
    }
  }
</style>
