<!--
  Deck page (v1.1, docs/11 §3): header with chips and the ⋯ menu, a flippable preview card
  (←/→ to move, Space to flip), study-mode tiles, the New / Learning / Mastered bar and the
  term list. Modes open their own pages through `ctx.navigate` with this deck selected; a
  mode whose module is off has no tile (rule 7). Progress numbers come from `decks.summaries`.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckSummary, DeckTreeNode, FlagDto, ModuleDto, NoteTerm } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    BookOpen,
    Button,
    Card,
    ChevronLeft,
    ChevronRight,
    Chip,
    ClipboardCheck,
    Download,
    Dialog,
    Flashcard,
    IconButton,
    Layers,
    Menu,
    PageHeader,
    Pencil,
    Plus,
    ProgressBar,
    Repeat,
    Shuffle,
    SlidersHorizontal,
    Skeleton,
    Tile,
    Trash2,
    type MenuItem,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import NameDialog from './NameDialog.svelte';
  import TermList from './TermList.svelte';
  import { menuRoute, moduleOn } from './library';

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

  let index = $state(0);
  let flipped = $state(false);

  let renaming = $state(false);
  let addingSub = $state(false);
  let deleting = $state(false);

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

  function go(route: string | null) {
    if (!route) return;
    ctx.selectDeck(deck.id);
    ctx.navigate?.(route);
  }

  /** Study modes whose module is on (docs/11 §3; Match: the `study.match` page). */
  const modes = $derived(
    [
      { id: 'cards', icon: Layers, route: menuRoute(modules, { route: '/study' }), title: t('deck_page.mode_cards'), desc: t('deck_page.mode_cards_desc'), meta: due > 0 ? t('deck_page.due_meta', { count: due }) : undefined },
      { id: 'learn', icon: Repeat, route: menuRoute(modules, { route: '/learn' }), title: t('deck_page.mode_learn'), desc: t('deck_page.mode_learn_desc'), meta: undefined },
      { id: 'test', icon: ClipboardCheck, route: menuRoute(modules, { route: '/test' }), title: t('deck_page.mode_test'), desc: t('deck_page.mode_test_desc'), meta: undefined },
      { id: 'match', icon: Shuffle, route: menuRoute(modules, { id: 'study.match' }), title: t('deck_page.mode_match'), desc: t('deck_page.mode_match_desc'), meta: undefined },
      { id: 'reading', icon: BookOpen, route: menuRoute(modules, { route: '/reading' }), title: t('deck_page.mode_reading'), desc: t('deck_page.mode_reading_desc'), meta: undefined },
    ].filter((m) => m.route !== null),
  );

  const presetsRoute = $derived(menuRoute(modules, { route: '/presets' }));
  const ioRoute = $derived(menuRoute(modules, { route: '/io' }));
  const addRoute = $derived(menuRoute(modules, { route: '/notes/new' }));

  const menu = $derived<MenuItem[]>([
    { id: 'rename', label: t('deck_page.rename'), icon: Pencil, onselect: () => (renaming = true) },
    { id: 'sub', label: t('deck_page.new_sub'), icon: Plus, onselect: () => (addingSub = true) },
    ...(presetsRoute ? [{ id: 'options', label: t('deck_page.settings'), icon: SlidersHorizontal, onselect: () => go(presetsRoute) }] : []),
    ...(ioRoute ? [{ id: 'export', label: t('deck_page.export'), icon: Download, onselect: () => go(ioRoute) }] : []),
    { id: 'sep', separator: true },
    { id: 'delete', label: t('deck_page.delete'), icon: Trash2, danger: true, onselect: () => (deleting = true) },
  ]);

  async function remove() {
    if (await ctx.guarded(() => api.deck.remove(deck.id))) {
      deleting = false;
      onback();
    }
  }

  const total = $derived(summary ? summary.new + summary.learning + summary.mastered : 0);
</script>

<PageHeader chrome={false}
  title={deck.name}
  subtitle={deck.parent_id ? deck.path : t('deck_page.cards', { count: node.counts.total })}
  back={{ label: t('deck_page.back'), onclick: onback }}
