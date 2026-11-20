<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, ExamList, JobDto, ModuleDto, OptimizerResult, PresetDto, WorkloadEstimate } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import JobProgress from '../io/JobProgress.svelte';
  import { followJob } from '../io/job';
  import { num, pct, tzOffset } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const DAYS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'] as const;
  const EASY = ['normal', 'reduced', 'minimum'] as const;
  type Easy = (typeof EASY)[number];

  let balance = $state(true);
  let easy = $state<Record<string, Easy>>(Object.fromEntries(DAYS.map((d) => [d, 'normal'])));
  let exams = $state<ExamList | null>(null);
  let name = $state('');
  let date = $state('');
  let examDeck = $state('');
  let examQuery = $state('');

  let optimizerOn = $state(false);
  let presets = $state<PresetDto[]>([]);
  let presetId = $state('');
  let job = $state<JobDto | null>(null);
  let outcome = $state('');
  let workloadDeck = $state('');
  let workload = $state<WorkloadEstimate | null>(null);

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));

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
    await ctx.guarded(async () => {
      exams = await api.call('planning.exams.list', { tz_offset_mins: tzOffset() });
    });
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
    outcome = '';
    await ctx.guarded(async () => {
      const started = await api.call('optimizer.run', { preset_id: presetId, tz_offset_mins: tzOffset() });
      job = started.job;
      const done = await followJob(started.job, (j) => (job = j));
      job = null;
      if (done.status === 'succeeded') {
        const r = done.result as OptimizerResult;
        outcome = t('planning.optimizer.done', { n: r.weights.length });
      } else if (done.status === 'cancelled') {
        outcome = t('planning.optimizer.cancelled');
      } else {
        outcome = t('planning.optimizer.failed');
        if (done.error) throw done.error;
      }
    });
    await loadWorkload();
  }

  onMount(() => {
    void loadSettings();
    void loadExams();
    void loadOptimizer();
    const unsub = api.on(['exams_changed', 'presets_changed', 'undone', 'redone'], () => {
      void loadExams();
      void loadOptimizer();
    });
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('planning.heading')}</h2>

<section>
  <h3>{t('planning.balance.heading')}</h3>
  <label class="check"><input type="checkbox" bind:checked={balance}
    onchange={() => void set('planning.load_balance', balance)} /> {t('planning.balance.toggle')}</label>
  <h4>{t('planning.balance.easy_days')}</h4>
  <p class="muted">{t('settings.planning.easy_days.description')}</p>
  <div class="days">
    {#each DAYS as d (d)}
      <label>{t(`settings.planning.easy_days.${d}.label`)}
        <select bind:value={easy[d]} onchange={() => void set(`planning.easy_days.${d}`, easy[d])}>
          {#each EASY as v (v)}<option value={v}>{t(`settings.planning.easy_days.${d}.${v}`)}</option>{/each}
        </select></label>
    {/each}
  </div>
</section>

<section>
  <h3>{t('planning.exams.heading')}</h3>
  <p class="muted">{t('planning.exams.intro')}</p>
  {#if exams && exams.items.length === 0}<p class="muted">{t('planning.exams.none')}</p>{/if}
  <ul>
    {#each exams?.items ?? [] as e (e.exam.id)}
      <li>
        <strong>{e.exam.name}</strong> {e.exam.date}
        <span class="muted">
          {e.days_left > 0 ? t('planning.exams.days_left', { n: e.days_left }) : e.days_left === 0 ? t('planning.exams.today') : t('planning.exams.past')}
          · {t('planning.exams.cards', { n: e.cards })}
          {#if e.exam.query}· <code>{e.exam.query}</code>{/if}
        </span>
        <button onclick={() => ctx.guarded(() => api.call('planning.exams.delete', { id: e.exam.id }))}>{t('planning.exams.delete')}</button>
      </li>
    {/each}
  </ul>
  <form class="row" onsubmit={(ev) => { ev.preventDefault(); void addExam(); }}>
    <label>{t('planning.exams.name')} <input bind:value={name} /></label>
    <label>{t('planning.exams.date')} <input type="date" bind:value={date} /></label>
    <label>{t('planning.exams.deck')}
      <select bind:value={examDeck}>
        <option value="">{t('planning.exams.any_deck')}</option>
        {#each decks as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
      </select></label>
    <label>{t('planning.exams.query')} <input bind:value={examQuery} placeholder={t('planning.exams.query_hint')} /></label>
    <button type="submit" disabled={!name.trim() || !date}>{t('planning.exams.add')}</button>
  </form>
</section>

{#if optimizerOn}
  <section>
    <h3>{t('planning.optimizer.heading')}</h3>
    <p class="muted">{t('planning.optimizer.intro')}</p>
    {#if presets.length === 0}
      <p class="muted">{t('planning.optimizer.none')}</p>
    {:else}
      <div class="row">
        <label>{t('planning.optimizer.preset')}
          <select bind:value={presetId}>{#each presets as p (p.id)}<option value={p.id}>{p.name}</option>{/each}</select></label>
        <button disabled={!presetId || job !== null} onclick={() => void optimize()}>{t('planning.optimizer.run')}</button>
      </div>
    {/if}
    {#if job}<JobProgress {job} />{/if}
    {#if outcome}<p role="status">{outcome}</p>{/if}
  </section>

  <section>
    <h3>{t('planning.workload.heading')}</h3>
    <label>{t('planning.workload.deck')}
      <select bind:value={workloadDeck} onchange={() => void loadWorkload()}>
        <option value="">{t('stats.scope.all_decks')}</option>
        {#each decks as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
      </select></label>
    {#if workload}
      <table>
        <thead><tr><th>{t('planning.workload.retention')}</th><th>{t('planning.workload.per_day')}</th><th>{t('planning.workload.relative')}</th></tr></thead>
        <tbody>
          {#each workload.points as p (p.retention)}
            <tr class:current={Math.abs(p.retention - workload.current_retention) < 1e-9}>
              <th scope="row">{pct(p.retention)}{#if Math.abs(p.retention - workload.current_retention) < 1e-9} ({t('planning.workload.current')}){/if}</th>
              <td>{num(p.reviews_per_day)}</td><td>×{num(p.relative, 2)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted">{t('planning.workload.basis', { n: workload.review_cards, new: workload.new_per_day })}</p>
    {/if}
  </section>
{/if}

<style>
  .row,
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
    align-items: end;
  }
  section {
    margin: var(--fc-space-5, 1.5rem) 0;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    margin: var(--fc-space-2) 0;
  }
  tr.current {
    font-weight: bold;
  }
  th,
  td {
    text-align: left;
    padding: var(--fc-space-1) var(--fc-space-3) var(--fc-space-1) 0;
  }
</style>
