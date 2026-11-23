<!--
  Settings › Basic › "Daily pace" (v1.4, prototype `ayarlar.html` › Günlük tempo): the
  everyday questions only — how many new words a day, at most how many reviews, how firmly
  to remember — with what that pace costs (minutes a day, the curve of the next 60 days,
  when the waiting new words are done), all from `planning.forecast`. Every change is saved
  at once through `planning.apply_pace` (one undo step: the toast's "Undo"), into the
  settings package the chosen deck studies with (the core creates one when it has none).
  Everything else of a package (steps, order, FSRS, packages per deck) stays on the full
  editor (`/presets`, Advanced › Studying), linked below.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, PresetConfig, WorkloadForecast } from '$lib/api/types';
  import { formatCount, formatShortDate } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import { Badge, Brain, Button, CalendarDays, ChevronRight, EmptyState, Icon, Input, Layers, Plus, SegmentedControl, Select, StatTile, toast } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  /**
   * v1.4 A/B 7 ("Recommended new words a day"): `A` = 10 (the prototype's first-load choice,
   * ~13 min a day), `B` = the core's built-in default (`presets.list` defaults, 20). Flip here.
   */
  const RECOMMENDED_NEW = 'A' as 'A' | 'B';
  /** The ready-made paces (prototype: Easy 5 / Recommended 10 / Fast 20 / Intense 30). */
  const PICKS = [
    { n: 5, key: 'easy' },
    { n: 10, key: 'rec' },
    { n: 20, key: 'fast' },
    { n: 30, key: 'intense' },
  ] as const;
  /** Retention choices (A/B 6 = A: three named steps on the Basic page). */
  const RETENTIONS = [
    { v: 0.85, key: 'relaxed' },
    { v: 0.9, key: 'balanced' },
    { v: 0.95, key: 'strict' },
  ] as const;
  const CURVE_DAYS = 60;

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const deckOptions = $derived(decks.map((d) => ({ value: d.id, label: d.path })));

  let chosen = $state<string | null>(null);
  /** Picking a deck here also makes it the shell's selected deck (kept when the page reloads). */
  function pick(id: string) {
    chosen = id;
    ctx.selectDeck(id);
  }
  /** The deck the page writes to: the one picked here, else the shell's, else the first. */
  const deckId = $derived(
    [chosen, ctx.selectedDeck, ctx.decks[0]?.deck.id ?? null].find((id) => id && decks.some((d) => d.id === id)) ?? null,
  );

  let config = $state<PresetConfig | null>(null);
  let defaults = $state<PresetConfig | null>(null);
  let forecast = $state<WorkloadForecast | null>(null);
  /** Minutes a day (in 3 weeks) of each ready-made pace. */
  let pickMinutes = $state<Record<number, number>>({});
  let failed = $state(false);
  let routes = $state<Set<string>>(new Set());

  // Drafts of the two number fields (saved on change).
  let newDraft = $state<number | string>('');
  let reviewsDraft = $state<number | string>('');

  const recommended = $derived(RECOMMENDED_NEW === 'A' ? 10 : (defaults?.new_per_day ?? 10));

  async function load(id: string | null = deckId) {
    if (!id) return;
    const ok = await ctx.guarded(async () => {
      const [eff, list, fc] = await Promise.all([
        api.call('presets.for_deck', { deck_id: id }),
        api.call('presets.list', {}),
        api.call('planning.forecast', { deck_id: id, days: CURVE_DAYS }),
      ]);
      config = eff.config;
      defaults = list.defaults;
      forecast = fc;
      newDraft = eff.config.new_per_day;
      reviewsDraft = eff.config.reviews_per_day;
    });
    failed = !ok && !config;
    if (!ok) return;
    const mins = await Promise.all(
      PICKS.map((p) =>
        api.call('planning.forecast', { deck_id: id, new_per_day: p.n }).then(
          (f) => [p.n, f.outlook.minutes_per_day] as const,
          () => null,
        ),
      ),
    );
    pickMinutes = Object.fromEntries(mins.filter((m) => m !== null));
  }

  $effect(() => {
    void load(deckId);
  });

  onMount(() => {
    void api
      .call('modules.list', {})
      .then((r) => (routes = new Set(r.modules.filter((m) => m.enabled).flatMap((m) => m.ui.flatMap((c) => (c.route ? [c.route] : []))))))
      .catch(() => {});
  });

  /** Saves one change (instant save, A/B 8 = A) and offers to undo it. */
  async function save(change: { new_per_day?: number; reviews_per_day?: number; desired_retention?: number }) {
    const id = deckId;
    if (!id || !config) return;
    const ok = await ctx.guarded(() =>
      api.call('planning.apply_pace', {
        deck_id: id,
        new_per_day: change.new_per_day ?? config!.new_per_day,
        reviews_per_day: change.reviews_per_day ?? null,
        desired_retention: change.desired_retention ?? null,
      }),
    );
    await load(id);
    if (ok) {
      toast.show(t('presets.pace.saved'), {
        tone: 'success',
        action: { label: t('presets.pace.undo'), run: () => void ctx.guarded(() => api.call('oplog.undo', {})).then(() => load()) },
      });
    }
  }

  const whole = (v: number | string) => {
    const n = Number(v);
    return v !== '' && Number.isInteger(n) ? n : null;
  };
  function commitNew() {
    const n = whole(newDraft);
    if (n === null || n === config?.new_per_day) return void (newDraft = config?.new_per_day ?? '');
    void save({ new_per_day: n });
  }
  function commitReviews() {
    const n = whole(reviewsDraft);
    if (n === null || n === config?.reviews_per_day) return void (reviewsDraft = config?.reviews_per_day ?? '');
    void save({ reviews_per_day: n });
  }

  const mins = (m: number) => formatCount(Math.round(m));
  const pickOptions = $derived(
    PICKS.map((p) => ({
      value: p.n,
      label: `${p.n} · ${t(`presets.pace.pick.${p.key}`)}${pickMinutes[p.n] !== undefined ? ` · ${t('presets.pace.pick.minutes', { min: mins(pickMinutes[p.n]!) })}` : ''}`,
    })),
  );
  const retentionOptions = $derived.by(() => {
    const opts = RETENTIONS.map((r) => ({ value: r.v as number, label: `${t(`presets.pace.retention.${r.key}`)} %${Math.round(r.v * 100)}` }));
    const cur = config?.desired_retention;
    if (cur !== undefined && !RETENTIONS.some((r) => Math.abs(r.v - cur) < 1e-9)) {
      opts.push({ value: cur, label: `${t('presets.pace.retention.other')} %${Math.round(cur * 100)}` });
    }
    return opts;
  });
  const retentionValue = $derived(retentionOptions.find((o) => config && Math.abs(o.value - config.desired_retention) < 1e-9)?.value);

  const weeks = $derived(forecast ? Math.max(1, Math.round((forecast.outlook.offset + 1) / 7)) : 3);
  const newEffect = $derived.by(() => {
    if (!forecast || !config) return '';
    if (forecast.unseen === 0) return t('presets.pace.new_effect_none');
    if (config.new_per_day === 0) return t('presets.pace.new_effect_zero');
    if (forecast.new_finished_on) return t('presets.pace.new_effect_on', { n: formatCount(forecast.unseen), date: formatShortDate(forecast.new_finished_on) });
    return t('presets.pace.new_effect_later', { n: formatCount(forecast.unseen) });
  });
  const waitingHint = $derived.by(() => {
    if (!forecast || !config) return '';
    if (forecast.unseen === 0) return t('presets.pace.waiting_none');
    if (config.new_per_day === 0) return t('presets.pace.waiting_zero');
    if (forecast.new_finished_on) return t('presets.pace.waiting_on', { date: formatShortDate(forecast.new_finished_on) });
    return t('presets.pace.waiting_later');
  });

  /** Sparkline points of the minutes a day (presentation only: scaled to the box). */
  const W = 260;
  const H = 48;
  const curve = $derived.by(() => {
    const days = forecast?.days ?? [];
    if (days.length < 2) return null;
    const max = Math.max(1, ...days.map((d) => d.minutes));
    const pts = days.map((d, i) => `${((i / (days.length - 1)) * W).toFixed(1)},${(H - 4 - (d.minutes / max) * (H - 8)).toFixed(1)}`).join(' ');
    return { pts, from: days[0]!.minutes, to: days[days.length - 1]!.minutes };
  });

  const target = $derived(forecast?.target ?? null);
  const scopeText = $derived.by(() => {
    if (!target) return '';
    if (!target.preset_id) return t('presets.pace.scope_new', { deck: target.deck.path });
    return t('presets.pace.scope', { count: target.decks.length, decks: target.decks.map((d) => d.path).join(', ') });
  });

  const RELATED = [
    { id: 'details', route: '/presets', where: 'presets', advanced: true },
    { id: 'per_deck', route: '/presets', where: 'presets', advanced: true },
    { id: 'day_start', route: '/mode-settings', where: 'modes', advanced: true },
    { id: 'easy_days', route: '/planning', where: 'planning', advanced: false },
  ] as const;
  const related = $derived(ctx.navigate ? RELATED.filter((r) => routes.has(r.route)) : []);