>
  {#snippet meta()}
    {#if deck.parent_id}<Chip size="sm" label={t('deck_page.cards', { count: node.counts.total })} />{/if}
    {#each summary?.chips ?? [] as chip (chip.attr_id + chip.label)}
      <Chip size="sm" tone="accent" label={chip.label} />
    {/each}
  {/snippet}
  {#snippet actions()}
    <Menu items={menu} label={t('deck_page.more')} align="end" />
  {/snippet}
</PageHeader>

<div class="layout">
  <section class="preview" aria-label={t('deck_page.preview')}>
    {#if termsState === 'loading'}
      <Skeleton shape="card" height="320px" />
    {:else if current}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="card-wrap" onkeydown={onPreviewKey}>
        <Flashcard bind:flipped size="md">
          {#snippet front()}<span class="face">{current.front.text}</span>{/snippet}
          {#snippet back()}<span class="face">{current.back?.text ?? ''}</span>{/snippet}
        </Flashcard>
      </div>
      <div class="nav">
        <IconButton icon={ChevronLeft} label={t('deck_page.prev')} variant="secondary" onclick={() => step(-1)} disabled={terms.length < 2} />
        <span class="position" aria-live="polite">{t('deck_page.position', { index: index + 1, total: terms.length })}</span>
        <IconButton icon={ChevronRight} label={t('deck_page.next')} variant="secondary" onclick={() => step(1)} disabled={terms.length < 2} />
      </div>
      <p class="hint">{t('deck_page.flip_hint')}</p>
    {:else}
      <Card padding="lg" class="preview-empty">
        <p>{t('deck_page.preview_empty')}</p>
        {#if addRoute}<Button icon={Plus} onclick={() => go(addRoute)}>{t('deck_page.add_card')}</Button>{/if}
      </Card>
    {/if}
  </section>

  {#if modes.length}
    <section aria-labelledby="deck-modes">
      <h2 id="deck-modes" class="section-title">{t('deck_page.modes')}</h2>
      <ul class="modes">
        {#each modes as m (m.id)}
          <li><Tile icon={m.icon} title={m.title} description={m.desc} meta={m.meta} testid="deck-mode-{m.id}" onclick={() => go(m.route)} /></li>
        {/each}
      </ul>
    </section>
  {/if}

  <section aria-labelledby="deck-progress">
    <h2 id="deck-progress" class="section-title">{t('deck_page.progress')}</h2>
    <Card padding="lg">
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
          <div><dt><span class="dot mastered" aria-hidden="true"></span>{t('deck_page.mastered')}</dt><dd>{summary.mastered}</dd></div>
          <div><dt><span class="dot learning" aria-hidden="true"></span>{t('deck_page.learning')}</dt><dd>{summary.learning}</dd></div>
          <div><dt><span class="dot new" aria-hidden="true"></span>{t('deck_page.new')}</dt><dd>{summary.new}</dd></div>
        </dl>
      {:else}
        <Skeleton shape="text" lines={2} />
      {/if}
    </Card>
  </section>

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
    onadd={addRoute ? () => go(addRoute) : null}
    onchange={(row) => (terms = terms.map((r) => (r.note_id === row.note_id ? row : r)))}
  />
</div>

<NameDialog bind:open={renaming} title={t('deck_page.rename_title')} initial={deck.name} onsubmit={(name) => ctx.guarded(() => api.deck.rename(deck.id, name))} />
<NameDialog
  bind:open={addingSub}
  title={t('deck_page.sub_title', { name: deck.name })}
  onsubmit={(name) => ctx.guarded(() => api.deck.create(name, deck.id))}
/>
<Dialog bind:open={deleting} role="alertdialog" size="sm" title={t('deck_page.delete_title', { name: deck.name })} description={t('deck_page.delete_body')}>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (deleting = false)}>{t('common.cancel')}</Button>
    <Button variant="danger" icon={Trash2} onclick={() => void remove()}>{t('deck_page.delete')}</Button>
  {/snippet}
</Dialog>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }
  .section-title {
    margin: 0 0 var(--space-3);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
  }
  .card-wrap {
    width: 100%;
    max-width: 720px;
  }
  .face {
    overflow-wrap: anywhere;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .position {
    min-width: 72px;
    text-align: center;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  :global(.preview-empty.fc-card) {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    max-width: 720px;
    color: var(--text-2);
  }
  :global(.preview-empty.fc-card) p {
    margin: 0;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(176px, 1fr));
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .modes :global(.fc-tile) {
    height: 100%;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-8);
    margin: var(--space-4) 0 0;
  }
  .legend div {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .legend dt {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .legend dd {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 10px;
    height: 10px;
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
</style>
