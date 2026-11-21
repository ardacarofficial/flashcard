<!--
  Deck page (v1.2, flow 3). Above the fold: the title with counts and chips and the ⋯ menu;
  one action bar — primary "Study" (`Enter`) and the study modes as compact buttons (a mode
  whose module is off has none, rule 7); then a smaller flippable preview (←/→ move, Space
  flips) beside the New / Learning / Mastered progress, today's counts and the sub-decks.
  Below: the term list as a dense table with inline edit. Progress numbers come from
  `decks.summaries`, counts from `decks.tree`.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckSummary, DeckTreeNode, FlagDto, ModuleDto, NoteTerm } from '$lib/api/types';
  import { formatCount, formatPercent100 } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import {
    BookOpen,
    Button,
    ChevronLeft,
    ChevronRight,
    ClipboardCheck,
    Chip,
    Flashcard,
    IconButton,
    Layers,
    ListRow,
    Menu,
    PageHeader,
    Play,
    Plus,
    ProgressBar,
    Repeat,
    Shuffle,
    Skeleton,
    Tooltip,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import DeckDialogs from './DeckDialogs.svelte';
  import MasteryBar from './MasteryBar.svelte';
  import TermList from './TermList.svelte';
  import { addCardAction, deckMenu, goWithDeck, type DeckRequest } from './deck-menu';
  import { pageEnter } from './keys';
  import { menuRoute, moduleOn, summariesById } from './library';

  interface Props {
    ctx: ScreenCtx;
    node: DeckTreeNode;
    summary: DeckSummary | null;
    modules: ModuleDto[];
    onback: () => void;
  }
  let { ctx, node, summary, modules, onback }: Props = $props();

  const PAGE = 100;
  const deck = $derived(node.deck);

  let terms = $state<NoteTerm[]>([]);
  let cursor = $state<string | null>(null);
  let termsState = $state<'loading' | 'ready' | 'failed'>('loading');
  let loadingMore = $state(false);
  let flags = $state<FlagDto[]>([]);
  let childSummaries = $state<Map<string, DeckSummary>>(new Map());

  let index = $state(0);
  let flipped = $state(false);
  let request = $state<DeckRequest | null>(null);

  const current = $derived(terms[Math.min(index, Math.max(0, terms.length - 1))] ?? null);
  const due = $derived(node.counts.due + node.counts.learning);

  async function loadTerms() {
    const id = deck.id;
    try {
      const page = await api.call('notes.terms', { deck_id: id, include_subdecks: true, page: { cursor: null, limit: PAGE } });
      if (id !== deck.id) return;
      terms = page.items;
      cursor = page.next_cursor;
      termsState = 'ready';
      if (index >= terms.length) index = 0;
    } catch {
      termsState = 'failed';
    }
  }

  async function loadMore() {
    if (!cursor || loadingMore) return;
    loadingMore = true;
    try {
      const page = await api.call('notes.terms', { deck_id: deck.id, include_subdecks: true, page: { cursor, limit: PAGE } });
      terms = [...terms, ...page.items];
      cursor = page.next_cursor;
    } catch {
      termsState = 'failed';
    } finally {
      loadingMore = false;
    }
  }

  /** Sub-deck progress for the sub-deck rows (the Library hands only this deck's summary). */
  async function loadChildren() {
    if (!node.children.length) return;
    try {
      childSummaries = summariesById(await api.call('decks.summaries', {}));
    } catch {
      /* rows show without bars */
    }
  }

  onMount(() => {
    if (moduleOn(modules, 'flags') || modules.length === 0) {
      void api.call('flags.list', {}).then(
        (r) => (flags = r.flags),
        () => {},
      );
    }
    const unsub = api.on(['note', 'cards_', 'card_', 'study_', 'undone', 'redone', 'data_changed', 'imported', 'attribute'], () => void loadTerms());
    return () => void unsub.then((u) => u());
  });

  // A different deck opened: start over.
  $effect(() => {
    void deck.id;
    termsState = 'loading';
    index = 0;
    flipped = false;
    void loadTerms();
    void loadChildren();
  });

  function step(by: number) {
    if (terms.length === 0) return;
    index = (index + by + terms.length) % terms.length;
    flipped = false;
  }

  function onPreviewKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      step(1);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      step(-1);
    }
  }

  const go = (route: string | null) => goWithDeck(ctx, node, route);

  /** Study modes whose module is on (Match: the `study.match` page). */
  const modes = $derived(
    [
      { id: 'cards', icon: Layers, route: menuRoute(modules, { route: '/study' }), title: t('deck_page.mode_cards'), desc: t('deck_page.mode_cards_desc') },
      { id: 'learn', icon: Repeat, route: menuRoute(modules, { route: '/learn' }), title: t('deck_page.mode_learn'), desc: t('deck_page.mode_learn_desc') },
      { id: 'test', icon: ClipboardCheck, route: menuRoute(modules, { route: '/test' }), title: t('deck_page.mode_test'), desc: t('deck_page.mode_test_desc') },
      { id: 'match', icon: Shuffle, route: menuRoute(modules, { id: 'study.match' }), title: t('deck_page.mode_match'), desc: t('deck_page.mode_match_desc') },
      { id: 'reading', icon: BookOpen, route: menuRoute(modules, { route: '/reading' }), title: t('deck_page.mode_reading'), desc: t('deck_page.mode_reading_desc') },
    ].filter((m) => m.route !== null),
  );
  const studyRoute = $derived(modes.find((m) => m.id === 'cards')?.route ?? null);
  const addCard = $derived(addCardAction(ctx, modules));

  function onWindowKey(e: KeyboardEvent) {
    if (studyRoute && pageEnter(e)) {
      e.preventDefault();
      go(studyRoute);
    }
  }

  const total = $derived(summary ? summary.new + summary.learning + summary.mastered : 0);
  const legend = $derived(
    summary
      ? [
          { id: 'mastered', label: t('deck_page.mastered'), value: summary.mastered },
          { id: 'learning', label: t('deck_page.learning'), value: summary.learning },
          { id: 'new', label: t('deck_page.new'), value: summary.new },
        ]
      : [],
  );
  const today = $derived([
    { id: 'due', label: t('deck_page.today_due'), value: node.counts.due },
    { id: 'learning', label: t('deck_page.learning'), value: node.counts.learning },
    { id: 'new', label: t('deck_page.new'), value: node.counts.new },
  ]);
