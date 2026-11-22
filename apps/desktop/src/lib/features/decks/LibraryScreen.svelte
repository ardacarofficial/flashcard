<!--
  Library (v1.2, flow 3): every deck in a dense tree-table — cards, due, new, mastery and
  classification per row, row actions (study, add card, ⋯) on hover/focus, keyboard
  navigation — or, as the secondary view, a grid of deck cards (`fc.library.view`; the tree is
  the default). "New deck". Opening a deck selects it and shows its deck page here; the page's
  back link returns. Numbers come from `decks.tree` / `decks.summaries`.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, DeckSummaries, ModuleDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, EmptyState, LayoutGrid, Library, List, PageHeader, Plus, SegmentedControl, Skeleton } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import DeckCard from './DeckCard.svelte';
  import DeckDialogs from './DeckDialogs.svelte';
  import DeckPage from './DeckPage.svelte';
  import DeckTreeRows from './DeckTreeRows.svelte';
  import NameDialog from './NameDialog.svelte';
  import { addCardAction, deckMenu, studyAction, type DeckRequest } from './deck-menu';
  import { nodesById, summariesById, todayCounts } from './library';

  let { ctx }: { ctx: ScreenCtx } = $props();

  type View = 'grid' | 'tree';
  let summaries = $state<DeckSummaries | null>(null);
  let failed = $state(false);
  let modules = $state<ModuleDto[]>([]);
  let view = $state<View>(readView());
  let creating = $state(false);
  let request = $state<DeckRequest | null>(null);

  const nodes = $derived(nodesById(ctx.decks));
  const byId = $derived(summariesById(summaries));
  const opened = $derived(ctx.selectedDeck ? (nodes.get(ctx.selectedDeck) ?? null) : null);
  const addCard = $derived(addCardAction(ctx, modules));

  /** The stored choice; the tree when there is none (v1.2 default). */
  function readView(): View {
    try {
      return localStorage.getItem('fc.library.view') === 'grid' ? 'grid' : 'tree';
    } catch {
      return 'tree';
    }
  }
  function saveView(v: View) {
    try {
      localStorage.setItem('fc.library.view', v);
    } catch {
      /* per-viewer convenience only */
    }
  }

  async function load() {
    try {
      summaries = await api.call('decks.summaries', {});
      failed = false;
    } catch {
      failed = true;
    }
  }

  onMount(() => {
    void load();
    void api.modules.list().then(
      (m) => (modules = m.modules),
      () => {},
    );
    const unsub = api.on(['deck_', 'note', 'card', 'study_', 'undone', 'redone', 'data_changed', 'imported', 'collection_', 'attribute'], () => void load());
    return () => void unsub.then((u) => u());
  });

  // "10 decks: 4 top-level, 6 sub-decks · 141 cards to study today"
  const subtitle = $derived.by(() => {
    const all = nodes.size;
    const top = ctx.decks.length;
    const decks = all > top ? t('library.subtitle_tree', { count: all, top, sub: all - top }) : t('library.subtitle', { count: all });
    const today = summaries ? todayCounts(summaries.today).total : 0;
    return today > 0 ? `${decks} · ${t('library.subtitle_today', { count: today })}` : decks;
  });
  const open = (id: DeckId) => ctx.selectDeck(id);
</script>

{#if opened}
  <DeckPage {ctx} node={opened} summary={byId.get(opened.deck.id) ?? null} {modules} onback={() => ctx.selectDeck(null)} />
{:else}
  <PageHeader title={t('library.title')} subtitle={ctx.decks.length ? subtitle : undefined}>
    {#snippet actions()}
      {#if ctx.decks.length > 0}
        <SegmentedControl
          class="view-toggle"
          size="sm"
          label={t('library.view')}
          options={[
            { value: 'tree', label: t('library.view_tree'), icon: List },
            { value: 'grid', label: t('library.view_grid'), icon: LayoutGrid },
          ]}
          bind:value={view}
          onchange={saveView}
        />
      {/if}
      <Button icon={Plus} data-testid="open-dialog-new-deck" onclick={() => (creating = true)}>{t('library.new_deck')}</Button>
    {/snippet}
  </PageHeader>

  {#if failed}
    <div class="notice" role="alert">
      <span>{t('library.load_failed')}</span>
      <Button size="sm" variant="ghost" onclick={() => void load()}>{t('library.retry')}</Button>
    </div>
  {/if}

  {#if ctx.decks.length === 0}
    {#if summaries || failed}
      <div class="panel empty">
        <EmptyState
          icon={Library}
          title={t('library.empty_title')}
          description={t('library.empty_body')}
          action={{ label: t('library.empty_action'), icon: Plus, onclick: () => (creating = true) }}
        />
      </div>
    {:else}
      <div class="panel" aria-busy="true" aria-label={t('common.loading')}>
        {#each [0, 1, 2, 3, 4] as i (i)}<div class="skeleton-row"><Skeleton shape="text" /></div>{/each}
      </div>
    {/if}
  {:else if view === 'grid'}
    <ul class="grid">
      {#each ctx.decks as node (node.deck.id)}
        <li><DeckCard {node} summary={byId.get(node.deck.id) ?? null} onopen={() => open(node.deck.id)} /></li>
      {/each}
    </ul>
  {:else}
    <div class="panel">
      <DeckTreeRows
        nodes={ctx.decks}
        summaries={byId}
        loading={!summaries}
        onopen={open}
        onstudy={studyAction()}
        onadd={addCard}
        menu={(node) => deckMenu(ctx, modules, node, (r) => (request = r))}
      />
    </div>
    <p class="keys">{t('library.keys_hint')}</p>
  {/if}
{/if}

<NameDialog
  bind:open={creating}
  title={t('library.create_title')}
  hint={t('library.create_hint')}
  onsubmit={(name) => ctx.guarded(() => api.deck.create(name, null))}
/>
<DeckDialogs bind:request guarded={ctx.guarded} />

<style>
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  /* No overflow clipping (row menus open past the panel): round the last row's hover instead. */
  .panel :global(.fc-list-row:last-child) {
    border-radius: 0 0 calc(var(--radius-card) - 1px) calc(var(--radius-card) - 1px);
  }
  .panel.empty {
    padding: var(--space-8) var(--space-4);
  }
  .skeleton-row {
    display: flex;
    align-items: center;
    height: var(--row-h);
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(232px, 1fr));
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: var(--row-h);
    margin-bottom: var(--space-3);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .keys {
    margin: var(--space-2) 0 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
</style>
