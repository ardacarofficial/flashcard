<script lang="ts">
  /**
   * Match (v1.1, docs/11 §3): six terms and six definitions as shuffled tiles; pick a term and
   * its definition — right pairs leave the board, a wrong pair shakes (a colour flash under
   * reduced motion). The backend deals the round, checks every pick, times it on its own clock
   * and keeps the deck's best time (`modes.match.*`); this screen only shows and reports.
   * Keys: on the result Enter plays again (the focused button), Esc ends. v1.3: rounds start
   * in the study launcher (what to match from, how many); no setup panel here.
   */
  import { formatDecimal } from '$lib/format';
  import { onDestroy, onMount, tick as nextTick } from 'svelte';
  import { api } from '$lib/api';
  import type { MatchResultDto, MatchRoundDto, MatchTileDto, SessionDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Chip, EmptyState, Icon, PageHeader, Play, Shuffle, Skeleton, Timer, Trophy } from '$lib/ui';
  import SessionFrame from './SessionFrame.svelte';
  import { trackModules } from '../cards/modules.svelte';
  import type { StudyScope } from '../study/contract';
  import LaunchIdle from '../study/LaunchIdle.svelte';
  import { handoffSeq, launcher, openLauncher, takeHandoff, type Handoff } from '../study/launcher.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let round = $state<MatchRoundDto | null>(null);
  let result = $state<MatchResultDto | null>(null);
  let tooFew = $state(false);
  let deckOf = $state<string | null>(null);
  let startedScope = $state<StudyScope | null>(null);
  let againBtn = $state<HTMLButtonElement | null>(null);
  let selected = $state<number | null>(null);
  /** Tiles of the last wrong pair (shake / flash for a moment). */
  let wrong = $state<number[]>([]);
  let now = $state(Date.now());
  /** When this screen got the board (display only; the backend times the round). */
  let shownAt = 0;
  let busy = false;
  let tick: ReturnType<typeof setInterval> | null = null;
  let wrongTimer: ReturnType<typeof setTimeout> | null = null;

  // A round started (or continued) in the study launcher arrives here.
  const initial = takeHandoff('match');
  let accepting = $state(initial !== null);
  let hadSession = $state(false);

  function accept(h: Handoff) {
    startedScope = h.scope;
    accepting = true;
    hadSession = true;
    void run(() => open(h.session)).finally(() => (accepting = false));
  }

  $effect(() => {
    void handoffSeq();
    const h = takeHandoff('match');
    if (h) accept(h);
  });

  onMount(() => {
    trackModules();
    if (initial) accept(initial);
  });

  $effect(() => {
    if (result) void nextTick().then(() => againBtn?.focus());
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
  // A running round owns the window (v1.2); the result is a normal page again.
  $effect(() => ctx.setFocus?.(playing));
  const elapsed = $derived(playing ? Math.max(0, now - shownAt) : (result?.time_ms ?? 0));

  const secs = (ms: number) => formatDecimal(Math.floor(ms / 100) / 10, 1);

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

  /** Deals a round of a started (or continued) match session. */
  async function open(s: SessionDto) {
    result = null;
    deckOf = s.deck_id;
    const r = await api.call('modes.match.start', { session_id: s.id });
    if (r.tiles.length === 0) await api.call('study.end', { session_id: s.id });
    show(r);
  }

  /** Another round of the same session. */
  async function again() {
    if (!round) return;
    show(await api.call('modes.match.start', { session_id: round.session_id }));
  }

  /** "Study again": the launcher on this deck in Match. */
  async function studyAgain() {
    const deckId = deckOf ?? ctx.selectedDeck;
    const missed = (result?.mistakes ?? 0) > 0;
    await end();
    openLauncher({ deckId, kind: 'match', scope: missed ? 'struggled_today' : (startedScope ?? undefined) });
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
    if (e.defaultPrevented || e.repeat) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    if (e.key === 'Escape' && round) {
      e.preventDefault();
      void run(end);
    }
  }
</script>
<svelte:window onkeydown={onKey} />

{#if playing && round}
  <SessionFrame
    title={t('modes.match.heading')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('modes.match.progress_label')}
    value={round.matched}
    max={round.pairs}
    count={t('study.position', { n: round.matched, total: round.pairs })}
    exitLabel={t('modes.match.end')}
    exitText={t('study.end')}
    onexit={() => void run(end)}
    width={960}
  >
    {#snippet meta()}
      <span>{t('modes.match.pairs_left', { n: round!.pairs - round!.matched })}</span>
      {#if round!.mistakes > 0}<span class="miss">{t('modes.match.mistakes', { n: round!.mistakes })}</span>{/if}
    {/snippet}
    {#snippet extra()}
      {#if round!.best_ms !== null}
        <span class="best"><Icon icon={Trophy} size={14} /><span class="num">{t('modes.match.best', { s: secs(round!.best_ms) })}</span></span>
      {/if}
      <span class="clock" role="timer" aria-label={t('modes.match.time_label')}>
        <Icon icon={Timer} size={16} />
        <span class="secs num" data-testid="match-clock">{t('modes.match.seconds', { s: secs(elapsed) })}</span>
      </span>
    {/snippet}

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
          data-side={tile.side}
          onclick={() => void run(() => pick(tile))}
        >
          <span class="text">{tile.text}</span>
        </button>
      {/each}
    </div>
    <p class="how">{t('modes.match.how')}</p>
  </SessionFrame>
{:else if result && round}
  <section class="done" role="status" data-testid="match-result">
    <header class="done-head">
      <span class="trophy" class:best={result.new_best}><Icon icon={Trophy} size={20} /></span>
      <div>
        <p class="time num">{t('modes.match.seconds', { s: secs(result.time_ms) })}</p>
        <p class="sub">{t('modes.match.summary', { pairs: result.pairs, mistakes: result.mistakes })}</p>
      </div>
      {#if result.new_best}
        <Chip tone="success" icon={Trophy} label={t('modes.match.new_best')} />
      {:else if round.best_ms !== null}
        <span class="best-line num">{t('modes.match.best', { s: secs(round.best_ms) })}</span>
      {/if}
    </header>
    <div class="done-actions">
      <Button variant="primary" icon={Shuffle} bind:element={againBtn} data-testid="match-again" onclick={() => void run(again)}>
        {t('modes.match.again')}
      </Button>
      {#if launcher.available}
        <Button icon={Play} title={t('study.summary.again_desc')} onclick={() => void run(studyAgain)}>{t('study.summary.again')}</Button>
      {/if}
      <Button kbd="Esc" aria-keyshortcuts="Escape" onclick={() => void run(end)}>{t('modes.match.done')}</Button>
    </div>
  </section>
{:else if tooFew}
  <section class="match">
    <PageHeader title={t('modes.match.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    <div class="empty">
      <EmptyState
        size="sm"
        icon={Shuffle}
        title={t('modes.match.too_few')}
        description={t('modes.match.too_few_hint')}
        action={{ label: t('modes.match.back'), onclick: () => void run(end) }}
      />
    </div>
  </section>
{:else if accepting}
  <section class="match">
    <PageHeader title={t('modes.match.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    <Skeleton shape="card" height="120px" />
  </section>
{:else}
  <LaunchIdle {ctx} kind="match" title={t('modes.match.heading')} auto={!hadSession} />
{/if}

<style>
  .match {
    display: flex;
    flex-direction: column;
    /* The start page is one column: the header (deck picker) lines up with the panel. */
    max-width: 760px;
  }
  .empty {
    max-width: 760px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  /* Session bar extras. */
  .miss {
    color: var(--warning);
  }
  .best {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .clock {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-1);
  }
  .secs {
    min-width: 7ch;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  /* Board. */
  .board {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .tile {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 104px;
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-1);
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
  .tile.definition {
    color: var(--text-1);
    background: var(--surface-0);
  }
  .tile:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .tile:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .tile.selected {
    border-color: var(--accent-text);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent-text);
  }
  .tile.wrong {
    border-color: var(--danger);
    box-shadow: inset 0 0 0 1px var(--danger);
    animation: shake 0.3s var(--ease);
  }
  .tile.gone {
    opacity: 0;
    transform: scale(0.96);
    pointer-events: none;
    cursor: default;
  }
  @keyframes shake {
    20%,
    60% {
      transform: translateX(-4px);
    }
    40%,
    80% {
      transform: translateX(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tile {
      transition: none;
    }
    .tile.wrong {
      animation: none;
      background: var(--danger-soft);
    }
    .tile.gone {
      transform: none;
    }
  }
  .how {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  /* Result: a normal page again. */
  .done {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 760px;
  }
  .done-head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
  }
  .trophy {
    display: inline-flex;
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    color: var(--text-2);
  }
  .trophy.best {
    color: var(--success);
    border-color: color-mix(in srgb, var(--success) 45%, var(--border));
  }
  .time {
    margin: 0;
    font-size: var(--text-3xl);
    font-weight: var(--weight-semibold);
    line-height: 1.1;
  }
  .sub {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .best-line {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .done-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  @media (max-width: 1000px) {
    .board {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .tile {
      min-height: 88px;
    }
  }
</style>
