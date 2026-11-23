<!--
  Test (v1.4, backend kind `quiz`; prototype `modlar.html`): one word, its meaning among up to
  four options; pick with 1–4 or a click and the right answer shows at once (green, the wrong
  pick red), with the card's back (sentence, source). Enter goes on; a right pick goes on by
  itself (`QUIZ_AUTO_NEXT_MS`). Esc pauses (the session stays open: "Continue" here, on Home and
  in the launcher). The end is the shared summary: score, missed words, "study the missed ones
  again" (R, `study.start { kind: quiz, from_session }`), what it did to the schedule. The core
  picks the options, judges, logs and keeps the score (`modes.quiz.answer`, `modes.quiz.result`).
  Space does nothing here: Space only flips cards.
-->
<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { QuizAnswerResult, QuizResultDto, SessionDto } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Calendar, CircleAlert, CircleCheck, CirclePause, Icon, OptionTile, Play } from '$lib/ui';
  import SessionFrame from './SessionFrame.svelte';
  import SessionSummary from './SessionSummary.svelte';
  import CardView from '../cards/CardView.svelte';
  import { launcher, openLauncher, type Handoff } from '../study/launcher.svelte';
  import { tzOffsetMins } from '../study/format';
  import type { ScreenCtx } from '../types';
  import { plainKey, QUIZ_AUTO_NEXT_MS } from './modes';

  interface Props {
    ctx: ScreenCtx;
    handoff: Handoff;
    /** Leave the Test screen (Done). */
    onclose: () => void;
  }
  let { ctx, handoff, onclose }: Props = $props();

  let session = $state<SessionDto | null>(null);
  let feedback = $state<QuizAnswerResult | null>(null);
  let result = $state<QuizResultDto | null>(null);
  let paused = $state(false);
  let continueBtn = $state<HTMLButtonElement | null>(null);
  let resumeBtn = $state<HTMLButtonElement | null>(null);
  let busy = false;
  let shownAt = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const card = $derived(session?.current ?? null);
  const quiz = $derived(session?.modes?.quiz ?? null);
  const progress = $derived((feedback?.session ?? session)?.modes?.quiz?.progress ?? null);
  // A running Test owns the window; the pause and the summary are normal pages.
  $effect(() => ctx.setFocus?.(session !== null && card !== null && !paused));

  onMount(() => void run(() => settle(handoff.session)));
  onDestroy(clearTimer);

  function clearTimer() {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  function show(s: SessionDto) {
    session = s;
    feedback = null;
    shownAt = Date.now();
  }

  async function settle(s: SessionDto) {
    clearTimer();
    if (s.current) {
      show(s);
      return;
    }
    await api.call('study.end', { session_id: s.id });
    result = await api.call('modes.quiz.result', { session_id: s.id });
    session = null;
    feedback = null;
  }

  async function answer(choice: number) {
    if (!session || !card || feedback || paused) return;
    const r = await api.call('modes.quiz.answer', { session_id: session.id, card_id: card.card_id, choice, duration_ms: Math.max(0, Date.now() - shownAt) });
    feedback = r;
    if (r.correct && QUIZ_AUTO_NEXT_MS > 0) {
      const shown = r;
      timer = setTimeout(() => {
        if (feedback === shown) void run(next);
      }, QUIZ_AUTO_NEXT_MS);
    }
    void tick().then(() => continueBtn?.focus());
  }

  async function next() {
    if (feedback) await settle(feedback.session);
  }

  function pause() {
    clearTimer();
    if (feedback) {
      // The answer is kept: pausing on the feedback continues with the next word.
      session = feedback.session;
      feedback = null;
    }
    paused = true;
    void tick().then(() => resumeBtn?.focus());
  }

  async function resume() {
    if (!session) return;
    paused = false;
    await settle(session);
  }

  /** Ends the Test now: the score of what was answered. */
  async function finishNow() {
    if (session) await settle({ ...session, current: null });
  }

  /** "Study the missed ones again": a Test of exactly the words this one missed. */
  async function missedAgain() {
    if (!result) return;
    const s = await api.call('study.start', { kind: 'quiz', deck_id: handoff.session.deck_id, from_session: result.session_id, tz_offset_mins: tzOffsetMins() });
    result = null;
    paused = false;
    await settle(s);
  }

  /** "Study again…": the launcher on this deck, today's struggles first. */
  function again() {
    openLauncher({ deckId: handoff.session.deck_id ?? ctx.selectedDeck, kind: 'quiz', scope: (result?.wrong ?? 0) > 0 ? 'struggled_today' : (handoff.scope ?? undefined) });
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
    if (e.defaultPrevented || e.repeat || document.querySelector('[aria-modal="true"]')) return;
    if (paused) {
      if (e.key === 'Enter' && (e.target as HTMLElement | null)?.tagName !== 'BUTTON') {
        e.preventDefault();
        void run(resume);
      }
      return;
    }
    if (!session || !card) return;
    const tag = (e.target as HTMLElement | null)?.tagName ?? '';
    if (e.key === 'Escape') {
      e.preventDefault();
      pause();
      return;
    }
    if (feedback) {
      if (e.key === 'Enter') {
        e.preventDefault();
        void run(next);
      }
      return;
    }
    if (tag === 'BUTTON' && e.key === 'Enter') return;
    if (quiz && plainKey(e) && /^[1-9]$/.test(e.key)) {
      const i = Number(e.key) - 1;
      if (i < quiz.options.length) {
        e.preventDefault();
        void run(() => answer(i));
      }
    }
  }

  const affects = $derived((feedback?.session ?? session)?.modes?.quiz?.affects_schedule ?? false);
  const last = $derived(progress ? progress.answered >= progress.total : false);
</script>
<svelte:window onkeydown={onKey} />

{#if result}
  <SessionSummary
    testid="quiz-summary"
    subTestid="quiz-score"
    heading={t('modes.quiz.done_heading')}
    sub={t('modes.quiz.done', { correct: result.correct, total: result.total })}
    percent={result.answered > 0 ? result.percent : null}
    tiles={[
      { label: t('modes.quiz.tile_questions'), value: formatCount(result.answered) },
      { label: t('modes.quiz.tile_right'), value: formatCount(result.correct) },
      { label: t('modes.quiz.tile_wrong'), value: formatCount(result.wrong) },
    ]}
    practice={!result.affects_schedule}
    scheduled={result.answered}
    rowsHeading={t('modes.quiz.missed_heading')}
    columns={[t('modes.quiz.col_word'), t('modes.quiz.col_right'), t('modes.quiz.col_yours')]}
    rows={result.missed.map((m) => ({ term: m.term, right: m.expected, yours: m.picked ?? t('modes.quiz.no_pick') }))}
    repeatLabel={result.missed.length ? `${t('modes.quiz.missed_again')} · ${formatCount(result.missed.length)}` : undefined}
    onrepeat={result.missed.length ? () => void run(missedAgain) : undefined}
    onagain={launcher.available ? again : undefined}
    ondone={onclose}
  />
{:else if session && paused}
  <section class="paused" role="status" data-testid="quiz-paused">
    <span class="badge"><Icon icon={CirclePause} size={20} /></span>
    <div>
      <h1>{t('modes.quiz.paused_heading')}</h1>
      <p>{t('modes.quiz.paused', { answered: formatCount(progress?.answered ?? 0), left: formatCount((progress?.total ?? 0) - (progress?.answered ?? 0)) })}</p>
    </div>
    <div class="acts">
      <Button variant="primary" icon={Play} kbd="Enter" bind:element={resumeBtn} data-testid="quiz-resume" onclick={() => void run(resume)}>{t('study.launcher.resume')}</Button>
      <Button onclick={() => void run(finishNow)}>{t('modes.quiz.finish_now')}</Button>
    </div>
  </section>
{:else if session && card && quiz}
  <SessionFrame
    title={t('study.mode.quiz.name')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('modes.quiz.progress_label')}
    value={progress?.answered ?? 0}
    max={progress?.total ?? 1}
    count={progress ? t('study.position', { n: progress.answered, total: progress.total }) : null}
    countTestid="quiz-progress"
    exitLabel={t('modes.quiz.pause_label')}
    exitText={t('modes.quiz.pause')}
    onexit={pause}
  >
    {#snippet meta()}
      {#if progress}
        <span class="num">{t('modes.quiz.tally', { right: progress.correct, wrong: progress.answered - progress.correct })}</span>
      {/if}
    {/snippet}
    {#snippet extra()}
      <span class="chip" class:cal={affects} data-testid="quiz-schedule-chip">
        {#if affects}<Icon icon={Calendar} size={12} />{t('study.launcher.chip.affects')}{:else}{t('study.launcher.chip.practice')}{/if}
      </span>
    {/snippet}

    <p class="how">{t('modes.quiz.how')} <span class="muted">{t('modes.quiz.how_keys', { n: quiz.options.length })}</span></p>
    <div class="question">
      {#if quiz.term}
        <p class="term" data-testid="question">{quiz.term}</p>
      {:else}
        <div class="prompt"><CardView sides={card.sides} showAnswer={false} interactive={false} testid="question" /></div>
      {/if}
      {#if quiz.few_options}<p class="few" role="note">{t('modes.quiz.few_options', { count: quiz.options.length })}</p>{/if}
    </div>

    <div class="choices" role="group" aria-label={t('modes.quiz.options_label')} data-testid="quiz-options">
      {#each quiz.options as option, i (i)}
        {@const right = feedback !== null && i === feedback.correct_option}
        {@const miss = feedback !== null && i === feedback.picked && !feedback.correct}
        <OptionTile
          kbd={String(i + 1)}
          label={option}
          result={right ? 'right' : miss ? 'miss' : null}
          rightLabel={t('modes.learn.correct_option')}
          missLabel={t('modes.learn.your_pick')}
          disabled={feedback !== null}
          class={feedback !== null && !right && !miss ? 'dim' : ''}
          onclick={() => void run(() => answer(i))}
        />
      {/each}
    </div>

    {#if feedback}
      <div class="feedback" class:ok={feedback.correct} role="status" data-testid="quiz-feedback" data-correct={feedback.correct}>
        <Icon icon={feedback.correct ? CircleCheck : CircleAlert} size={20} />
        <div class="fb-text">
          <strong>{feedback.correct ? t('modes.learn.correct') : t('modes.quiz.wrong', { text: feedback.expected })}</strong>
          <div class="back"><CardView sides={card.sides} showAnswer={true} interactive={false} /></div>
          {#if feedback.correct && QUIZ_AUTO_NEXT_MS > 0}<span class="auto">{t('modes.quiz.auto_next')}</span>{/if}
        </div>
        <Button variant="primary" kbd="Enter" bind:element={continueBtn} data-testid="quiz-next" onclick={() => void run(next)}>
          {last ? t('modes.quiz.see_result') : t('modes.quiz.next')}
        </Button>
      </div>
    {/if}
    <p class="keys" data-focus-hide>
      <span><kbd>1–{quiz.options.length}</kbd> {t('modes.learn.key_pick')}</span>
      <span><kbd>↵</kbd> {t('modes.quiz.next')}</span>
      <span><kbd>Esc</kbd> {t('modes.quiz.pause')}</span>
    </p>
  </SessionFrame>
{/if}

<style>
  .how {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .muted {
    color: var(--text-2);
  }
  .question {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: 140px;
    padding: var(--space-5) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-flashcard);
    background: var(--surface-1);
    text-align: center;
  }
  .term {
    margin: 0;
    font-size: var(--text-card);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }
  .prompt :global(.card) {
    background: transparent;
    border: none;
    padding: 0;
    min-height: 0;
    font-size: var(--text-card);
  }
  .few {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .choices :global(.fc-option) {
    min-height: 52px;
    font-size: var(--text-md);
  }
  .choices :global(.fc-option.dim) {
    opacity: 0.55;
  }
  .feedback {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--danger);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--danger);
  }
  .feedback.ok {
    border-color: var(--success);
    color: var(--success);
  }
  .fb-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    color: var(--text-1);
  }
  .back {
    max-height: 180px;
    overflow: auto;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .back :global(.card) {
    background: transparent;
    border: none;
    padding: 0;
    min-height: 0;
    font-size: var(--text-sm);
    text-align: start;
  }
  .auto {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-2xs);
    color: var(--text-2);
    white-space: nowrap;
  }
  .chip.cal {
    color: var(--accent-text);
    border-color: var(--accent-text);
  }
  .keys {
    display: flex;
    gap: var(--space-4);
    justify-content: center;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .keys kbd {
    font-family: var(--font-mono);
  }
  .paused {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: var(--space-3) var(--space-4);
    align-items: center;
    max-width: 640px;
  }
  .paused h1 {
    margin: 0;
    font-size: var(--text-xl);
  }
  .paused p {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
  }
  .paused .acts {
    grid-column: 2;
    display: flex;
    gap: var(--space-2);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
  }
</style>