</script>

<svelte:window onkeydown={onWindowKey} />

<PageHeader chrome={false} title={deck.name} subtitle={deck.parent_id ? deck.path : undefined} back={{ label: t('deck_page.back'), onclick: onback }}>
  {#snippet meta()}
    <span class="facts">
      <span>{t('deck_page.cards', { count: node.counts.total })}</span>
      {#if node.children.length}<span aria-hidden="true">·</span><span>{t('library.subdecks', { count: node.children.length })}</span>{/if}
    </span>
    {#each summary?.chips ?? [] as chip (chip.attr_id + chip.label)}
      <Chip size="sm" label={chip.label} />
    {/each}
  {/snippet}
  {#snippet actions()}
    <Menu items={deckMenu(ctx, modules, node, (r) => (request = r))} label={t('deck_page.more')} align="end" />
  {/snippet}
</PageHeader>

{#if modes.length}
  <div class="actionbar">
    {#if studyRoute}
      <Button variant="primary" icon={Play} kbd="Enter" aria-keyshortcuts="Enter" onclick={() => go(studyRoute)}>
        {t('deck_page.study')}
      </Button>
      <span class="divider" aria-hidden="true"></span>
    {/if}
    <div class="modes" role="group" aria-label={t('deck_page.modes')}>
      {#each modes as m (m.id)}
        <Tooltip text={m.desc} placement="bottom">
          <Button icon={m.icon} class="mode" data-testid="deck-mode-{m.id}" onclick={() => go(m.route)}>
            {m.title}
            {#if m.id === 'cards' && due > 0}
              <span class="due-badge num" aria-label={t('deck_page.due_meta', { count: due })}>{formatCount(due)}</span>
            {/if}
          </Button>
        </Tooltip>
      {/each}
    </div>
  </div>
{/if}

<div class="overview">
  <section class="preview" aria-label={t('deck_page.preview')}>
    {#if termsState === 'loading'}
      <Skeleton shape="card" height="260px" />
    {:else if current}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="card-wrap" onkeydown={onPreviewKey}>
        <Flashcard bind:flipped size="md">
          {#snippet front()}<span class="face">{current.front.text}</span>{/snippet}
          {#snippet back()}<span class="face">{current.back?.text ?? ''}</span>{/snippet}
        </Flashcard>
      </div>
      <div class="nav">
        <IconButton size="sm" icon={ChevronLeft} label={t('deck_page.prev')} onclick={() => step(-1)} disabled={terms.length < 2} />
        <span class="position num" aria-live="polite">{t('deck_page.position', { index: formatCount(index + 1), total: formatCount(terms.length) })}</span>
        <IconButton size="sm" icon={ChevronRight} label={t('deck_page.next')} onclick={() => step(1)} disabled={terms.length < 2} />
        <span class="hint">{t('deck_page.flip_hint')}</span>
      </div>
    {:else}
      <div class="preview-empty">
        <p>{t('deck_page.preview_empty')}</p>
        {#if addCard}<Button size="sm" icon={Plus} onclick={() => addCard(node)}>{t('deck_page.add_card')}</Button>{/if}
      </div>
    {/if}
  </section>

  <div class="side">
    <section class="block" aria-labelledby="deck-progress">
      <div class="block-head">
        <h2 id="deck-progress" class="block-title">{t('deck_page.progress')}</h2>
        {#if summary}<span class="aside">{t('library.mastered', { pct: formatPercent100(summary.mastered_pct) })}</span>{/if}
      </div>
      {#if summary}
        <ProgressBar
          size="lg"
          label={t('deck_page.progress_label')}
          max={Math.max(1, total)}
          segments={[
            { value: summary.mastered, label: t('deck_page.mastered'), tone: 'success' },
            { value: summary.learning, label: t('deck_page.learning'), tone: 'warning' },
            { value: summary.new, label: t('deck_page.new'), tone: 'neutral' },
          ]}
        />
        <dl class="legend">
          {#each legend as l (l.id)}
            <div><dt><span class="dot {l.id}" aria-hidden="true"></span>{l.label}</dt><dd class="num">{formatCount(l.value)}</dd></div>
          {/each}
        </dl>
      {:else}
        <Skeleton shape="text" lines={2} />
      {/if}
    </section>

    <section class="block" aria-labelledby="deck-today">
      <h2 id="deck-today" class="block-title">{t('deck_page.today')}</h2>
      <dl class="legend">
        {#each today as l (l.id)}
          <div><dt>{l.label}</dt><dd class="num" class:zero={l.value === 0}>{formatCount(l.value)}</dd></div>
        {/each}
      </dl>
    </section>

    {#if node.children.length}
      <section class="block subdecks" aria-labelledby="deck-subs">
        <h2 id="deck-subs" class="block-title">{t('deck_page.subdecks')}</h2>
        <ul class="sub-list">
          {#each node.children as child (child.deck.id)}
            {@const childDue = child.counts.due + child.counts.learning}
            <ListRow onclick={() => ctx.selectDeck(child.deck.id)} buttonLabel={t('library.open', { name: child.deck.name })} cellColumns="48px 96px">
              <span class="sub-name">
                <span class="swatch" style:background={child.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
                <span class="sub-label">{child.deck.name}</span>
              </span>
              {#snippet cells()}
                <span class="num sub-due" class:zero={childDue === 0} title={t('library.due', { count: childDue })}>{formatCount(childDue)}</span>
                <MasteryBar summary={childSummaries.get(child.deck.id) ?? null} name={child.deck.name} />
              {/snippet}
            </ListRow>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>

<TermList
  {terms}
  status={termsState}
  {flags}
  flagsOn={moduleOn(modules, 'flags')}
  hasMore={cursor !== null}
  {loadingMore}
  guarded={ctx.guarded}
  onmore={() => void loadMore()}
  onretry={() => void loadTerms()}
  onadd={addCard ? () => addCard(node) : null}
  onchange={(row) => (terms = terms.map((r) => (r.note_id === row.note_id ? row : r)))}
/>

<DeckDialogs bind:request guarded={ctx.guarded} ondeleted={(n) => n.deck.id === deck.id && onback()} />

<style>
  .facts {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-inline-end: var(--space-1);
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .actionbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    margin-bottom: var(--space-4);
  }
  .divider {
    width: 1px;
    height: 20px;
    background: var(--border);
  }
  .modes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .modes :global(.mode svg) {
    color: var(--text-2);
  }
  .modes :global(.mode:hover svg) {
    color: var(--accent-text);
  }
  .due-badge {
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--accent-soft);
    color: var(--accent-text);
    font-size: var(--text-2xs);
    line-height: 18px;
  }
  .overview {
    container-type: inline-size;
    display: grid;
    grid-template-columns: minmax(0, 460px) minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
    margin-bottom: var(--space-6);
  }
  @container (max-width: 720px) {
    .overview > * {
      grid-column: 1 / -1;
    }
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .card-wrap {
    width: 100%;
  }
  .face {
    overflow-wrap: anywhere;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .position {
    min-width: 56px;
    color: var(--text-2);
    font-size: var(--text-xs);
    text-align: center;
  }
  .hint {
    margin-inline-start: auto;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .preview-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    min-height: 200px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-flashcard);
    color: var(--text-2);
  }
  .preview-empty p {
    margin: 0;
  }
  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
  }
  .block + .block {
    border-top: 1px solid var(--border);
  }
  .block-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .block-title {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  .aside {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-6);
    margin: 0;
  }
  .legend div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .legend dt {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .legend dd {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .legend dd.zero,
  .sub-due.zero {
    color: var(--text-2);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .dot.new {
    background: var(--border-strong);
  }
  .dot.learning {
    background: var(--warning-solid);
  }
  .dot.mastered {
    background: var(--success-solid);
  }
  .subdecks {
    padding-bottom: var(--space-1);
  }
  .sub-list {
    margin: 0 calc(-1 * var(--space-2));
    padding: 0;
    list-style: none;
  }
  .sub-name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .sub-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .swatch {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .sub-due {
    color: var(--accent-text);
  }
</style>
