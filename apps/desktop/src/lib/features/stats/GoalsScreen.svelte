<!--
  Statistics → Goals (v1.3): every goal reads as a sentence ("Her gün 50 kart çalış",
  "20 Haz tarihine kadar İngilizce destesini bitir") with mono progress, a thin bar and the
  backend's pace forecast; a new goal is the same sentence with blanks to fill. Home's goal
  tile reads the same `goals.list` data.
-->
<script lang="ts">
  import { formatPercent100 } from '$lib/format';
  import { onMount } from 'svelte';
  import type { IconNode } from 'lucide';
  import { api } from '$lib/api';
  import type { GoalItem, GoalKind, GoalList } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Chip,
    CircleCheck,
    ChevronDown,
    ChevronRight,
    Clock,
    Flame,
    GraduationCap,
    Icon,
    IconButton,
    Input,
    PageHeader,
    Plus,
    ProgressBar,
    Select,
    Target,
    Trash2,
    TriangleAlert,
    TrendingUp,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import { LoadState } from '../core/kit';
  import Panel from './Panel.svelte';
  import { num, shortDate, tzOffset } from './format';
  import { TEMPLATES, deckQuery, hasDeck, kindOf, sentenceKey, sentenceParts, type Template } from './goals';
  import type { DeckTreeNode } from '$lib/api/types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const KIND_ICON: Record<GoalKind, IconNode> = {
    daily_cards: Target,
    cards_learned: GraduationCap,
    study_minutes: Clock,
    streak: Flame,
    deck_completion: Target,
  };

  let goals = $state<GoalList | null>(null);
  let failed = $state(false);
  let template = $state<Template>('daily_cards');
  let target = $state(50);
  let deadline = $state('');
  let deck = $state('');
  let query = $state('');
  let more = $state(false);
  /** Unstarted cards of the chosen deck ("finish a deck" sets its target to them). */
  let unseen = $state<number | null>(null);

  const templateOptions = $derived(TEMPLATES.map((k) => ({ value: k, label: t(`goals.template.${k}`) })));
  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const deckOptions = $derived([{ value: '', label: t('goals.pick_deck') }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const deckPath = $derived(decks.find((d) => d.id === deck)?.path ?? '');
  const parts = $derived(sentenceParts(t(`goals.s.${template}` as never)));
  const ready = $derived(
    (!hasDeck(template) || deck !== '') &&
      (template !== 'finish_deck' || (deadline !== '' && (unseen ?? 0) > 0)) &&
      (template === 'finish_deck' || Number(target) > 0),
  );

  /** Unstarted cards of the deck, from the planning forecast (Rust), else a card search. */
  async function loadUnseen() {
    unseen = null;
    if (template !== 'finish_deck' || !deck) return;
    const id = deck;
    try {
      const r = await api.call('planning.forecast', { deck_id: id, new_per_day: null, exam_date: null, days: 1, tz_offset_mins: tzOffset() });
      if (deck === id) unseen = r.unseen;
    } catch {
      try {
        const r = await api.call('search.ids', { query: `${deckQuery(deckPath)} is:new` });
        if (deck === id) unseen = r.card_ids.length;
      } catch {
        unseen = null;
      }
    }
  }

  async function load() {
    const ok = await ctx.guarded(async () => {
      goals = await api.call('goals.list', { tz_offset_mins: tzOffset() });
    });
    failed = !ok && !goals;
  }

  async function add() {
    if (!ready) return;
    const scope = [hasDeck(template) && deckPath ? deckQuery(deckPath) : '', query.trim()].filter(Boolean).join(' ');
    const goalTarget = template === 'finish_deck' ? (unseen ?? 0) : Number(target);
    const ok = await ctx.guarded(() =>
      api.call('goals.create', { kind: kindOf(template), target: goalTarget, deadline: deadline || null, query: scope || null }),
    );
    if (ok) {
      deadline = '';
      query = '';
    }
    await load();
  }

  /** A stored goal as its sentence (values filled in). */
  function sentence(g: GoalItem): string {
    const d = g.goal.deadline;
    return td(sentenceKey(g.goal), {
      n: num(g.goal.target),
      date: d ? shortDate(d, i18n.locale) : '',
      deck: g.goal.query ?? t('goals.all_decks'),
    });
  }

  /** The pace forecast the backend computed, as text + tone (+ icon: colour is never the only cue). */
  function forecast(g: GoalItem): { text: string; tone: Tone; icon: IconNode } | null {
    const p = g.progress;
    if (p.done) return { text: t('goals.done'), tone: 'success', icon: CircleCheck };
    if (p.days_late !== null) {
      if (p.days_late > 0) return { text: t('goals.late', { n: p.days_late }), tone: 'warning', icon: TriangleAlert };
      if (p.days_late < 0) return { text: t('goals.early', { n: -p.days_late }), tone: 'success', icon: TrendingUp };
      return { text: t('goals.on_track', { date: p.projected_date ?? '' }), tone: 'accent', icon: TrendingUp };
    }
    if (p.projected_date) return { text: t('goals.eta', { date: p.projected_date }), tone: 'neutral', icon: TrendingUp };
    return p.daily ? null : { text: t('goals.no_pace'), tone: 'neutral', icon: Clock };
  }

  onMount(() => {
    void load();
    const unsub = api.on(['goals_changed', 'study_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="goals-page">
  <PageHeader title={t('goals.heading')} subtitle={t('goals.subtitle')} />

  <Panel body="rows">
    {#if failed}
      <div class="pad"><LoadState state="error" retry={() => void load()} /></div>
    {:else if !goals}
      <div class="pad"><LoadState state="loading" /></div>
    {:else if goals.items.length === 0}
      <p class="pad empty"><Icon icon={Target} size={14} /><strong>{t('goals.none')}</strong> <span>{t('goals.none_hint')}</span></p>
    {:else}
      <ul class="goals">
        {#each goals.items as g (g.goal.id)}
          {@const f = forecast(g)}
          <li>
            <span class="ico" aria-hidden="true"><Icon icon={KIND_ICON[g.goal.kind]} size={16} /></span>
            <div class="titles">
              <h3>{sentence(g)}</h3>
              <p class="meta">
                <span>{t(`goals.kind.${g.goal.kind}`)}</span>
                {#if g.progress.daily}<Chip size="sm" label={t('goals.daily')} />{/if}
                {#if g.goal.deadline}<span class="dl"><Icon icon={Calendar} size={14} />{t('goals.deadline')}: <span class="num">{g.goal.deadline}</span></span>{/if}
                {#if g.goal.query && g.goal.kind !== 'deck_completion'}<code>{t('goals.scope', { query: g.goal.query })}</code>{/if}
              </p>
            </div>
            <div class="progress">
              <div class="figures">
                <span class="amount num">{t('goals.progress', { current: num(g.progress.current), target: num(g.goal.target) })}</span>
                <span class="pc num">{formatPercent100(Math.round(g.progress.percent))}</span>
              </div>
              <ProgressBar
                size="sm"
                label={t(`goals.kind.${g.goal.kind}`)}
                value={Math.round(g.progress.percent)}
                tone={g.progress.done ? 'success' : 'accent'}
              />
            </div>
            <p class="forecast {f?.tone ?? ''}" role="status">
              {#if f}<Icon icon={f.icon} size={14} /><span>{f.text}</span>{/if}
            </p>
            <IconButton
              size="sm"
              icon={Trash2}
              label={t('goals.delete_named', { name: sentence(g) })}
              onclick={() => ctx.guarded(() => api.call('goals.delete', { id: g.goal.id }))}
            />
          </li>
        {/each}
      </ul>
    {/if}

    <form
      class="add"
      aria-label={t('goals.new')}
      data-testid="goal-form"
      onsubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <div class="add-head">
        <Select size="sm" label={t('goals.template.label')} options={templateOptions} bind:value={template} onchange={() => void loadUnseen()} />
      </div>
      <p class="sentence-form">
        {#each parts as p, i (i)}
          {#if 'text' in p}
            <span>{p.text}</span>
          {:else if p.slot === 'n'}
            <span class="slot n"><Input size="sm" type="number" min="1" hideLabel label={t('goals.slot.n')} bind:value={target} /></span>
          {:else if p.slot === 'date'}
            <span class="slot date"><Input size="sm" type="date" hideLabel label={t('goals.slot.date')} bind:value={deadline} /></span>
          {:else}
            <span class="slot deck"><Select size="sm" hideLabel label={t('goals.slot.deck')} options={deckOptions} bind:value={deck} onchange={() => void loadUnseen()} /></span>
          {/if}
        {/each}
        <Button type="submit" size="sm" variant="primary" icon={Plus} disabled={!ready}>{t('goals.add')}</Button>
      </p>
      {#if template === 'finish_deck' && deck && unseen !== null}
        <p class="hint-line">{unseen > 0 ? t('goals.finish_hint', { deck: deckPath, n: unseen }) : t('goals.finish_empty')}</p>
      {/if}
      <Button
        variant="ghost"
        size="sm"
        icon={more ? ChevronDown : ChevronRight}
        aria-expanded={more}
        onclick={() => (more = !more)}>{t('goals.more')}</Button
      >
      {#if more}
        <div class="extras">
          {#if template !== 'finish_deck'}
            <Input size="sm" label={t('goals.deadline')} type="date" bind:value={deadline} />
          {/if}
          <Input size="sm" label={t('goals.query')} bind:value={query} />
        </div>
      {/if}
    </form>
    <p class="hint">{t('goals.new_hint')} {t('goals.home_hint')}</p>
  </Panel>
</div>

<style>
  .goals-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  /* The header (title + Statistics tabs) spans the page like on Overview; the forms stay 880. */
  .goals-page > :global(.fc-page-header) {
    margin-bottom: var(--space-1);
  }
  .goals-page > :global(:not(.fc-page-header)) {
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
    border-bottom: 1px solid var(--border);
  }
  .empty strong {
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .goals {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .goals li {
    display: grid;
    grid-template-columns: 28px minmax(9rem, 1fr) minmax(10rem, 14rem) minmax(8rem, 12rem) auto;
    grid-template-areas: 'ico title progress forecast del';
    align-items: center;
    gap: var(--space-2) var(--space-4);
    min-height: 52px;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .ico {
    grid-area: ico;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .titles {
    grid-area: title;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .meta:empty {
    display: none;
  }
  .dl {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .progress {
    grid-area: progress;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .figures {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
  }
  .amount {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .pc {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .forecast {
    grid-area: forecast;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .forecast :global(svg) {
    flex: none;
  }
  .forecast.success {
    color: var(--success);
  }
  .forecast.warning {
    color: var(--warning);
  }
  .forecast.accent {
    color: var(--text-1);
  }
  .goals li > :global(.fc-iconbtn) {
    grid-area: del;
  }
  .add {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-1);
    background: var(--surface-0);
  }
  .add-head {
    width: 14rem;
  }
  .sentence-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-md);
  }
  .slot.n {
    width: 6rem;
  }
  .slot.date {
    width: 10rem;
  }
  .slot.deck {
    width: 14rem;
  }
  .hint-line {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .extras {
    display: grid;
    grid-template-columns: 10rem minmax(12rem, 22rem);
    gap: var(--space-2);
  }
  .hint {
    margin: 0;
    padding: 0 var(--space-4) var(--space-3);
    background: var(--surface-0);
    border-radius: 0 0 var(--radius-card) var(--radius-card);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .add > :global(.fc-btn) {
    justify-self: start;
  }
  @media (max-width: 1023.98px) {
    .goals li {
      grid-template-columns: 28px minmax(0, 1fr) minmax(9rem, 13rem) auto;
      grid-template-areas:
        'ico title progress del'
        '. forecast forecast forecast';
    }
  }
</style>
