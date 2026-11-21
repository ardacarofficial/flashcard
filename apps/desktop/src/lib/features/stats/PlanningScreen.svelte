<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, ExamList, JobDto, ModuleDto, OptimizerResult, PresetDto, WorkloadEstimate } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Chip,
    Icon,
    IconButton,
    PageHeader,
    Input,
    Plus,
    SegmentedControl,
    Select,
    Switch,
    Trash2,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import JobProgress from '../io/JobProgress.svelte';
  import { followJob } from '../io/job';
  import { FormRow, LoadState } from '../core/kit';
  import Panel from './Panel.svelte';
  import { num, pct, shortDate, tzOffset } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const DAYS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'] as const;
  const EASY = ['normal', 'reduced', 'minimum'] as const;
  type Easy = (typeof EASY)[number];

  let balance = $state(true);
  let easy = $state<Record<string, Easy>>(Object.fromEntries(DAYS.map((d) => [d, 'normal'])));
  let exams = $state<ExamList | null>(null);
  let examsFailed = $state(false);
  let name = $state('');
  let date = $state('');
  let examDeck = $state('');
  let examQuery = $state('');

  let optimizerOn = $state(false);
  let presets = $state<PresetDto[]>([]);
  let presetId = $state('');
  let job = $state<JobDto | null>(null);
  let outcome = $state<{ text: string; tone: Tone } | null>(null);
  let workloadDeck = $state('');
  let workload = $state<WorkloadEstimate | null>(null);

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const examDeckOptions = $derived([{ value: '', label: t('planning.exams.any_deck') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const workloadDeckOptions = $derived([{ value: '', label: t('stats.scope.all_decks') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);

  // Settings are read best-effort: without them (e.g. the dev mock) the defaults show.
  async function loadSettings() {
    try {
      const keys = ['planning.load_balance', ...DAYS.map((d) => `planning.easy_days.${d}`)];
      const s = await api.call('settings.get', { scope: { kind: 'app' }, keys });
      for (const v of s.values) {
        if (v.key === 'planning.load_balance' && typeof v.value === 'boolean') balance = v.value;
        const day = v.key.replace('planning.easy_days.', '');
        if (day in easy && EASY.includes(v.value as Easy)) easy[day] = v.value as Easy;
      }
    } catch {
      /* keep defaults */
    }
  }

  async function set(key: string, value: unknown) {
    await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key, value }));
  }

  async function loadExams() {
    const ok = await ctx.guarded(async () => {
      exams = await api.call('planning.exams.list', { tz_offset_mins: tzOffset() });
    });
    examsFailed = !ok && !exams;
  }

  async function addExam() {
    const ok = await ctx.guarded(() =>
      api.call('planning.exams.create', {
        name,
        date,
        deck_id: examDeck || null,
        query: examQuery.trim() || null,
      }),
    );
    if (ok) {
      name = '';
      examQuery = '';
    }
    await loadExams();
  }

  async function loadOptimizer() {
    const mods: ModuleDto[] = (await api.call('modules.list', {})).modules;
    optimizerOn = mods.some((m) => m.id === 'optimizer' && m.enabled);
    if (!optimizerOn) return;
    await ctx.guarded(async () => {
      presets = (await api.call('presets.list', {})).items;
      if (!presetId) presetId = presets.at(0)?.id ?? '';
    });
    await loadWorkload();
  }

  async function loadWorkload() {
    await ctx.guarded(async () => {
      workload = await api.call('optimizer.workload', { deck_id: workloadDeck || null, retentions: null });
    });
  }

  async function optimize() {
    outcome = null;
    await ctx.guarded(async () => {
      const started = await api.call('optimizer.run', { preset_id: presetId, tz_offset_mins: tzOffset() });
      job = started.job;
      const done = await followJob(started.job, (j) => (job = j));
      job = null;
      if (done.status === 'succeeded') {
        const r = done.result as OptimizerResult;
        outcome = { text: t('planning.optimizer.done', { n: r.weights.length }), tone: 'success' };
      } else if (done.status === 'cancelled') {
        outcome = { text: t('planning.optimizer.cancelled'), tone: 'neutral' };
      } else {
        outcome = { text: t('planning.optimizer.failed'), tone: 'danger' };
        if (done.error) throw done.error;
      }
    });
    await loadWorkload();
  }

  const when = (n: number) => (n > 0 ? t('planning.exams.days_left', { n }) : n === 0 ? t('planning.exams.today') : t('planning.exams.past'));
  const whenTone = (n: number): Tone => (n < 0 ? 'neutral' : n <= 7 ? 'warning' : 'accent');
  const isCurrent = (r: number) => !!workload && Math.abs(r - workload.current_retention) < 1e-9;
  const maxPerDay = $derived(workload ? Math.max(1, ...workload.points.map((p) => p.reviews_per_day)) : 1);

  onMount(() => {
    void loadSettings();
    void loadExams();
    void loadOptimizer();
    const unsub = api.on(['exams_changed', 'presets_changed', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => {
      void loadExams();
      void loadOptimizer();
    });
    return () => void unsub.then((u) => u());
  });
</script>


<div class="planning">
  <PageHeader title={t('planning.heading')} subtitle={t('planning.subtitle')} />

  <Panel title={t('planning.balance.heading')} body="rows">
    <FormRow label={t('settings.planning.load_balance.label')} description={t('settings.planning.load_balance.description')}>
      <Switch ariaLabel={t('planning.balance.toggle')} bind:checked={balance} onchange={() => void set('planning.load_balance', balance)} />
    </FormRow>
    <FormRow label={t('planning.balance.easy_days')} description={t('settings.planning.easy_days.description')} stack>
      <ul class="days">
        {#each DAYS as d (d)}
          <li>
            <span class="day">{t(`settings.planning.easy_days.${d}.label`)}</span>
            <SegmentedControl
              size="sm"
              label={t(`settings.planning.easy_days.${d}.label`)}
              options={EASY.map((v) => ({ value: v, label: t(`settings.planning.easy_days.${d}.${v}`) }))}
              bind:value={easy[d]}
              onchange={() => void set(`planning.easy_days.${d}`, easy[d])}
            />
          </li>
        {/each}
      </ul>
    </FormRow>
  </Panel>

  <Panel title={t('planning.exams.heading')} description={t('planning.exams.intro')} body="rows">
    {#if examsFailed}
      <div class="pad"><LoadState state="error" retry={() => void loadExams()} /></div>
    {:else if !exams}
      <div class="pad"><LoadState state="loading" lines={2} /></div>
    {:else if exams.items.length === 0}
      <p class="pad empty"><Icon icon={Calendar} size={14} /><strong>{t('planning.exams.none')}</strong> <span>{t('planning.exams.none_hint')}</span></p>
    {:else}
      <ul class="exams">
        {#each exams.items as e (e.exam.id)}
          <li>
            <span class="date num" aria-hidden="true">{shortDate(e.exam.date, i18n.locale)}</span>
            <span class="exam-text">
              <strong>{e.exam.name}</strong>
              <span class="meta">
                <span class="num">{e.exam.date}</span> · {t('planning.exams.cards', { n: e.cards })}
                {#if e.exam.query}· <code>{e.exam.query}</code>{/if}
              </span>
            </span>
            <Chip size="sm" tone={whenTone(e.days_left)} label={when(e.days_left)} />
            <IconButton
              size="sm"
              icon={Trash2}
              label={t('planning.exams.delete_named', { name: e.exam.name })}
              onclick={() => ctx.guarded(() => api.call('planning.exams.delete', { id: e.exam.id }))}
            />
          </li>
        {/each}
      </ul>
    {/if}

    <form
      class="add"
      aria-label={t('planning.exams.add')}
      onsubmit={(ev) => {
        ev.preventDefault();
        void addExam();
      }}
    >
      <Input size="sm" label={t('planning.exams.name')} bind:value={name} />
      <Input size="sm" label={t('planning.exams.date')} type="date" bind:value={date} />
      <Select size="sm" label={t('planning.exams.deck')} options={examDeckOptions} bind:value={examDeck} />
      <Input size="sm" label={t('planning.exams.query')} bind:value={examQuery} placeholder={t('planning.exams.query_hint')} />
      <Button type="submit" size="sm" variant="primary" icon={Plus} disabled={!name.trim() || !date}>{t('planning.exams.add')}</Button>
    </form>
  </Panel>

  {#if optimizerOn}
    <Panel title={t('planning.optimizer.heading')} description={t('planning.optimizer.intro')} body="rows">
      {#if presets.length === 0}
        <FormRow label={t('planning.optimizer.preset')} description={t('planning.optimizer.none')} />
      {:else}
        <FormRow label={t('planning.optimizer.preset')} description={t('planning.optimizer.preset_hint')}>
          <Select size="sm" hideLabel label={t('planning.optimizer.preset')} options={presets.map((p) => ({ value: p.id, label: p.name }))} bind:value={presetId} />
          <Button size="sm" variant="primary" loading={job !== null} disabled={!presetId || job !== null} onclick={() => void optimize()}>{t('planning.optimizer.run')}</Button>
        </FormRow>
      {/if}
      {#if job || outcome}
        <div class="job">
          {#if job}<JobProgress {job} />{/if}
          {#if outcome}<p class="outcome {outcome.tone}" role="status">{outcome.text}</p>{/if}
        </div>
      {/if}
    </Panel>

    <Panel title={t('planning.workload.heading')} description={t('planning.workload.intro')} body="rows">
      {#snippet actions()}
        <Select size="sm" hideLabel label={t('planning.workload.deck')} options={workloadDeckOptions} bind:value={workloadDeck} onchange={() => void loadWorkload()} />
      {/snippet}
      {#if !workload}
        <div class="pad"><LoadState state="loading" /></div>
      {:else}
        <table class="dt">
          <thead>
            <tr>
              <th scope="col">{t('planning.workload.retention')}</th>
              <th scope="col" class="bar-col">{t('planning.workload.per_day')}</th>
              <th scope="col" class="n">{t('planning.workload.relative')}</th>
            </tr>
          </thead>
          <tbody>
            {#each workload.points as p (p.retention)}
              <tr class:current={isCurrent(p.retention)}>
                <th scope="row">
                  <span class="ret num">{pct(p.retention)}{isCurrent(p.retention) ? ` (${t('planning.workload.current')})` : ''}</span>
                </th>
                <td class="bar-col">
                  <span class="inline-bar">
                    <span class="track" aria-hidden="true"><span class="fill" style:width="{(p.reviews_per_day / maxPerDay) * 100}%"></span></span>
                    <span class="num v">{num(p.reviews_per_day)}</span>
                  </span>
                </td>
                <td class="n num">×{num(p.relative, 2)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="basis">{t('planning.workload.basis', { n: workload.review_cards, new: workload.new_per_day })}</p>
      {/if}
    </Panel>
  {/if}
</div>

<style>
  .planning {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 880px;
  }
  .planning > :global(.fc-page-header) {
    margin-bottom: var(--space-1);
  }
  .pad {
    padding: var(--space-3) var(--space-4);
  }
  .empty {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .empty strong {
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .days {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(23rem, 1fr));
    gap: var(--space-1) var(--space-6);
  }
  .days li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 32px;
  }
  .day {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .exams {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .exams li {
    display: grid;
    grid-template-columns: 4.5rem minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .date {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--accent-text);
  }
  .exam-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    font-size: var(--text-sm);
  }
  .exam-text strong {
    font-weight: var(--weight-medium);
  }
  .meta {
    font-size: var(--text-xs);
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .add {
    display: grid;
    grid-template-columns: minmax(8rem, 1.2fr) 9.5rem minmax(8rem, 1fr) minmax(8rem, 1fr) auto;
    align-items: end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-0);
    border-radius: 0 0 var(--radius-card) var(--radius-card);
  }
  .job {
    padding: var(--space-2) var(--space-4) var(--space-3);
  }
  .outcome {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
  }
  .outcome.success {
    color: var(--success);
  }
  .outcome.danger {
    color: var(--danger);
  }
  .basis {
    margin: 0;
    padding: var(--space-2) var(--space-4);
    color: var(--text-2);
    font-size: var(--text-xs);
    border-top: 1px solid var(--border);
  }
  .dt {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  .dt th,
  .dt td {
    height: 32px;
    padding: 0 var(--space-3);
    text-align: start;
    border-bottom: 1px solid var(--border);
  }
  .dt th:first-child {
    padding-inline-start: var(--space-4);
  }
  .dt td:last-child,
  .dt th:last-child {
    padding-inline-end: var(--space-4);
  }
  .dt thead th {
    height: 28px;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .dt tbody th {
    font-weight: var(--weight-regular);
    white-space: nowrap;
  }
  .dt tbody tr:last-child > * {
    border-bottom: none;
  }
  tr.current th,
  tr.current td {
    font-weight: var(--weight-semibold);
    background: var(--accent-soft);
  }
  tr.current .ret {
    color: var(--accent-text);
  }
  .n {
    text-align: end !important;
  }
  .bar-col {
    width: 55%;
  }
  .inline-bar {
    display: grid;
    grid-template-columns: minmax(3rem, 1fr) 3.5rem;
    align-items: center;
    gap: var(--space-2);
  }
  .track {
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--surface-3);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: var(--radius-pill);
    background: light-dark(color-mix(in srgb, var(--accent) 40%, var(--accent-text)), color-mix(in srgb, var(--accent) 78%, var(--surface-1)));
  }
  tr.current .fill {
    background: light-dark(var(--accent-text), var(--accent));
  }
  .v {
    text-align: end;
  }
  .add > :global(.fc-btn) {
    justify-self: start;
  }
  @media (max-width: 1023.98px) {
    .add {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
