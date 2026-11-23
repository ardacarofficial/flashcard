<script lang="ts">
  /**
   * Learn mode (docs/02 §6.1, Quizlet style). The backend picks the cards (deck, query such as
   * `level:A2`), asks each at its stage (multiple choice, then written), judges the answer and
   * schedules the card; this screen only shows and reports. v1.3: sessions start in the
   * study launcher (no setup panel here); the running session owns the window
   * (`SessionFrame`). Keys: 1–4 pick, Enter check/continue, Z undo, Esc end; a held key is one
   * press.
   *
   * v1.4: a quiz session ("Test": one word, four meanings, the answer at once) arrives on this
   * route too and runs on its own screen (`QuizRun`). One key = one job: Space never goes on
   * here (Space only flips cards); Enter does.
   */
  import { formatCount } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LearnAnswerResult, LearnDto, SessionDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    CircleAlert,
    CircleCheck,
    Icon,
    Input,
    PageHeader,
    OptionTile,
    Skeleton,
    TriangleAlert,
  } from '$lib/ui';
  import SessionFrame from './SessionFrame.svelte';
  import SessionSummary from './SessionSummary.svelte';
  import QuizRun from './QuizRun.svelte';
  import CardView from '../cards/CardView.svelte';
  import { trackModules } from '../cards/modules.svelte';
  import type { StudyScope } from '../study/contract';
  import LaunchIdle from '../study/LaunchIdle.svelte';
  import { handoffSeq, launcher, openLauncher, takeHandoff, type Handoff } from '../study/launcher.svelte';
  import type { ScreenCtx } from '../types';
  import { plainKey } from './modes';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let session = $state<SessionDto | null>(null);
  let feedback = $state<LearnAnswerResult | null>(null);
  let finished = $state<SessionDto | null>(null);
  let typed = $state('');
  let picked = $state<number | null>(null);
  let input = $state<HTMLInputElement | null>(null);
  let continueBtn = $state<HTMLButtonElement | null>(null);
  let busy = false;
  let shownAt = 0;
  /** The scope the session was started with ("Learn again" builds on it). */
  let startedScope = $state<StudyScope | null>(null);

  /** v1.4: a Test (quiz) handed to this route runs on its own screen. */
  let quizRun = $state<Handoff | null>(null);

  // A session started (or continued) in the study launcher arrives here (learn or quiz).
  const initial = takeHandoff('learn') ?? takeHandoff('quiz');
  let accepting = $state(initial !== null);
  let hadSession = $state(false);

  function accept(h: Handoff) {
    if (h.kind === 'quiz') {
      session = null;
      finished = null;
      quizRun = h;
      hadSession = true;
      accepting = false;
      return;
    }
    quizRun = null;
    startedScope = h.scope;
    finished = null;
    accepting = true;
    hadSession = true;
    void run(() => settle(h.session)).finally(() => (accepting = false));
  }

  $effect(() => {
    void handoffSeq();
    const h = takeHandoff('learn') ?? takeHandoff('quiz');
    if (h) accept(h);
  });

  onMount(() => {
    trackModules();
    if (initial) accept(initial);
  });

  // The shell's global undo (Ctrl+Z, palette) may take back an answer: show the card again.
  $effect(() => {
    const unsub = api.on(['undone', 'redone'], () => {
      if (session && !busy) void run(async () => show(await api.call('study.session', { session_id: session!.id })));
    });
    return () => void unsub.then((u) => u());
  });

  const card = $derived(session?.current ?? null);
  const learn = $derived<LearnDto | null>(session?.modes?.learn ?? null);
  // A running session owns the window (v1.2); the end screen is a normal page again.
  $effect(() => ctx.setFocus?.(session !== null && card !== null && learn !== null));
  const progress = $derived((feedback?.session ?? session ?? finished)?.modes?.learn?.progress ?? learn?.progress ?? null);

  function show(s: SessionDto) {
    session = s;
    feedback = null;
    typed = '';
    picked = null;
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
    }
  }

  /** "Learn again": the launcher on this deck, today's struggles first when there were any. */
  function learnAgain() {
    const struggled = (finished?.mistakes ?? 0) > 0;
    openLauncher({ deckId: finished?.deck_id ?? ctx.selectedDeck, kind: 'learn', scope: struggled ? 'struggled_today' : (startedScope ?? undefined) });
  }

  async function answer(choice: number | null) {
    if (!session || !card || feedback) return;
    if (choice === null && typed.trim() === '') return;
    picked = choice;
    feedback = await api.call('modes.learn.answer', {
      session_id: session.id,
      card_id: card.card_id,
      choice,
      given: choice === null ? typed : null,
      duration_ms: Math.max(0, Date.now() - shownAt),
    });
    queueMicrotask(() => continueBtn?.focus());
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
    // Handled already (the palette's Esc), an open dialog, or a held key: not ours.
    if (e.defaultPrevented || e.repeat || document.querySelector('[aria-modal="true"]')) return;
    const tag = (e.target as HTMLElement | null)?.tagName ?? '';
    if (tag === 'BUTTON' && (e.key === 'Enter' || e.key === ' ')) return;
    if (!session || quizRun) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      void run(end);
      return;
    }
    if (feedback) {
      // Enter goes on, from the answer field too. Space does not: one key = one job.
      if (e.key === 'Enter') {
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
    } else if (learn?.stage === 'written' && e.key === 'Enter' && tag === 'INPUT') {
      e.preventDefault();
      void run(() => answer(null));
    } else if (e.key.toLowerCase() === 'z' && plainKey(e)) {
      e.preventDefault();
      void run(undo);
    }
  }

  const toneOf = (g: string) => (g === 'correct' ? 'success' : g === 'almost' ? 'warning' : 'danger');
