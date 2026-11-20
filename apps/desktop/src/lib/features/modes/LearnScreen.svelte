<script lang="ts">
  /**
   * Learn mode (docs/02 §6.1, Quizlet style). The backend picks the cards (deck, query such as
   * `level:A2`), asks each at its stage (multiple choice, then written), judges the answer and
   * schedules the card; this screen only shows and reports. Keys: 1–4 pick, Enter
   * check/continue, Z undo, Esc end.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LearnAnswerResult, SessionDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import CardView from '../cards/CardView.svelte';
  import { moduleOn, trackModules } from '../cards/modules.svelte';
  import { tzOffsetMins } from '../study/format';
  import type { ScreenCtx } from '../types';
  import { plainKey } from './modes';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let session = $state<SessionDto | null>(null);
  let resumable = $state<SessionDto | null>(null);
  let feedback = $state<LearnAnswerResult | null>(null);
  let finished = $state<SessionDto | null>(null);
  let query = $state('');
  let practice = $state(false);
  let typed = $state('');
  let notice = $state<string | null>(null);
  let input = $state<HTMLInputElement | null>(null);
  let busy = false;
  let shownAt = 0;

  onMount(() => {
    trackModules();
    void ctx.guarded(async () => {
      const s = (await api.call('study.active', {})).session;
      resumable = s?.kind === 'learn' ? s : null;
    });
  });

  const card = $derived(session?.current ?? null);
  const learn = $derived(session?.modes?.learn ?? null);
  const progress = $derived((feedback?.session ?? session ?? finished)?.modes?.learn?.progress ?? null);

  function show(s: SessionDto) {
    session = s;
    feedback = null;
    typed = '';
    shownAt = Date.now();
    queueMicrotask(() => input?.focus());
  }

  async function settle(s: SessionDto) {
    if (s.current) show(s);
    else {
      await api.call('study.end', { session_id: s.id });
      finished = s;
      session = null;
      feedback = null;
      resumable = null;
    }
  }

  async function start() {
    notice = null;
    finished = null;
    const q = query.trim();
    const s = await api.call('study.start', {
      kind: 'learn',
      mode: practice ? 'trial' : 'scheduled',
      deck_id: ctx.selectedDeck,
      filter: q ? { attributes: [], states: null, due_only: null, limit: null, query: q, saved_query_id: null } : null,
      tz_offset_mins: tzOffsetMins(),
    });
    if (!s.current) {
      await api.call('study.end', { session_id: s.id });
      notice = t('modes.learn.nothing');
      return;
    }
    show(s);
  }

  async function answer(choice: number | null) {
    if (!session || !card || feedback) return;
    if (choice === null && typed.trim() === '') return;
    feedback = await api.call('modes.learn.answer', {
      session_id: session.id,
      card_id: card.card_id,
      choice,
      given: choice === null ? typed : null,
      duration_ms: Math.max(0, Date.now() - shownAt),
    });
  }

  async function next() {
    if (feedback) await settle(feedback.session);
  }

  async function undo() {
    if (!session) return;
    await api.call('oplog.undo', {});
    show(await api.call('study.session', { session_id: session.id }));
  }

  async function end() {
    if (session) await settle({ ...session, current: null });
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
    if (!session) {
      if (e.key === 'Enter' && plainKey(e) && !finished) {
        e.preventDefault();
        void run(start);
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      void run(end);
      return;
    }
    if (feedback) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        void run(next);
      } else if (e.key.toLowerCase() === 'z' && plainKey(e)) {
        e.preventDefault();
        void run(undo);
      }
      return;
    }
    if (learn?.stage === 'choice' && plainKey(e) && /^[1-9]$/.test(e.key)) {
      const i = Number(e.key) - 1;
      if (i < learn.options.length) {
        e.preventDefault();
        void run(() => answer(i));
      }
    } else if (learn?.stage === 'written' && e.key === 'Enter') {
      e.preventDefault();
      void run(() => answer(null));
    } else if (e.key.toLowerCase() === 'z' && plainKey(e)) {
      e.preventDefault();
      void run(undo);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<section class="learn">
  <h2>{t('modes.learn.heading')}</h2>
  {#if progress}
    <div class="progress" data-focus-hide>
      <progress max={progress.total} value={progress.learned}></progress>
      <span data-testid="learn-progress">{t('modes.learn.progress', { learned: progress.learned, total: progress.total })}</span>
    </div>
  {/if}

  {#if session && card && learn}
    <CardView sides={card.sides} showAnswer={false} interactive={false} testid="question" />
    <p class="muted">{learn.stage === 'choice' ? t('modes.learn.stage_choice') : t('modes.learn.stage_written')}</p>
    {#if learn.stage === 'choice'}
      <div class="choices" role="group" aria-label={t('modes.learn.stage_choice')}>
        {#each learn.options as option, i (i)}
          <button
            class:right={feedback !== null && option === feedback.expected}
            disabled={feedback !== null}
            onclick={() => void run(() => answer(i))}
          >
            <span>{option}</span><kbd>{i + 1}</kbd>
          </button>
        {/each}
      </div>
    {:else}
      <div class="row">
        <input
          bind:this={input}
          bind:value={typed}
          disabled={feedback !== null}
          placeholder={t('modes.learn.type_placeholder')}
          aria-label={t('modes.learn.type_placeholder')}
        />
        <button disabled={feedback !== null} onclick={() => void run(() => answer(null))}>{t('modes.learn.check')}</button>
      </div>
    {/if}
    {#if feedback}
      <div class="feedback {feedback.grade}" role="status" data-testid="learn-feedback">
        <strong>{t(`modes.learn.${feedback.grade}`)}</strong>
        {#if feedback.learned}<span>{t('modes.learn.learned_card')}</span>{/if}
        {#if feedback.grade !== 'correct'}<p>{t('modes.learn.expected', { text: feedback.expected })}</p>{/if}
        <button class="primary" onclick={() => void run(next)}>{t('modes.learn.continue')} <kbd>{'Enter'}</kbd></button>
      </div>
    {/if}
    <p class="muted keys">{t('modes.learn.keys')}</p>
  {:else if finished}
    <div class="panel" role="status">
      <p>{t('modes.learn.done', { total: finished.modes?.learn?.progress.total ?? 0 })}</p>
      <div class="row">
        <button class="primary" onclick={() => void run(start)}>{t('modes.learn.again')}</button>
        <button onclick={() => (finished = null)}>{t('modes.learn.back')}</button>
      </div>
    </div>
  {:else}
    <div class="panel">
      <p class="muted">{t('modes.learn.intro')}</p>
      <p class="muted">{ctx.selectedPath ?? t('study.whole_collection')}</p>
      {#if resumable}
        <button onclick={() => resumable && void run(() => settle(resumable!))}>{t('study.resume')}</button>
      {/if}
      <label class="row">
        <input type="checkbox" bind:checked={practice} />
        {t('study.practice')}
      </label>
      {#if moduleOn('browse')}
        <label>
          {t('study.query_label')}
          <input bind:value={query} placeholder={t('study.query_placeholder')} spellcheck="false" />
        </label>
      {/if}
      <button class="primary" onclick={() => void run(start)}>{t('modes.learn.start')} <kbd>{'Enter'}</kbd></button>
      {#if notice}<p class="muted" role="status">{notice}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .learn {
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  .learn :global(.card) {
    font-size: 1.3em;
    padding: var(--fc-space-5);
  }
  .panel,
  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .choices button {
    display: flex;
    justify-content: space-between;
    text-align: left;
  }
  .choices .right {
    border-color: var(--fc-success);
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .row input:not([type='checkbox']) {
    flex: 1;
  }
  .progress {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .feedback {
    border-left: 4px solid var(--fc-danger);
    padding-left: var(--fc-space-3);
  }
  .feedback.correct {
    border-color: var(--fc-success);
  }
  .feedback p {
    margin: var(--fc-space-1) 0;
  }
  kbd {
    color: var(--fc-text-muted);
    font-size: 0.8em;
    margin-left: var(--fc-space-1);
  }
  .keys {
    font-size: 0.85em;
  }
</style>
