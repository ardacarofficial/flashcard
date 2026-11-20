<script lang="ts">
  /**
   * Practice tests (docs/02 §6.1): the backend builds a paper of mixed question types from
   * the chosen cards (deck, query such as `exam:TYT`), grades it on submission and never
   * changes the schedule. The paper prints (with or without the answer key). After grading: a
   * score ring and the list of wrong answers.
   */
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { TestAnswerDto, TestPaperDto, TestQuestionDto, TestQuestionKind, TestVerdict } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Card,
    Chip,
    CircleAlert,
    CircleCheck,
    ClipboardCheck,
    FileText,
    Icon,
    Input,
    Minus,
    PageHeader,
    ProgressRing,
    RadioGroup,
    Skeleton,
    TriangleAlert,
    type Tone,
  } from '$lib/ui';
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
    top?.scrollIntoView?.({ block: 'start' });
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

  const truthText = (b: boolean | null | undefined) => (b ? t('modes.test.true') : t('modes.test.false'));
  const keyText = (q: TestQuestionDto) => (q.kind === 'true_false' ? truthText(q.key?.truth) : (q.key?.expected ?? ''));
  const given = (q: TestQuestionDto): string => {
    const a: TestAnswerDto | undefined = paper?.answers?.find((x) => x.index === q.index);
    if (!a) return t('modes.test.blank_answer');
    if (q.kind === 'multiple_choice' && a.choice != null) return q.options[a.choice] ?? '';
    if (q.kind === 'true_false' && a.truth != null) return truthText(a.truth);
    return a.text ?? t('modes.test.blank_answer');
  };
  const choiceOptions = (q: TestQuestionDto) => q.options.map((o, i) => ({ value: i, label: o }));
  const truthOptions = $derived([
    { value: true, label: t('modes.test.true') },
    { value: false, label: t('modes.test.false') },
  ]);
</script>