</script>
<svelte:window onkeydown={onKey} />

{#if quizRun}
  {#key quizRun}
    <QuizRun {ctx} handoff={quizRun} onclose={() => (quizRun = null)} />
  {/key}
{:else if session && card && learn}
  <SessionFrame
    title={t('modes.learn.heading')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('modes.learn.progress_label')}
    max={progress?.total ?? 1}
    segments={progress
      ? [
          { value: progress.learned, label: t('modes.learn.seg_learned'), tone: 'success' },
          { value: progress.written, label: t('modes.learn.seg_written'), tone: 'accent' },
        ]
      : undefined}
    count={progress ? t('study.position', { n: progress.learned, total: progress.total }) : null}
    countTestid="learn-progress"
    exitLabel={t('modes.learn.end')}
    exitText={t('study.end')}
    onexit={() => void run(end)}
  >
    {#snippet meta()}
      {#if progress}
        <span class="cnt"><span class="k">{t('modes.learn.seg_learned')}</span><span class="num v">{formatCount(progress.learned)}</span></span>
        <span class="cnt"><span class="k">{t('modes.learn.count_written')}</span><span class="num v">{formatCount(progress.written)}</span></span>
        <span class="cnt"><span class="k">{t('modes.learn.count_choice')}</span><span class="num v">{formatCount(progress.choice)}</span></span>
      {/if}
    {/snippet}

    <div class="question">
      <div class="q-top">
        <span class="stage" class:written={learn.stage !== 'choice'}>
          <span class="step num" aria-hidden="true">{learn.stage === 'choice' ? 1 : 2}</span>
          {learn.stage === 'choice' ? t('modes.learn.stage_choice') : t('modes.learn.stage_written')}
        </span>
        {#if learn.mistakes > 0}<span class="again">{t('modes.learn.missed', { n: learn.mistakes })}</span>{/if}
      </div>
      <div class="prompt">
        <CardView sides={card.sides} showAnswer={false} interactive={false} testid="question" />
      </div>
    </div>

    {#if learn.stage === 'choice'}
      <div class="choices" role="group" aria-label={t('modes.learn.stage_choice')}>
        {#each learn.options as option, i (i)}
          {@const right = feedback !== null && option === feedback.expected}
          {@const miss = feedback !== null && picked === i && !right}
          <OptionTile
            kbd={String(i + 1)}
            label={option}
            result={right ? 'right' : miss ? 'miss' : null}
            rightLabel={t('modes.learn.correct_option')}
            missLabel={t('modes.learn.your_pick')}
            disabled={feedback !== null}
            onclick={() => void run(() => answer(i))}
          />
        {/each}
      </div>
    {:else}
      <div class="written">
        <Input
          bind:element={input}
          bind:value={typed}
          size="lg"
          disabled={feedback !== null}
          label={t('modes.learn.type_placeholder')}
          hideLabel
          placeholder={t('modes.learn.type_placeholder')}
          spellcheck="false"
          autocomplete="off"
        />
        <Button size="lg" variant="primary" kbd="Enter" disabled={feedback !== null} onclick={() => void run(() => answer(null))}>
          {t('modes.learn.check')}
        </Button>
      </div>
    {/if}

    {#if feedback}
      <div class="feedback {toneOf(feedback.grade)}" role="status" data-testid="learn-feedback">
        <Icon icon={feedback.grade === 'correct' ? CircleCheck : feedback.grade === 'almost' ? TriangleAlert : CircleAlert} size={20} />
        <div class="fb-text">
          <strong>{t(`modes.learn.${feedback.grade}`)}</strong>
          {#if feedback.learned}<span class="learned">{t('modes.learn.learned_card')}</span>{/if}
          {#if feedback.grade !== 'correct'}<span class="exp">{t('modes.learn.expected', { text: feedback.expected })}</span>{/if}
        </div>
        <Button variant="primary" kbd="Enter" bind:element={continueBtn} onclick={() => void run(next)}>
          {t('modes.learn.continue')}
        </Button>
      </div>
    {/if}
    <p class="keys" data-focus-hide>
      {#if learn.stage === 'choice' && !feedback}<span><kbd>1–{learn.options.length}</kbd> {t('modes.learn.key_pick')}</span>{/if}
      <span><kbd>↵</kbd> {t('modes.learn.key_check')}</span>
      <span><kbd>Z</kbd> {t('study.undo')}</span>
    </p>
  </SessionFrame>
{:else if finished}
  {@const lp = finished.modes?.learn?.progress}
  <SessionSummary
    testid="learn-summary"
    heading={t('modes.learn.done_heading')}
    sub={t('modes.learn.done', { total: lp?.total ?? 0 })}
    percent={null}
    tiles={[
      { label: t('modes.learn.tile_learned'), value: formatCount(lp?.learned ?? 0) },
      { label: t('modes.learn.tile_answers'), value: formatCount(finished.answered) },
      { label: t('modes.learn.tile_missed'), value: formatCount(finished.mistakes) },
    ]}
    practice={finished.mode === 'trial'}
    scheduled={lp?.learned ?? 0}
    onagain={launcher.available ? learnAgain : undefined}
    ondone={() => (finished = null)}
  />
{:else if accepting}
  <section class="learn">
    <PageHeader title={t('modes.learn.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    <Skeleton shape="card" height="120px" />
  </section>
{:else}
  <LaunchIdle {ctx} kind="learn" title={t('modes.learn.heading')} auto={!hadSession} />
{/if}

<style>
  .learn {
    display: flex;
    flex-direction: column;
    /* The start page is one column: the header (deck picker) lines up with the panel. */
    max-width: 760px;
  }
  .step {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-size: var(--text-2xs);
    color: var(--text-1);
  }
  .cnt {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
  }
  .cnt .v {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  /* Session. */
  .question {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-flashcard);
    background: var(--surface-1);
    padding: var(--space-3) var(--space-4) var(--space-6);
  }
  .q-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .stage {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: var(--weight-medium);
  }
  .stage.written .step {
    border-color: var(--accent-text);
    color: var(--accent-text);
  }
  .prompt {
    min-height: 160px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4) var(--space-4) 0;
    text-align: center;
  }
  .prompt :global(.card) {
    background: transparent;
    border: none;
    padding: 0;
    min-height: 0;
    font-size: var(--text-card);
    line-height: var(--leading-tight);
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
  .written {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
  }
  .written :global(.fc-field) {
    flex: 1;
  }
  .feedback {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-left: 3px solid var(--fb);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .feedback.success {
    --fb: var(--success);
  }
  .feedback.warning {
    --fb: var(--warning);
  }
  .feedback.danger {
    --fb: var(--danger);
  }
  .feedback > :global(svg),
  .feedback strong {
    color: var(--fb);
  }
  .fb-text {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px var(--space-3);
    min-width: 0;
  }
  .learned {
    font-size: var(--text-sm);
    color: var(--success);
    font-weight: var(--weight-medium);
  }
  .exp {
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .again {
    color: var(--warning);
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .keys kbd {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
    margin-right: 2px;
  }
  /* End: a normal page again. */
  @media (max-width: 720px) {
    .choices {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
