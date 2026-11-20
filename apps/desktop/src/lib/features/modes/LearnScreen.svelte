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
  import {
    Button,
    Card,
    Chip,
    CircleAlert,
    CircleCheck,
    GraduationCap,
    Icon,
    IconButton,
    Input,
    Kbd,
    PageHeader,
    ProgressBar,
    Repeat,
    Skeleton,
    Switch,
    TriangleAlert,
    X,
  } from '$lib/ui';
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

  const toneOf = (g: string) => (g === 'correct' ? 'success' : g === 'almost' ? 'warning' : 'danger');
</script>

<svelte:window onkeydown={onKey} />

<section class="learn">
  {#if session && card && learn}
    <div class="topbar">
      {#if progress}
        <div class="progress" data-focus-hide>
          <ProgressBar
            label={t('modes.learn.progress_label')}
            max={progress.total}
            size="sm"
            segments={[
              { value: progress.learned, label: t('modes.learn.seg_learned'), tone: 'success' },
              { value: progress.written, label: t('modes.learn.seg_written'), tone: 'accent' },
            ]}
          />
          <span class="count" data-testid="learn-progress">{t('modes.learn.progress', { learned: progress.learned, total: progress.total })}</span>
        </div>
      {/if}
      <IconButton icon={X} label={t('modes.learn.end')} onclick={() => void run(end)} />
    </div>

    <Card padding="lg" elevated class="question-card">
      <div class="stage-row">
        <Chip size="sm" tone={learn.stage === 'choice' ? 'neutral' : 'accent'} label={learn.stage === 'choice' ? t('modes.learn.stage_choice') : t('modes.learn.stage_written')} />
        {#if learn.mistakes > 0}<span class="again">{t('modes.learn.missed', { n: learn.mistakes })}</span>{/if}
      </div>
      <div class="prompt">
        <CardView sides={card.sides} showAnswer={false} interactive={false} testid="question" />
      </div>
    </Card>

    {#if learn.stage === 'choice'}
      <div class="choices" role="group" aria-label={t('modes.learn.stage_choice')}>
        {#each learn.options as option, i (i)}
          {@const right = feedback !== null && option === feedback.expected}
          {@const miss = feedback !== null && picked === i && !right}
          <button
            type="button"
            class="option"
            class:right
            class:miss
            disabled={feedback !== null}
            onclick={() => void run(() => answer(i))}
          >
            <Kbd keys={[String(i + 1)]} />
            <span class="opt-text">{option}</span>
            {#if right}<Icon icon={CircleCheck} size={20} label={t('modes.learn.correct_option')} />{/if}
            {#if miss}<Icon icon={CircleAlert} size={20} label={t('modes.learn.your_pick')} />{/if}
          </button>
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
        />
        <Button size="lg" variant="primary" disabled={feedback !== null} onclick={() => void run(() => answer(null))}>
          {t('modes.learn.check')}
        </Button>
      </div>
    {/if}

    {#if feedback}
      <div class="feedback {toneOf(feedback.grade)}" role="status" data-testid="learn-feedback">
        <Icon icon={feedback.grade === 'correct' ? CircleCheck : feedback.grade === 'almost' ? TriangleAlert : CircleAlert} size={24} />
        <div class="fb-text">
          <strong>{t(`modes.learn.${feedback.grade}`)}</strong>
          {#if feedback.learned}<span class="learned">{t('modes.learn.learned_card')}</span>{/if}
          {#if feedback.grade !== 'correct'}<span class="exp">{t('modes.learn.expected', { text: feedback.expected })}</span>{/if}
        </div>
        <Button variant="primary" bind:element={continueBtn} onclick={() => void run(next)}>
          {t('modes.learn.continue')} <Kbd keys={['Enter']} class="on-accent" />
        </Button>
      </div>
    {/if}
    <p class="keys">
      <span><Kbd keys={['1']} />–<Kbd keys={['4']} /> {t('modes.learn.key_pick')}</span>
      <span><Kbd keys={['Enter']} /> {t('modes.learn.key_check')}</span>
      <span><Kbd keys={['Z']} /> {t('study.undo')}</span>
      <span><Kbd keys={['Esc']} /> {t('study.end')}</span>
    </p>
  {:else if finished}
    <div class="done" role="status">
      <Card padding="lg" elevated>
        <div class="done-body">
          <span class="badge"><Icon icon={GraduationCap} size={24} /></span>
          <h2>{t('modes.learn.done_heading')}</h2>
          <p>{t('modes.learn.done', { total: finished.modes?.learn?.progress.total ?? 0 })}</p>
          <div class="actions">
            <Button onclick={() => (finished = null)}>{t('modes.learn.back')}</Button>
            <Button variant="primary" icon={Repeat} onclick={() => void run(start)}>{t('modes.learn.again')}</Button>
          </div>
        </div>
      </Card>
    </div>
  {:else}
    <PageHeader title={t('modes.learn.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="200px" />
    {:else}
      <Card padding="lg" class="start-card">
        <div class="steps" aria-hidden="true">
          <span class="step"><span class="n">1</span>{t('modes.learn.stage_choice')}</span>
          <span class="line"></span>
          <span class="step"><span class="n">2</span>{t('modes.learn.stage_written')}</span>
          <span class="line"></span>
          <span class="step"><Icon icon={CircleCheck} size={16} />{t('modes.learn.learned_card')}</span>
        </div>
        <p class="intro">{t('modes.learn.intro')}</p>
        <div class="form">
          <Switch label={t('study.practice_label')} description={t('study.practice_hint')} bind:checked={practice} />
          {#if moduleOn('browse')}
            <Input label={t('study.query_label')} bind:value={query} placeholder={t('study.query_placeholder')} spellcheck="false" />
          {/if}
        </div>
        {#if notice}<p class="notice" role="status">{notice}</p>{/if}
        <div class="actions">
          {#if resumable}
            <Button onclick={() => resumable && void run(() => settle(resumable!))}>{t('study.resume')}</Button>
          {/if}
          <Button variant="primary" size="lg" data-testid="study-start" onclick={() => void run(start)}>
            {t('modes.learn.start')} <Kbd keys={['Enter']} class="on-accent" />
          </Button>
        </div>
      </Card>
    {/if}
  {/if}
</section>

<style>
  .learn {
    max-width: 760px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  :global(.start-card) {
    max-width: 720px;
  }
  .steps {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .step {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  .line {
    width: 24px;
    height: 1px;
    background: var(--border-strong);
  }
  .intro {
    margin: var(--space-4) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    max-width: 56ch;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    margin-top: var(--space-6);
    max-width: 420px;
  }
  .notice {
    margin: var(--space-4) 0 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-sm);
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
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .progress {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .progress :global(.fc-progress) {
    flex: 1;
  }
  .count {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    white-space: nowrap;
  }
  .stage-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .again {
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .prompt {
    min-height: 140px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6) 0 var(--space-2);
    text-align: center;
  }
  .prompt :global(.card) {
    background: transparent;
    border: none;
    padding: 0;
    min-height: 0;
    font-size: var(--text-2xl);
    line-height: var(--leading-tight);
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
  }
  .option {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 64px;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    text-align: left;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background var(--motion-fast) var(--ease);
  }
  .opt-text {
    flex: 1;
  }
  .option:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .option:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .option:disabled {
    cursor: default;
    opacity: 1;
    color: var(--text-2);
  }
  .option.right {
    border-color: var(--success);
    background: var(--success-soft);
    color: var(--text-1);
  }
  .option.right :global(svg) {
    color: var(--success);
  }
  .option.miss {
    border-color: var(--danger);
    background: var(--danger-soft);
  }
  .option.miss :global(svg) {
    color: var(--danger);
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
    padding: var(--space-4);
    border-radius: var(--radius-card);
    border: 1px solid var(--border);
  }
  .feedback.success {
    background: var(--success-soft);
    border-color: color-mix(in srgb, var(--success) 35%, transparent);
  }
  .feedback.success > :global(svg) {
    color: var(--success);
  }
  .feedback.warning {
    background: var(--warning-soft);
    border-color: color-mix(in srgb, var(--warning) 35%, transparent);
  }
  .feedback.warning > :global(svg) {
    color: var(--warning);
  }
  .feedback.danger {
    background: var(--danger-soft);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .feedback.danger > :global(svg) {
    color: var(--danger);
  }
  .fb-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .learned {
    font-size: var(--text-sm);
    color: var(--success);
    font-weight: var(--weight-medium);
  }
  .exp {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .keys {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: var(--space-4);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .keys span {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .done {
    max-width: 480px;
    width: 100%;
    margin-top: var(--space-4);
  }
  .done-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-2);
  }
  .done-body h2 {
    margin: var(--space-2) 0 0;
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
  }
  .done-body p {
    margin: 0;
    color: var(--text-2);
  }
  .badge {
    display: inline-flex;
    padding: var(--space-3);
    border-radius: 50%;
    background: var(--success-soft);
    color: var(--success);
  }
  .done-body .actions {
    justify-content: center;
  }
  @media (max-width: 720px) {
    .choices {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
