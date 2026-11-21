<script lang="ts">
  /**
   * Practice tests (docs/02 §6.1): the backend builds a paper of mixed question types from
   * the chosen cards (deck, query such as `exam:TYT`), grades it on submission and never
   * changes the schedule. The paper prints (with or without the answer key). v1.2: an open
   * paper owns the window (`SessionFrame`, Esc leaves — after a confirmation once something is
   * answered; Ctrl+Enter submits). After grading: a score ring and a dense list of misses.
   */
  import { formatPercent100 } from '$lib/format';
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { TestAnswerDto, TestPaperDto, TestQuestionDto, TestQuestionKind, TestVerdict } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Chip,
    CircleAlert,
    CircleCheck,
    ClipboardCheck,
    Dialog,
    Icon,
    IconButton,
    Input,
    Minus,
    OptionTile,
    PageHeader,
    Printer,
    ProgressRing,
    Skeleton,
    TriangleAlert,
    type Tone,
  } from '$lib/ui';
  import OptRow from './OptRow.svelte';
  import SessionFrame from './SessionFrame.svelte';
  import SetupPanel from './SetupPanel.svelte';
  import { moduleOn, trackModules } from '../cards/modules.svelte';
  import { tzOffsetMins } from '../study/format';
  import type { ScreenCtx } from '../types';
  import { emptyInput, testAnswers, type PaperInput } from './modes';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: TestQuestionKind[] = ['true_false', 'multiple_choice', 'written'];

  let paper = $state<TestPaperDto | null>(null);
  let input = $state<PaperInput>(emptyInput());
  let query = $state('');
  let count = $state<number | string>(20);
  let kinds = $state<Record<TestQuestionKind, boolean>>({ true_false: true, multiple_choice: true, written: true });
  let notice = $state<string | null>(null);
  let loading = $state(true);
  let startedAt = 0;
  let top = $state<HTMLElement | null>(null);
  /** "Leave the test?" — an unfinished paper with answers is only discarded on confirmation. */
  let confirmOpen = $state(false);

  onMount(() => {
    trackModules();
    void api
      .call('settings.get', { scope: { kind: 'app' }, keys: ['practice_test.questions'] })
      .then(
        (r) => {
          const v = r.values.find((x) => x.key === 'practice_test.questions')?.value;
          if (typeof v === 'number') count = v;
        },
        () => {},
      )
      .finally(() => (loading = false));
  });

  const result = $derived(paper?.result ?? null);
  // An open paper (not yet graded) switches the shell to the focus layout.
  $effect(() => ctx.setFocus?.(paper !== null && result === null));
  const verdict = (i: number): TestVerdict | null => result?.items.find((x) => x.index === i)?.verdict ?? null;
  const answered = $derived(paper ? testAnswers(paper.questions, input).length : 0);
  const misses = $derived(paper && result ? paper.questions.filter((q) => verdict(q.index) !== 'correct') : []);
  const noKind = $derived(!KINDS.some((k) => kinds[k]));

  const VERDICT: Record<TestVerdict, { tone: Tone; icon: typeof CircleCheck }> = {
    correct: { tone: 'success', icon: CircleCheck },
    almost: { tone: 'warning', icon: TriangleAlert },
    wrong: { tone: 'danger', icon: CircleAlert },
    blank: { tone: 'neutral', icon: Minus },
  };

  async function start() {
    notice = null;
    const q = query.trim();
    const chosen = KINDS.filter((k) => kinds[k]);
    const s = await api.call('study.start', {
      kind: 'test',
      mode: 'scheduled',
      deck_id: ctx.selectedDeck,
      filter: q ? { attributes: [], states: null, due_only: null, limit: null, query: q, saved_query_id: null } : null,
      tz_offset_mins: tzOffsetMins(),
      modes: { timed: null, rating_scale: null, test: { questions: Number(count) || 20, kinds: chosen.length ? chosen : null } },
    });
    if ((s.modes?.test?.questions ?? 0) === 0) {
      await api.call('study.end', { session_id: s.id });
      notice = t('modes.test.nothing');
      return;
    }
    paper = await api.call('modes.test.paper', { session_id: s.id, with_key: null });
    const fresh = emptyInput();
    for (const q of paper.questions) if (q.kind === 'written') fresh.text[q.index] = '';
    input = fresh;
    startedAt = Date.now();
  }

  async function submit() {
    if (!paper) return;
    paper = await api.call('modes.test.submit', {
      session_id: paper.session_id,
      answers: testAnswers(paper.questions, input),
      duration_ms: Math.max(0, Date.now() - startedAt),
    });
    await tick();
    // Back to the score: scroll the shell's content area, not the whole window.
    const scroller = top?.closest('main');
    if (scroller) scroller.scrollTop = 0;
    else top?.scrollIntoView?.({ block: 'start' });
  }

  async function printKey() {
    if (!paper) return;
    const keyed = await api.call('modes.test.paper', { session_id: paper.session_id, with_key: true });
    const before = paper;
    paper = { ...before, questions: keyed.questions };
    await tick();
    window.print();
    if (!before.result) paper = before;
  }

  async function newTest() {
    if (paper) await api.call('study.end', { session_id: paper.session_id });
    paper = null;
  }

  /** Exit button / Esc on an open paper: leave at once when nothing is answered yet. */
  function requestExit() {
    if (!paper || result) return;
    if (answered > 0) confirmOpen = true;
    else void ctx.guarded(newTest);
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented || !paper || result || confirmOpen) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      requestExit();
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void ctx.guarded(submit);
    }
  }

  const truthText = (b: boolean | null | undefined) => (b ? t('modes.test.true') : t('modes.test.false'));
  const keyText = (q: TestQuestionDto) => (q.kind === 'true_false' ? truthText(q.key?.truth) : (q.key?.expected ?? ''));
  const given = (q: TestQuestionDto): string => {
    const a: TestAnswerDto | undefined = paper?.answers?.find((x) => x.index === q.index);
    if (!a) return t('modes.test.blank_answer');
    if (q.kind === 'multiple_choice' && a.choice != null) return q.options[a.choice] ?? '';
    if (q.kind === 'true_false' && a.truth != null) return truthText(a.truth);
    return a.text ?? t('modes.test.blank_answer');
  };
  const LETTERS = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'];
  /** Graded paper: the right option (when the key is known) and a wrong pick. */
  const tileResult = (q: TestQuestionDto, picked: boolean, isKey: boolean): 'right' | 'miss' | null => {
    if (!result) return null;
    if (q.key && isKey) return 'right';
    return picked && verdict(q.index) !== 'correct' ? 'miss' : picked ? 'right' : null;
  };
  const truthOptions = $derived([
    { value: true, label: t('modes.test.true') },
    { value: false, label: t('modes.test.false') },
  ]);
