<script lang="ts">
  /**
   * Learn mode (docs/02 §6.1, Quizlet style). The backend picks the cards (deck, query such as
   * `level:A2`), asks each at its stage (multiple choice, then written), judges the answer and
   * schedules the card; this screen only shows and reports. v1.2: a dense start panel; the
   * running session owns the window (`SessionFrame`). Keys: 1–4 pick, Enter check/continue
   * (and on the end screen: learn again), Z undo, Esc end.
   */
  import { formatCount } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LearnAnswerResult, SessionDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    ChevronRight,
    CircleAlert,
    CircleCheck,
    GraduationCap,
    Icon,
    Input,
    PageHeader,
    OptionTile,
    Repeat,
    Skeleton,
    Switch,
    TriangleAlert,
  } from '$lib/ui';
  import OptRow from './OptRow.svelte';
  import SessionFrame from './SessionFrame.svelte';
  import SetupPanel from './SetupPanel.svelte';
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
  let loading = $state(true);
  let query = $state('');
  let practice = $state(false);
  let typed = $state('');
  let picked = $state<number | null>(null);
  let notice = $state<string | null>(null);
  let input = $state<HTMLInputElement | null>(null);
  let continueBtn = $state<HTMLButtonElement | null>(null);
  let busy = false;
  let shownAt = 0;

  onMount(() => {
    trackModules();
    void ctx
      .guarded(async () => {
        const s = (await api.call('study.active', {})).session;
        resumable = s?.kind === 'learn' ? s : null;
      })
      .finally(() => (loading = false));
  });

  const card = $derived(session?.current ?? null);
  const learn = $derived(session?.modes?.learn ?? null);
  // A running session owns the window (v1.2); the end screen is a normal page again.
  $effect(() => ctx.setFocus?.(session !== null && card !== null && learn !== null));
  const progress = $derived((feedback?.session ?? session ?? finished)?.modes?.learn?.progress ?? null);

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
    const tag = (e.target as HTMLElement | null)?.tagName ?? '';
    if (tag === 'BUTTON' && (e.key === 'Enter' || e.key === ' ')) return;
    if (!session) {
      if (e.key === 'Enter' && plainKey(e)) {
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

  const toneOf = (g: string) => (g === 'correct' ? 'success' : g === 'almost' ? 'warning' : 'danger');
</script>
<svelte:window onkeydown={onKey} />

{#if session && card && learn}
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
  <section class="done" role="status">
    <header class="done-head">
      <span class="badge"><Icon icon={GraduationCap} size={20} /></span>
      <div>
        <h1>{t('modes.learn.done_heading')}</h1>
        <p>{t('modes.learn.done', { total: finished.modes?.learn?.progress.total ?? 0 })}</p>
      </div>
    </header>
    <div class="done-actions">
      <Button variant="primary" icon={Repeat} kbd="Enter" onclick={() => void run(start)}>{t('modes.learn.again')}</Button>
      <Button onclick={() => (finished = null)}>{t('modes.learn.back')}</Button>
    </div>
  </section>
{:else}
  <section class="learn">
    <PageHeader title={t('modes.learn.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="120px" />
    {:else}
      <SetupPanel title={t('modes.learn.start_heading')} {notice}>
        {#snippet meta()}
          <span class="steps">
            <span class="st"><span class="step num" aria-hidden="true">1</span>{t('modes.learn.stage_choice')}</span>
            <span class="arrow" aria-hidden="true"><Icon icon={ChevronRight} size={14} /></span>
            <span class="st"><span class="step num" aria-hidden="true">2</span>{t('modes.learn.stage_written')}</span>
            <span class="arrow" aria-hidden="true"><Icon icon={ChevronRight} size={14} /></span>
            <span class="st ok"><Icon icon={CircleCheck} size={14} />{t('modes.learn.learned_card')}</span>
          </span>
        {/snippet}
        {#snippet actions()}
          {#if resumable}
            <Button onclick={() => resumable && void run(() => settle(resumable!))}>{t('study.resume')}</Button>
          {/if}
          <Button variant="primary" kbd="Enter" aria-keyshortcuts="Enter" data-testid="study-start" onclick={() => void run(start)}>
            {t('modes.learn.start')}
          </Button>
        {/snippet}
        <OptRow label={t('study.practice_label')} description={t('study.practice_hint')}>
          <Switch ariaLabel={t('study.practice_label')} bind:checked={practice} />
        </OptRow>
        {#if moduleOn('browse')}
          <OptRow label={t('study.query_label')} labelFor="learn-query">
            <Input
              id="learn-query"
              class="query"
              size="sm"
              label={t('study.query_label')}
              hideLabel
              bind:value={query}
              placeholder={t('study.query_placeholder')}
              spellcheck="false"
            />
          </OptRow>
        {/if}
      </SetupPanel>
    {/if}
  </section>
{/if}

<style>
  .learn {
    display: flex;
    flex-direction: column;
  }
  .learn :global(.query) {
    width: 240px;
  }
  .steps {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .st {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .st.ok {
    color: var(--success);
  }
  .arrow {
    display: inline-flex;
    color: var(--text-3);
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
  .done {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .done-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .done-head h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .done-head p {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .badge {
    display: inline-flex;
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    color: var(--success);
  }
  .done-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  @media (max-width: 720px) {
    .choices {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