</script>

<SettingsPage title={t('presets.pace.title')} subtitle={t('presets.pace.intro')}>
  {#if decks.length === 0}
    <EmptyState
      icon={Layers}
      title={t('presets.pace.empty_title')}
      description={t('presets.pace.empty')}
      action={ctx.navigate ? { label: t('presets.pace.add'), icon: Plus, onclick: () => ctx.navigate?.('/notes/new') } : undefined}
    />
  {:else if failed}
    <LoadState state="error" retry={() => void load()} />
  {:else}
    {#if decks.length > 1}
      <div class="deck">
        <Select label={t('presets.pace.deck')} options={deckOptions} value={deckId} onchange={pick} />
      </div>
    {/if}

    {#if forecast && config}
      <div class="hero" data-testid="pace-hero">
        <StatTile label={t('presets.pace.minutes')} value={`~${mins(forecast.outlook.minutes_per_day)}`} unit={t('presets.pace.minutes_unit')} hint={t('presets.pace.minutes_hint', { min: mins(forecast.weeks[0]?.minutes_per_day ?? forecast.days[0]?.minutes ?? 0) })} />
        <StatTile label={t('presets.pace.later', { weeks })} value={`~${formatCount(Math.round(forecast.outlook.cards_per_day))}`} unit={t('presets.pace.cards_unit')} hint={t('presets.pace.later_hint')} />
        <StatTile label={t('presets.pace.waiting')} value={formatCount(forecast.unseen)} unit={t('presets.pace.new_unit')} hint={waitingHint} />
        {#if curve}
          <div class="spark">
            <div class="spark-k"><span>{t('presets.pace.curve')}</span><span class="tag">{t('presets.pace.curve_tag')}</span></div>
            <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label={t('presets.pace.curve_label', { from: mins(curve.from), to: mins(curve.to) })}>
              <polyline points="0,{H} {curve.pts} {W},{H}" class="area" />
              <polyline points={curve.pts} class="line" />
            </svg>
            <div class="spark-k"><span>{t('presets.pace.today')}</span><span>{t('presets.pace.month1')}</span><span>{t('presets.pace.month2')}</span></div>
          </div>
        {/if}
      </div>

      <SettingsSection title={t('presets.pace.limits')} icon={CalendarDays} body="rows">
        <FormRow label={t('presets.pace.new_label')} description={`${t('presets.pace.new_desc')} ${t('presets.pace.new_rec', { n: recommended })}`} example={newEffect} labelFor="pace-new" stack>
          <div class="ctl">
            <SegmentedControl label={t('presets.pace.picks')} size="sm" options={pickOptions} value={config.new_per_day} onchange={(n: number) => void save({ new_per_day: n })} />
            <div class="num">
              <Input id="pace-new" type="number" size="sm" min="0" max="9999" step="1" bind:value={newDraft} onchange={commitNew} />
              <span class="unit">{t('presets.pace.new_unit')}</span>
            </div>
          </div>
        </FormRow>
        <FormRow
          label={t('presets.pace.reviews_label')}
          description={t('presets.pace.reviews_desc')}
          example={t('presets.pace.reviews_effect', { n: formatCount(Math.round(forecast.outlook.cards_per_day)), weeks })}
          labelFor="pace-reviews"
          isDefault={defaults ? config.reviews_per_day === defaults.reviews_per_day : undefined}
          onReset={defaults ? () => void save({ reviews_per_day: defaults!.reviews_per_day }) : undefined}
        >
          <div class="num">
            <Input id="pace-reviews" type="number" size="sm" min="0" max="99999" step="1" bind:value={reviewsDraft} onchange={commitReviews} />
            <span class="unit">{t('presets.pace.reviews_unit')}</span>
          </div>
        </FormRow>
      </SettingsSection>

      <SettingsSection title={t('presets.pace.memory')} icon={Brain} body="rows">
        <FormRow
          label={t('presets.pace.retention_label')}
          description={t('presets.pace.retention_desc')}
          example={t('presets.pace.retention_effect', { n: Math.round(config.desired_retention * 100) })}
          isDefault={defaults ? Math.abs(config.desired_retention - defaults.desired_retention) < 1e-9 : undefined}
          onReset={defaults ? () => void save({ desired_retention: defaults!.desired_retention }) : undefined}
        >
          <SegmentedControl label={t('presets.pace.retention_label')} options={retentionOptions} value={retentionValue} onchange={(v: number) => void save({ desired_retention: v })} />
        </FormRow>
      </SettingsSection>

      {#if scopeText}
        <div class="scope" data-testid="pace-scope">
          <Icon icon={Layers} size={16} />
          <p>{scopeText}</p>
          {#if ctx.navigate && routes.has('/presets')}
            <Button size="sm" variant="secondary" onclick={() => ctx.navigate?.('/presets', 'settings')}>{t('presets.pace.scope_action')}</Button>
          {/if}
        </div>
      {/if}

      {#if related.length > 0}
        <SettingsSection title={t('presets.pace.related')} description={t('presets.pace.related_hint')}>
          <ul class="related">
            {#each related as r (r.id)}
              <li>
                <button type="button" onclick={() => ctx.navigate?.(r.route, r.advanced ? 'settings' : undefined)}>
                  <span class="q">{t(`presets.pace.rel.${r.id}`)}</span>
                  <span class="where">{t(`presets.pace.where.${r.where}`)}{#if r.advanced}<Badge tone="neutral" variant="soft">{t('presets.pace.advanced_tag')}</Badge>{/if}</span>
                  <Icon icon={ChevronRight} size={14} />
                </button>
              </li>
            {/each}
          </ul>
        </SettingsSection>
      {/if}
    {:else}
      <LoadState state="loading" />
    {/if}
  {/if}
</SettingsPage>

<style>
  .deck {
    max-width: 360px;
  }
  .hero {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)) minmax(0, 1.4fr);
    gap: var(--space-2);
  }
  .spark {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .spark svg {
    width: 100%;
    height: 48px;
  }
  .spark .area {
    fill: var(--accent-soft);
    stroke: none;
  }
  .spark .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .spark-k {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .tag {
    font-family: var(--font-mono);
    color: var(--text-2);
  }
  .ctl {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
  }
  .num {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .num :global(.fc-field) {
    flex: none;
    width: 96px;
    min-width: 0;
  }
  .unit {
    color: var(--text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }
  .scope {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .scope :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .scope p {
    flex: 1;
    margin: 0;
  }
  .related {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--border);
  }
  .related button {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 40px;
    padding: var(--space-2);
    border: none;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    text-align: start;
    cursor: pointer;
  }
  .related button:hover {
    background: var(--surface-2);
  }
  .related button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .related :global(svg) {
    color: var(--text-3);
  }
  .where {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  @media (max-width: 1000px) {
    .hero {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .spark {
      grid-column: 1 / -1;
    }
  }
  @media (max-width: 640px) {
    .hero {
      grid-template-columns: minmax(0, 1fr);
    }
    .related button {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .where {
      grid-column: 1;
      grid-row: 2;
    }
    .scope {
      flex-wrap: wrap;
    }
  }
</style>