</script>
<svelte:window onkeydown={onKey} />

{#snippet questions()}
  <ol class="paper">
    {#each paper!.questions as q, n (q.index)}
      {@const v = verdict(q.index)}
      <li class="question {v ?? ''}" data-testid="question-{q.index}">
        <div class="q-head">
          <span class="qn num">{t('modes.test.number', { n: n + 1 })}</span>
          <span class="kind">{t(`modes.test.kind.${q.kind}`)}</span>
          {#if v}
            <span class="verdict {VERDICT[v].tone}"><Icon icon={VERDICT[v].icon} size={14} />{t(`modes.test.verdict.${v}`)}</span>
          {/if}
        </div>
        <div class="prompt">{@html q.prompt_html}</div>
        {#if q.kind === 'multiple_choice'}
          <div class="options" role="radiogroup" aria-label={t('modes.test.choose')}>
            {#each q.options as option, i (i)}
              <OptionTile
                letter={LETTERS[i] ?? String(i + 1)}
                checked={input.choice[q.index] === i}
                result={tileResult(q, input.choice[q.index] === i, option === q.key?.expected)}
                rightLabel={t('modes.test.correct_is')}
                missLabel={t('modes.test.your')}
                disabled={result !== null}
                onclick={() => (input.choice[q.index] = i)}
                testid="option-{q.index}-{i}"
              >{@html option}</OptionTile>
            {/each}
          </div>
        {:else if q.kind === 'true_false'}
          <p class="statement">{t('modes.test.statement', { text: q.statement ?? '' })}</p>
          <div class="options" role="radiogroup" aria-label={t('modes.test.is_right')}>
            {#each truthOptions as o, i (o.value)}
              <OptionTile
                letter={LETTERS[i]}
                label={o.label}
                checked={input.truth[q.index] === o.value}
                result={tileResult(q, input.truth[q.index] === o.value, q.key?.truth === o.value)}
                rightLabel={t('modes.test.correct_is')}
                missLabel={t('modes.test.your')}
                disabled={result !== null}
                onclick={() => (input.truth[q.index] = o.value)}
                testid="option-{q.index}-{i}"
              />
            {/each}
          </div>
        {:else}
          <Input
            bind:value={input.text[q.index]}
            size="lg"
            disabled={result !== null}
            label={t('modes.test.your_answer')}
            hideLabel
            placeholder={t('modes.test.your_answer')}
            spellcheck="false"
            autocomplete="off"
          />
        {/if}
        {#if q.key}
          <p class="key">{t('modes.test.key', { text: keyText(q) })}</p>
        {/if}
      </li>
    {/each}
  </ol>
{/snippet}

{#if paper && !result}
  <SessionFrame
    title={t('modes.test.heading')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('modes.test.progress_label')}
    value={answered}
    max={paper.questions.length}
    count={t('study.position', { n: answered, total: paper.questions.length })}
    exitLabel={t('modes.test.exit_label')}
    exitText={t('modes.test.exit')}
    onexit={requestExit}
    width={760}
  >
    {#snippet meta()}<span>{t('modes.test.answered_short')}</span>{/snippet}
    {#snippet extra()}
      <IconButton size="sm" icon={Printer} label={t('modes.test.print')} onclick={() => window.print()} />
      <Button variant="ghost" size="sm" onclick={() => void ctx.guarded(printKey)}>{t('modes.test.print_key')}</Button>
    {/snippet}
    {@render questions()}
    <div class="submit no-print">
      <span class="answered">{t('modes.test.answered', { n: answered, total: paper.questions.length })}</span>
      <Button variant="primary" size="lg" kbd="Mod+Enter" aria-keyshortcuts="Control+Enter" data-testid="test-submit" onclick={() => void ctx.guarded(submit)}>
        {t('modes.test.submit')}
      </Button>
    </div>
  </SessionFrame>
{:else if paper && result}
  <section class="test" bind:this={top}>
    <div class="no-print">
      <PageHeader chrome={false} title={t('modes.test.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')}>
        {#snippet actions()}
          <Button variant="ghost" icon={Printer} onclick={() => window.print()}>{t('modes.test.print')}</Button>
          <Button variant="ghost" onclick={() => void ctx.guarded(printKey)}>{t('modes.test.print_key')}</Button>
          <Button variant="primary" onclick={() => void ctx.guarded(newTest)}>{t('modes.test.new')}</Button>
        {/snippet}
      </PageHeader>
    </div>
    <div class="score" role="status" data-testid="test-score">
      <ProgressRing
        label={t('modes.test.score_label')}
        value={result.percent}
        size={64}
        stroke={6}
        tone={result.percent >= 80 ? 'success' : result.percent >= 50 ? 'warning' : 'danger'}
      >
        <span class="pct num">{formatPercent100(result.percent)}</span>
      </ProgressRing>
      <div class="score-text">
        <h2>{t('modes.test.score', { correct: result.correct, total: result.items.length })}</h2>
        <p class="tally">
          <span class="t success"><Icon icon={CircleCheck} size={14} />{t('modes.test.n_correct', { n: result.correct })}</span>
          {#if result.almost > 0}<span class="t warning"><Icon icon={TriangleAlert} size={14} />{t('modes.test.n_almost', { n: result.almost })}</span>{/if}
          {#if result.wrong > 0}<span class="t danger"><Icon icon={CircleAlert} size={14} />{t('modes.test.n_wrong', { n: result.wrong })}</span>{/if}
          {#if result.blank > 0}<span class="t neutral"><Icon icon={Minus} size={14} />{t('modes.test.n_blank', { n: result.blank })}</span>{/if}
        </p>
      </div>
    </div>
    {#if misses.length > 0}
      <section class="misses no-print" aria-labelledby="test-misses">
        <h2 id="test-misses">{t('modes.test.misses', { n: misses.length })}</h2>
        <div class="miss-table" role="table" aria-labelledby="test-misses">
          <div class="miss-row head" role="row">
            <span role="columnheader"><span class="sr-only">{t('modes.test.verdict_col')}</span></span>
            <span role="columnheader">{t('modes.test.question_col')}</span>
            <span role="columnheader">{t('modes.test.your')}</span>
            <span role="columnheader">{t('modes.test.correct_is')}</span>
          </div>
          {#each misses as q (q.index)}
            {@const v = verdict(q.index) ?? 'blank'}
            <div class="miss-row tone-{VERDICT[v].tone}" role="row">
              <span role="cell" class="mi"><Icon icon={VERDICT[v].icon} size={16} label={t(`modes.test.verdict.${v}`)} /></span>
              <span role="cell" class="mp">{@html q.prompt_html}</span>
              <span role="cell" class="mg">{given(q)}</span>
              <span role="cell" class="mk">{keyText(q)}</span>
            </div>
          {/each}
        </div>
      </section>
    {/if}
    {@render questions()}
  </section>
{:else}
  <section class="setup-page">
    <PageHeader title={t('modes.test.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="160px" />
    {:else}
      <SetupPanel title={t('modes.test.start_heading')} {notice}>
        {#snippet meta()}<span>{t('modes.test.intro')}</span>{/snippet}
        {#snippet actions()}
          <Button variant="primary" icon={ClipboardCheck} data-testid="study-start" onclick={() => void ctx.guarded(start)}>{t('modes.test.start')}</Button>
        {/snippet}
        {#if moduleOn('browse')}
          <OptRow label={t('study.query_label')} labelFor="test-query">
            <Input
              id="test-query"
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
        <OptRow label={t('modes.test.questions')} labelFor="test-count">
          <Input id="test-count" class="count" size="sm" type="number" min="1" max="200" label={t('modes.test.questions')} hideLabel bind:value={count} />
        </OptRow>
        <OptRow label={t('modes.test.kinds')} description={noKind ? t('modes.test.all_kinds') : undefined}>
          <div class="kind-chips" role="group" aria-label={t('modes.test.kinds')}>
            {#each KINDS as k (k)}
              <Chip label={t(`modes.test.kind.${k}`)} selected={kinds[k]} onclick={() => (kinds[k] = !kinds[k])} />
            {/each}
          </div>
        </OptRow>
      </SetupPanel>
    {/if}
  </section>
{/if}

<Dialog bind:open={confirmOpen} role="alertdialog" size="sm" title={t('modes.test.discard_title')} description={t('modes.test.discard_body', { n: answered })} initialFocus=".keep">
  {#snippet footer()}
    <Button class="keep" onclick={() => (confirmOpen = false)}>{t('modes.test.discard_keep')}</Button>
    <Button variant="danger" onclick={() => { confirmOpen = false; void ctx.guarded(newTest); }}>{t('modes.test.discard')}</Button>
  {/snippet}
</Dialog>

<style>
  .test {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .setup-page {
    display: flex;
    flex-direction: column;
  }
  .setup-page :global(.query) {
    width: 240px;
  }
  .setup-page :global(.count) {
    width: 80px;
  }
  .kind-chips {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  /* Paper. */
  .paper {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    max-width: 760px;
  }
  .question {
    break-inside: avoid;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5) 0;
    border-top: 1px solid var(--border);
  }
  .question:first-child {
    border-top: none;
    padding-top: 0;
  }
  .q-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .qn {
    min-width: 3ch;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .verdict {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-weight: var(--weight-medium);
  }
  .success {
    color: var(--success);
  }
  .warning {
    color: var(--warning);
  }
  .danger {
    color: var(--danger);
  }
  .neutral {
    color: var(--text-2);
  }
  .prompt {
    font-size: var(--text-lg);
    line-height: var(--leading-tight);
  }
  .prompt :global(p) {
    margin: 0;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .statement {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-left: 2px solid var(--border-strong);
    background: var(--surface-1);
    font-size: var(--text-md);
  }
  .key {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .submit {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }
  .answered {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  /* Result. */
  .score {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    max-width: 760px;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .pct {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .score-text h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .tally {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: var(--space-1) 0 0;
    font-size: var(--text-sm);
  }
  .t {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .misses {
    max-width: 760px;
  }
  .misses h2 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .miss-table {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    font-size: var(--text-sm);
  }
  .miss-row {
    display: grid;
    grid-template-columns: 24px minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-3);
    align-items: center;
    min-height: var(--row-h);
    padding: var(--space-1) var(--space-3);
  }
  .miss-row + .miss-row {
    border-top: 1px solid var(--border);
  }
  .miss-row.head {
    min-height: 28px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .mi {
    display: inline-flex;
  }
  .tone-danger .mi {
    color: var(--danger);
  }
  .tone-warning .mi {
    color: var(--warning);
  }
  .tone-neutral .mi {
    color: var(--text-2);
  }
  .mp :global(p) {
    margin: 0;
  }
  .mp,
  .mg,
  .mk {
    overflow-wrap: anywhere;
  }
  .mg {
    color: var(--text-2);
  }
  .mk {
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  @media (max-width: 720px) {
    .options {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media print {
    .no-print {
      display: none !important;
    }
    :global(nav),
    :global(.deck-pick),
    :global(.section-tabs) {
      display: none !important;
    }
  }
</style>
