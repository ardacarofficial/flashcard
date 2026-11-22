<!--
  Statistics → Planning (v1.3): leads with two controls and their live consequence — "how many
  new cards a day?" and an optional exam date — the daily workload forecast for the next weeks
  (`planning.forecast`, a Rust model of the cards' memory) in one sentence, a chart and a week
  table, and an "Apply" that says which preset and decks it changes (`planning.apply_pace`,
  one undo step). Below: exam dates and load balancing, explained; the optimizer and the
  retention/workload table under "Advanced".
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, ExamList, JobDto, ModuleDto, OptimizerResult, PresetDto, WorkloadEstimate, WorkloadForecast } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Check,
    ChevronDown,
    ChevronRight,
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
    X,
    type Tone,
  } from '$lib/ui';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import BarChart from './BarChart.svelte';
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
  let advanced = $state(false);

  // v1.3 pace planner.
  let paceDeck = $state('');
  let pace = $state<number | null>(null);
  let examDate = $state('');
  let fc = $state<WorkloadForecast | null>(null);
  let fcFailed = $state(false);
  let applying = $state(false);
  let applied = $state(false);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const examDeckOptions = $derived([{ value: '', label: t('planning.exams.any_deck') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const workloadDeckOptions = $derived([{ value: '', label: t('stats.scope.all_decks') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const paceDeckOptions = $derived([{ value: '', label: t('planning.pace.all_decks') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const QUICK = [5, 10, 20, 30] as const;

  /** Fetches the forecast for the controls (the latest call wins). */
  async function loadForecast() {
    const mine = ++seq;
    const n = pace === null || Number.isNaN(Number(pace)) ? null : Math.max(0, Math.min(9999, Math.round(Number(pace))));
    try {
      const r = await api.call('planning.forecast', {
        deck_id: paceDeck || null,
        new_per_day: n,
        exam_date: examDate || null,
        days: 28,
        tz_offset_mins: tzOffset(),
      });
      if (mine !== seq) return;
      fc = r;
      fcFailed = false;
      if (pace === null) pace = r.new_per_day;
    } catch {
      if (mine === seq) fcFailed = !fc;
    }
  }

  /** Controls changed: refresh shortly (typing in the number field stays smooth). */
  function changed() {
    applied = false;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void loadForecast(), 150);
  }

  function pickDeck() {
    pace = null;
    changed();
  }

  async function applyPace() {
    if (!fc || !paceDeck) return;
    applying = true;
    const ok = await ctx.guarded(() =>
      api.call('planning.apply_pace', {
        deck_id: paceDeck,
        new_per_day: fc!.new_per_day,
        preset_name: null,
        exam_date: examDate || null,
        exam_name: null,
      }),
    );
    applying = false;
    applied = ok;
    await Promise.all([loadForecast(), loadExams()]);
  }

  const r0 = (x: number) => num(x, 0);
  const weeksAhead = $derived(fc ? Math.max(1, Math.round((fc.outlook.offset + 1) / 7)) : 0);
  const peakIndex = $derived(fc ? fc.days.findIndex((d) => d.offset === fc!.peak.offset && fc!.peak.cards > 0) : -1);
  const canApply = $derived(!!fc && !!paceDeck && moduleKnownOn('presets') && (fc.new_per_day !== fc.current_new_per_day || !!examDate));
  const exam = $derived(fc?.exam ?? null);

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
    trackModules();
    paceDeck = ctx.selectedDeck ?? ctx.decks[0]?.deck.id ?? '';
    void loadForecast();
    void loadSettings();
    void loadExams();
    void loadOptimizer();
    const unsub = api.on(['exams_changed', 'presets_changed', 'study_', 'deck_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => {
      void loadExams();
      void loadOptimizer();
      void loadForecast();
    });
    return () => {
      if (timer) clearTimeout(timer);
      void unsub.then((u) => u());
    };
  });
</script>


<div class="planning">
  <PageHeader title={t('planning.heading')} subtitle={t('planning.subtitle')} />

  <Panel class="pace" testid="planning-pace" title={t('planning.pace.heading')} description={t('planning.pace.intro')}>
    <div class="controls">
      <Select size="sm" label={t('planning.pace.deck')} options={paceDeckOptions} bind:value={paceDeck} onchange={pickDeck} />
      <div class="pace-field">
        <Input
          size="sm"
          type="number"
          min="0"
          max="9999"
          label={t('planning.pace.new_per_day')}
          bind:value={pace}
          oninput={changed}
          data-testid="pace-input"
        />
        <div class="quick" role="group" aria-label={t('planning.pace.quick')}>
          {#each QUICK as q (q)}
            <Chip
              size="sm"
              label={String(q)}
              selected={Number(pace) === q}
              onclick={() => {
                pace = q;
                changed();
              }}
            />
          {/each}
          {#if fc}<span class="cur num">{t('planning.pace.current', { n: fc.current_new_per_day })}</span>{/if}
        </div>
      </div>
      <div class="exam-field">
        <Input size="sm" type="date" label={t('planning.pace.exam')} bind:value={examDate} oninput={changed} onchange={changed} />
        {#if examDate}
          <IconButton
            size="sm"
            icon={X}
            label={t('planning.pace.clear_exam')}
            onclick={() => {
              examDate = '';
              changed();
            }}
          />
        {/if}
      </div>
    </div>

    {#if fcFailed && !fc}
      <LoadState state="error" retry={() => void loadForecast()} />
    {:else if !fc}
      <LoadState state="loading" lines={3} />
    {:else}
      <p class="sentence" data-testid="pace-sentence" aria-live="polite">
        {fc.outlook.offset >= 6
          ? t('planning.pace.sentence', { weeks: weeksAhead, cards: r0(fc.outlook.cards_per_day), min: r0(fc.outlook.minutes_per_day) })
          : t('planning.pace.sentence_days', { days: fc.outlook.offset, cards: r0(fc.outlook.cards_per_day), min: r0(fc.outlook.minutes_per_day) })}
      </p>
      <ul class="facts">
        {#if fc.days[0]}<li>{t('planning.pace.today', { cards: r0(fc.days[0].cards), min: r0(fc.days[0].minutes) })}</li>{/if}
        {#if fc.peak.cards > 0}<li>{t('planning.pace.peak', { date: shortDate(fc.peak.date, i18n.locale), cards: r0(fc.peak.cards) })}</li>{/if}
        <li>
          {#if fc.unseen === 0}
            {t('planning.pace.new_none')}
          {:else if fc.new_per_day === 0}
            {t('planning.pace.new_zero', { n: fc.unseen })}
          {:else if fc.new_finished_on}
            {t('planning.pace.new_done', { n: fc.unseen, date: shortDate(fc.new_finished_on, i18n.locale) })}
          {:else}
            {t('planning.pace.new_later', { n: fc.unseen })}
          {/if}
        </li>
        {#if exam}
          {#if exam.days_left < 0}
            <li class="warn">{t('planning.pace.exam_past')}</li>
          {:else}
            <li class:warn={exam.new_left > 0}>
              {exam.new_left > 0 ? t('planning.pace.exam_left', { days: exam.days_left, left: exam.new_left }) : t('planning.pace.exam_ok', { days: exam.days_left })}
              {#if exam.new_left > 0 && exam.new_per_day_needed !== null}
                {t('planning.pace.exam_need', { n: exam.new_per_day_needed })}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    pace = exam!.new_per_day_needed;
                    changed();
                  }}>{t('planning.pace.exam_use', { n: exam.new_per_day_needed })}</Button
                >
              {/if}
            </li>
            <li>{t('planning.pace.exam_week', { cards: r0(exam.last_week.cards_per_day), min: r0(exam.last_week.minutes_per_day) })}</li>
          {/if}
        {/if}
      </ul>
      <BarChart
        label={t('planning.pace.chart')}
        highlight={0}
        peak={peakIndex >= 0 ? peakIndex : null}
        peakLabel={t('stats.ahead.peak_mark')}
        height={120}
        formatMax={(n) => r0(n)}
        bars={fc.days.map((d) => ({
          label: shortDate(d.date, i18n.locale),
          value: d.cards,
          title: t('planning.pace.chart_day', { date: shortDate(d.date, i18n.locale), cards: r0(d.cards), min: r0(d.minutes) }),
        }))}
      />
      <table class="dt weeks">
        <caption class="cap">{t('planning.pace.weeks')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('planning.pace.col_week')}</th>
            <th scope="col" class="n">{t('planning.pace.col_cards')}</th>
            <th scope="col" class="n">{t('planning.pace.col_minutes')}</th>
            <th scope="col" class="n">{t('planning.pace.col_new')}</th>
          </tr>
        </thead>
        <tbody>
          {#each fc.weeks as w (w.week)}
            <tr>
              <th scope="row">{t('planning.pace.week_n', { n: w.week })} <span class="range num">{shortDate(w.start_date, i18n.locale)}–{shortDate(w.end_date, i18n.locale)}</span></th>
              <td class="n num">~{r0(w.cards_per_day)}</td>
              <td class="n num">~{r0(w.minutes_per_day)}</td>
              <td class="n num">{r0(w.new_cards)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="basis">
        {t('planning.pace.meaning')}
        {fc.measured_time ? t('planning.pace.time_measured', { s: num(fc.secs_per_answer, 1) }) : t('planning.pace.time_default', { s: num(fc.secs_per_answer, 0) })}
      </p>
      <div class="apply" data-testid="pace-apply">
        <div class="apply-text">
          {#if !paceDeck}
            <p>{t('planning.pace.pick_deck')}</p>
          {:else if !moduleKnownOn('presets')}
            <p>{t('planning.pace.presets_off')}</p>
          {:else if fc.target}
            <p>
              {fc.target.preset_id
                ? t('planning.pace.apply_preset', { preset: fc.target.preset_name ?? '', n: fc.new_per_day, current: fc.target.current_new_per_day })
                : t('planning.pace.apply_create', { deck: fc.target.deck.path.split('::').at(-1) ?? '', n: fc.new_per_day })}
              {#if examDate && exam && exam.days_left >= 0}{t('planning.pace.apply_exam', { date: shortDate(examDate, i18n.locale) })}{/if}
            </p>
            <p class="muted">{t('planning.pace.apply_decks', { decks: fc.target.decks.map((d) => d.path).join(', ') })}</p>
          {/if}
          {#if applied}<p class="ok" role="status"><Icon icon={Check} size={14} />{t('planning.pace.applied')}</p>{/if}
        </div>
        <Button variant="primary" size="sm" loading={applying} disabled={!canApply || applying} onclick={() => void applyPace()}>{t('planning.pace.apply')}</Button>
      </div>
    {/if}
  </Panel>

  <Panel title={t('planning.balance.heading')} description={t('planning.balance.meaning')} body="rows">
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

  <div class="advanced">
    <Button
      variant="ghost"
      size="sm"
      icon={advanced ? ChevronDown : ChevronRight}
      aria-expanded={advanced}
      aria-controls="planning-advanced"
      onclick={() => (advanced = !advanced)}>{advanced ? t('planning.advanced.hide') : t('planning.advanced.show')}</Button
    >
    <span class="muted">{t('planning.advanced.intro')}</span>
  </div>

  {#if optimizerOn && advanced}
    <div id="planning-advanced" class="adv-body">
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
    </div>
  {/if}
</div>

<style>
  .planning {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  /* The header (title + Statistics tabs) spans the page like on Overview; the forms stay 880. */
  .planning > :global(.fc-page-header) {
    margin-bottom: var(--space-1);
  }
  .planning > :global(:not(.fc-page-header)) {
    max-width: 880px;
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
    border-top: 1px solid var(--border);
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
  /* --- pace planner --- */
  .controls {
    display: grid;
    grid-template-columns: minmax(10rem, 1fr) minmax(12rem, 1.2fr) minmax(10rem, 1fr);
    gap: var(--space-3);
    align-items: start;
  }
  .pace-field,
  .exam-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .exam-field {
    flex-direction: row;
    align-items: end;
  }
  .exam-field > :global(:first-child) {
    flex: 1;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }
  .cur {
    margin-inline-start: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .sentence {
    margin: var(--space-4) 0 var(--space-1);
    font-size: var(--text-lg, 16px);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .facts {
    margin: 0;
    padding: 0 0 0 var(--space-4);
    font-size: var(--text-sm);
    color: var(--text-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .facts li.warn {
    color: var(--warning);
  }
  .weeks {
    margin-top: var(--space-3);
  }
  .cap {
    caption-side: top;
    text-align: start;
    padding: 0 0 var(--space-1) var(--space-4);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .range {
    margin-inline-start: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  :global(.pace) .basis {
    border-top: none;
    padding-inline: 0;
  }
  .apply {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .apply-text p {
    margin: 0;
    font-size: var(--text-sm);
  }
  .apply-text .muted {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .apply-text .ok {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-1);
    color: var(--success);
  }
  .advanced {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .advanced .muted {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .adv-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  @media (max-width: 1023.98px) {
    .add {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .controls {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
  }
</style>