<section class="test" bind:this={top}>
  {#if paper}
    <div class="no-print">
      <PageHeader title={t('modes.test.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')}>
        {#snippet actions()}
          <Button variant="ghost" icon={FileText} onclick={() => window.print()}>{t('modes.test.print')}</Button>
          <Button variant="ghost" onclick={() => void ctx.guarded(printKey)}>{t('modes.test.print_key')}</Button>
          <Button onclick={() => void ctx.guarded(newTest)}>{t('modes.test.new')}</Button>
        {/snippet}
      </PageHeader>
    </div>
    {#if result}
      <Card padding="lg" elevated>
        <div class="score" role="status" data-testid="test-score">
          <ProgressRing
            label={t('modes.test.score_label')}
            value={result.percent}
            size={120}
            tone={result.percent >= 80 ? 'success' : result.percent >= 50 ? 'warning' : 'danger'}
          >
            <span class="pct">{result.percent}%</span>
          </ProgressRing>
          <div class="score-text">
            <h2>{t('modes.test.score', { percent: result.percent })}</h2>
            <div class="chips">
              <Chip size="sm" tone="success" icon={CircleCheck} label={t('modes.test.n_correct', { n: result.correct })} />
              {#if result.almost > 0}<Chip size="sm" tone="warning" icon={TriangleAlert} label={t('modes.test.n_almost', { n: result.almost })} />{/if}
              {#if result.wrong > 0}<Chip size="sm" tone="danger" icon={CircleAlert} label={t('modes.test.n_wrong', { n: result.wrong })} />{/if}
              {#if result.blank > 0}<Chip size="sm" icon={Minus} label={t('modes.test.n_blank', { n: result.blank })} />{/if}
            </div>
          </div>
        </div>
      </Card>
      {#if misses.length > 0}
        <section class="misses no-print" aria-labelledby="test-misses">
          <h2 id="test-misses">{t('modes.test.misses', { n: misses.length })}</h2>
          <ul>
            {#each misses as q (q.index)}
              {@const v = verdict(q.index) ?? 'blank'}
              <li class="miss {VERDICT[v].tone}">
                <Icon icon={VERDICT[v].icon} size={20} label={t(`modes.test.verdict.${v}`)} />
                <div class="miss-body">
                  <div class="miss-prompt">{@html q.prompt_html}</div>
                  <div class="miss-row"><span class="k">{t('modes.test.your')}</span><span>{given(q)}</span></div>
                  <div class="miss-row"><span class="k">{t('modes.test.correct_is')}</span><strong>{keyText(q)}</strong></div>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {:else}
      <p class="answered no-print">{t('modes.test.answered', { n: answered, total: paper.questions.length })}</p>
    {/if}
    <ol class="paper">
      {#each paper.questions as q, n (q.index)}
        {@const v = verdict(q.index)}
        <li class="question {v ?? ''}" data-testid="question-{q.index}">
          <Card padding="lg">
            <div class="q-head">
              <span class="num">{t('modes.test.number', { n: n + 1 })}</span>
              <Chip size="sm" label={t(`modes.test.kind.${q.kind}`)} />
              {#if v}<Chip size="sm" tone={VERDICT[v].tone} icon={VERDICT[v].icon} label={t(`modes.test.verdict.${v}`)} />{/if}
            </div>
            <div class="prompt">{@html q.prompt_html}</div>
            {#if q.kind === 'multiple_choice'}
              <RadioGroup
                label={t('modes.test.choose')}
                hideLabel
                name="q{q.index}"
                options={choiceOptions(q)}
                disabled={result !== null}
                bind:value={input.choice[q.index]}
              />
            {:else if q.kind === 'true_false'}
              <p class="statement">{t('modes.test.statement', { text: q.statement ?? '' })}</p>
              <RadioGroup
                label={t('modes.test.is_right')}
                hideLabel
                name="q{q.index}"
                options={truthOptions}
                disabled={result !== null}
                bind:value={input.truth[q.index]}
              />
            {:else}
              <Input
                bind:value={input.text[q.index]}
                disabled={result !== null}
                label={t('modes.test.your_answer')}
                hideLabel
                placeholder={t('modes.test.your_answer')}
              />
            {/if}
            {#if q.key}
              <p class="key">{t('modes.test.key', { text: keyText(q) })}</p>
            {/if}
          </Card>
        </li>
      {/each}
    </ol>
    {#if !result}
      <div class="submit no-print">
        <Button variant="primary" size="lg" onclick={() => void ctx.guarded(submit)}>{t('modes.test.submit')}</Button>
      </div>
    {/if}
  {:else}
    <PageHeader title={t('modes.test.heading')} subtitle={ctx.selectedPath ?? t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="240px" />
    {:else}
      <Card padding="lg" class="start-card">
        <div class="setup-head">
          <span class="badge"><Icon icon={ClipboardCheck} size={24} /></span>
          <p class="intro">{t('modes.test.intro')}</p>
        </div>
        <div class="form">
          {#if moduleOn('browse')}
            <Input label={t('study.query_label')} bind:value={query} placeholder={t('study.query_placeholder')} spellcheck="false" />
          {/if}
          <Input class="count" type="number" min="1" max="200" label={t('modes.test.questions')} bind:value={count} />
          <fieldset class="kinds">
            <legend>{t('modes.test.kinds')}</legend>
            <div class="kind-chips">
              {#each KINDS as k (k)}
                <Chip label={t(`modes.test.kind.${k}`)} selected={kinds[k]} onclick={() => (kinds[k] = !kinds[k])} />
              {/each}
            </div>
            {#if noKind}<p class="hint">{t('modes.test.all_kinds')}</p>{/if}
          </fieldset>
        </div>
        {#if notice}<p class="notice" role="status">{notice}</p>{/if}
        <div class="actions">
          <Button variant="primary" size="lg" data-testid="study-start" onclick={() => void ctx.guarded(start)}>{t('modes.test.start')}</Button>
        </div>
      </Card>
    {/if}
  {/if}
</section>

<style>
  .test {
    max-width: 800px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  :global(.start-card) {
    max-width: 720px;
  }
  .setup-head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .badge {
    flex: none;
    display: inline-flex;
    padding: var(--space-3);
    border-radius: var(--radius-card);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .intro {
    margin: 0;
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
  .form :global(.count) {
    width: 160px;
  }
  .kinds {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: none;
  }
  .kind-chips {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  legend {
    margin-bottom: var(--space-2);
    padding: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .notice {
    margin: var(--space-4) 0 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .actions,
  .submit {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-6);
  }
  .answered {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .score {
    display: flex;
    align-items: center;
    gap: var(--space-6);
  }
  .pct {
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
  }
  .score-text h2 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .chips {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .misses h2 {
    margin: var(--space-2) 0 var(--space-3);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .misses ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .miss {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .miss + .miss {
    border-top: 1px solid var(--border);
  }
  .miss.danger > :global(svg) {
    color: var(--danger);
  }
  .miss.warning > :global(svg) {
    color: var(--warning);
  }
  .miss.neutral > :global(svg) {
    color: var(--text-2);
  }
  .miss-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .miss-prompt {
    font-weight: var(--weight-medium);
  }
  .miss-prompt :global(p) {
    margin: 0;
  }
  .miss-row {
    display: flex;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .k {
    min-width: 9ch;
    color: var(--text-2);
  }
  .paper {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .question {
    break-inside: avoid;
  }
  .q-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .num {
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    min-width: 2.5ch;
  }
  .prompt {
    margin: var(--space-3) 0 var(--space-4);
    font-size: var(--text-lg);
  }
  .prompt :global(p) {
    margin: 0;
  }
  .statement {
    margin: 0 0 var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-md);
  }
  .key {
    margin: var(--space-3) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
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
