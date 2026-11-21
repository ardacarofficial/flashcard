<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, ExamList, JobDto, ModuleDto, OptimizerResult, PresetDto, WorkloadEstimate } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import {
    Brain,
    Button,
    Calendar,
    CalendarDays,
    Chip,
    EmptyState,
    Gauge,
    IconButton,
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
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
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

<SettingsPage title={t('planning.heading')} subtitle={t('planning.subtitle')}>
  <SettingsSection title={t('planning.balance.heading')} icon={CalendarDays} body="rows">
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
  </SettingsSection>

  <SettingsSection title={t('planning.exams.heading')} icon={Calendar} description={t('planning.exams.intro')}>
    {#if examsFailed}
      <LoadState state="error" retry={() => void loadExams()} />
    {:else if !exams}
      <LoadState state="loading" lines={2} />
    {:else if exams.items.length === 0}
      <EmptyState size="sm" icon={Calendar} title={t('planning.exams.none')} description={t('planning.exams.none_hint')} />
    {:else}
      <ul class="exams">
        {#each exams.items as e (e.exam.id)}
          <li>
            <div class="date" aria-hidden="true">
              <span class="d">{shortDate(e.exam.date, i18n.locale)}</span>
            </div>
            <div class="exam-text">
              <strong>{e.exam.name}</strong>
              <span class="meta">
                {e.exam.date} · {t('planning.exams.cards', { n: e.cards })}
                {#if e.exam.query}· <code>{e.exam.query}</code>{/if}
              </span>
            </div>
            <Chip size="sm" tone={whenTone(e.days_left)} label={when(e.days_left)} />
            <IconButton
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
      <h3 class="sub">{t('planning.exams.new')}</h3>
      <div class="grid">
        <Input label={t('planning.exams.name')} bind:value={name} />
        <Input label={t('planning.exams.date')} type="date" bind:value={date} />
        <Select label={t('planning.exams.deck')} options={examDeckOptions} bind:value={examDeck} />
        <Input label={t('planning.exams.query')} bind:value={examQuery} placeholder={t('planning.exams.query_hint')} />
      </div>
      <div class="end">
        <Button type="submit" variant="primary" icon={Plus} disabled={!name.trim() || !date}>{t('planning.exams.add')}</Button>
      </div>
    </form>
  </SettingsSection>

  {#if optimizerOn}
    <SettingsSection title={t('planning.optimizer.heading')} icon={Brain} description={t('planning.optimizer.intro')} body="rows">
      {#if presets.length === 0}
        <FormRow label={t('planning.optimizer.preset')} description={t('planning.optimizer.none')} />
      {:else}
        <FormRow label={t('planning.optimizer.preset')} description={t('planning.optimizer.preset_hint')}>
          <Select hideLabel label={t('planning.optimizer.preset')} options={presets.map((p) => ({ value: p.id, label: p.name }))} bind:value={presetId} />
          <Button variant="primary" loading={job !== null} disabled={!presetId || job !== null} onclick={() => void optimize()}>{t('planning.optimizer.run')}</Button>
        </FormRow>
      {/if}
      {#if job || outcome}
        <div class="job">
          {#if job}<JobProgress {job} />{/if}
          {#if outcome}<p class="outcome {outcome.tone}" role="status">{outcome.text}</p>{/if}
        </div>
      {/if}
    </SettingsSection>

    <SettingsSection title={t('planning.workload.heading')} icon={Gauge} description={t('planning.workload.intro')}>
      {#snippet actions()}
        <Select size="sm" hideLabel label={t('planning.workload.deck')} options={workloadDeckOptions} bind:value={workloadDeck} onchange={() => void loadWorkload()} />
      {/snippet}
      {#if !workload}
        <LoadState state="loading" />
      {:else}
        <table>
          <thead>
            <tr>
              <th scope="col">{t('planning.workload.retention')}</th>
              <th scope="col" class="bar-col">{t('planning.workload.per_day')}</th>
              <th scope="col" class="num">{t('planning.workload.relative')}</th>
            </tr>
          </thead>
          <tbody>
            {#each workload.points as p (p.retention)}
              <tr class:current={isCurrent(p.retention)}>
                <th scope="row">
                  <span class="ret">{pct(p.retention)}{isCurrent(p.retention) ? ` (${t('planning.workload.current')})` : ''}</span>
                </th>
                <td class="bar-col">
                  <div class="inline-bar">
                    <span class="track" aria-hidden="true"><span class="fill" style:width="{(p.reviews_per_day / maxPerDay) * 100}%"></span></span>
                    <span class="v">{num(p.reviews_per_day)}</span>
                  </div>
                </td>
                <td class="num">×{num(p.relative, 2)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="muted">{t('planning.workload.basis', { n: workload.review_cards, new: workload.new_per_day })}</p>
      {/if}
    </SettingsSection>
  {/if}
</SettingsPage>

<style>
  .days {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .days li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .day {
    font-size: var(--text-sm);
    min-width: 7rem;
  }
  .exams {
    list-style: none;
    margin: 0 0 var(--space-4);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .exams li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .date {
    flex: none;
    width: 56px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--text-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    line-height: 1.2;
  }
  .exam-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .meta {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .add {
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }
  .sub {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3) var(--space-4);
  }
  .end {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-4);
  }
  .job {
    padding: var(--space-2) var(--space-6) var(--space-4);
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
  .muted {
    margin: var(--space-3) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  th,
  td {
    text-align: start;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  th:first-child {
    padding-inline-start: 0;
  }
  td:last-child,
  th:last-child {
    padding-inline-end: 0;
  }
  thead th {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  tbody th {
    font-weight: var(--weight-regular);
    white-space: nowrap;
  }
  tr.current th,
  tr.current td {
    font-weight: var(--weight-semibold);
  }
  tr.current .ret {
    color: var(--accent-text);
  }
  .num {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
  .bar-col {
    width: 50%;
  }
  .inline-bar {
    display: grid;
    grid-template-columns: minmax(3rem, 1fr) 3.5rem;
    align-items: center;
    gap: var(--space-2);
  }
  .track {
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--accent) 70%, var(--surface-1));
  }
  tr.current .fill {
    background: var(--accent);
  }
  .v {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 639.98px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
