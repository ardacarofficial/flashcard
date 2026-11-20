<script lang="ts">
  /**
   * Practice tests (docs/02 §6.1): the backend builds a paper of mixed question types from
   * the chosen cards (deck, query such as `exam:TYT`), grades it on submission and never
   * changes the schedule. The paper prints (with or without the answer key).
   */
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { TestPaperDto, TestQuestionKind } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { moduleOn, trackModules } from '../cards/modules.svelte';
  import { tzOffsetMins } from '../study/format';
  import type { ScreenCtx } from '../types';
  import { emptyInput, testAnswers, type PaperInput } from './modes';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: TestQuestionKind[] = ['true_false', 'multiple_choice', 'written'];

  let paper = $state<TestPaperDto | null>(null);
  let input = $state<PaperInput>(emptyInput());
  let query = $state('');
  let count = $state(20);
  let kinds = $state<Record<TestQuestionKind, boolean>>({ true_false: true, multiple_choice: true, written: true });
  let notice = $state<string | null>(null);
  let startedAt = 0;

  onMount(() => {
    trackModules();
    void api.call('settings.get', { scope: { kind: 'app' }, keys: ['practice_test.questions'] }).then(
      (r) => {
        const v = r.values.find((x) => x.key === 'practice_test.questions')?.value;
        if (typeof v === 'number') count = v;
      },
      () => {},
    );
  });

  const result = $derived(paper?.result ?? null);
  const verdict = (i: number) => result?.items.find((x) => x.index === i)?.verdict ?? null;

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
      modes: { timed: null, rating_scale: null, test: { questions: count, kinds: chosen.length ? chosen : null } },
    });
    if ((s.modes?.test?.questions ?? 0) === 0) {
      await api.call('study.end', { session_id: s.id });
      notice = t('modes.test.nothing');
      return;
    }
    input = emptyInput();
    paper = await api.call('modes.test.paper', { session_id: s.id, with_key: null });
    startedAt = Date.now();
  }

  async function submit() {
    if (!paper) return;
    paper = await api.call('modes.test.submit', {
      session_id: paper.session_id,
      answers: testAnswers(paper.questions, input),
      duration_ms: Math.max(0, Date.now() - startedAt),
    });
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
</script>

<section class="test">
  <h2 class="no-print">{t('modes.test.heading')}</h2>
  {#if paper}
    <div class="row no-print">
      <button onclick={() => window.print()}>{t('modes.test.print')}</button>
      <button onclick={() => void ctx.guarded(printKey)}>{t('modes.test.print_key')}</button>
      <button onclick={() => void ctx.guarded(newTest)}>{t('modes.test.new')}</button>
    </div>
    {#if result}
      <div class="score" role="status" data-testid="test-score">
        <strong>{t('modes.test.score', { percent: result.percent })}</strong>
        <span class="muted">{t('modes.test.breakdown', { correct: result.correct, almost: result.almost, wrong: result.wrong, blank: result.blank })}</span>
      </div>
    {/if}
    <ol class="paper">
      {#each paper.questions as q (q.index)}
        <li class="question {verdict(q.index) ?? ''}" data-testid="question-{q.index}">
          <div class="prompt">{@html q.prompt_html}</div>
          {#if q.kind === 'multiple_choice'}
            <div class="options">
              {#each q.options as o, i (i)}
                <label>
                  <input
                    type="radio"
                    name="q{q.index}"
                    disabled={result !== null}
                    checked={input.choice[q.index] === i}
                    onchange={() => (input.choice[q.index] = i)}
                  />
                  {o}
                </label>
              {/each}
            </div>
          {:else if q.kind === 'true_false'}
            <p>{t('modes.test.statement', { text: q.statement ?? '' })}</p>
            <div class="options">
              {#each [true, false] as b (b)}
                <label>
                  <input
                    type="radio"
                    name="q{q.index}"
                    disabled={result !== null}
                    checked={input.truth[q.index] === b}
                    onchange={() => (input.truth[q.index] = b)}
                  />
                  {b ? t('modes.test.true') : t('modes.test.false')}
                </label>
              {/each}
            </div>
          {:else}
            <input
              class="written"
              bind:value={input.text[q.index]}
              disabled={result !== null}
              placeholder={t('modes.test.your_answer')}
              aria-label={t('modes.test.your_answer')}
            />
          {/if}
          {#if q.key}
            <p class="key">
              {#if verdict(q.index)}<strong>{t(`modes.test.verdict.${verdict(q.index)!}`)}</strong> ·{/if}
              {t('modes.test.key', {
                text: q.kind === 'true_false' ? (q.key.truth ? t('modes.test.true') : t('modes.test.false')) : q.key.expected,
              })}
            </p>
          {/if}
        </li>
      {/each}
    </ol>
    {#if !result}
      <button class="primary no-print" onclick={() => void ctx.guarded(submit)}>{t('modes.test.submit')}</button>
    {/if}
  {:else}
    <div class="panel">
      <p class="muted">{t('modes.test.intro')}</p>
      <p class="muted">{ctx.selectedPath ?? t('study.whole_collection')}</p>
      {#if moduleOn('browse')}
        <label>
          {t('study.query_label')}
          <input bind:value={query} placeholder={t('study.query_placeholder')} spellcheck="false" />
        </label>
      {/if}
      <label>
        {t('modes.test.questions')}
        <input type="number" min="1" max="200" bind:value={count} />
      </label>
      <fieldset class="row">
        <legend>{t('modes.test.kinds')}</legend>
        {#each KINDS as k (k)}
          <label><input type="checkbox" bind:checked={kinds[k]} /> {t(`modes.test.kind.${k}`)}</label>
        {/each}
      </fieldset>
      <button class="primary" onclick={() => void ctx.guarded(start)}>{t('modes.test.start')}</button>
      {#if notice}<p class="muted" role="status">{notice}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .test {
    max-width: 760px;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .paper {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-4);
    padding-left: var(--fc-space-5);
  }
  .question {
    break-inside: avoid;
  }
  .question.correct {
    border-left: 4px solid var(--fc-success);
    padding-left: var(--fc-space-2);
  }
  .question.wrong,
  .question.blank,
  .question.almost {
    border-left: 4px solid var(--fc-danger);
    padding-left: var(--fc-space-2);
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .written {
    width: 100%;
  }
  .key {
    color: var(--fc-text-muted);
  }
  input[type='number'] {
    width: 5em;
  }
  @media print {
    .no-print {
      display: none !important;
    }
    :global(nav),
    :global(.deck-pick) {
      display: none !important;
    }
  }
</style>
