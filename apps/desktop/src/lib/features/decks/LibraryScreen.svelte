<!--
  Library (v1.1, docs/11 §2–3): every deck as a card (or a tree), with card and due counts,
  classification chips and the mastery bar; "New deck". Opening a deck selects it and shows
  its deck page here; "Library" goes back. Numbers come from `decks.tree` / `decks.summaries`.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, DeckSummaries, ModuleDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Card, EmptyState, LayoutGrid, Library, List, PageHeader, Plus, SegmentedControl, Skeleton } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import DeckCard from './DeckCard.svelte';
  import DeckPage from './DeckPage.svelte';
  import DeckTreeRows from './DeckTreeRows.svelte';
  import NameDialog from './NameDialog.svelte';
  import { nodesById, summariesById } from './library';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let summaries = $state<DeckSummaries | null>(null);
  let failed = $state(false);
  let modules = $state<ModuleDto[]>([]);
  let view = $state<'grid' | 'tree'>(readView());
  let creating = $state(false);

  const nodes = $derived(nodesById(ctx.decks));
  const byId = $derived(summariesById(summaries));
  const opened = $derived(ctx.selectedDeck ? (nodes.get(ctx.selectedDeck) ?? null) : null);

  function readView(): 'grid' | 'tree' {
    try {
      return localStorage.getItem('fc.library.view') === 'tree' ? 'tree' : 'grid';
    } catch {
      return 'grid';
    }
  }
  function saveView(v: 'grid' | 'tree') {
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

  const open = (id: DeckId) => ctx.selectDeck(id);
</script>

{#if opened}
  <DeckPage {ctx} node={opened} summary={byId.get(opened.deck.id) ?? null} {modules} onback={() => ctx.selectDeck(null)} />
{:else}
  <PageHeader title={t('library.title')} subtitle={summaries ? t('library.subtitle', { count: summaries.decks.length }) : undefined}>
    {#snippet actions()}
      {#if ctx.decks.length > 0}
        <SegmentedControl
          size="sm"
          label={t('library.view')}
          options={[
            { value: 'grid', label: t('library.view_grid'), icon: LayoutGrid },
            { value: 'tree', label: t('library.view_tree'), icon: List },
          ]}
          bind:value={view}
          onchange={saveView}
        />
      {/if}
      <Button variant="primary" icon={Plus} data-testid="open-dialog-new-deck" onclick={() => (creating = true)}>{t('library.new_deck')}</Button>
    {/snippet}
  </PageHeader>

  {#if failed}
    <Card class="notice">
      <p role="alert">{t('library.load_failed')}</p>
      <Button size="sm" onclick={() => void load()}>{t('library.retry')}</Button>
    </Card>
  {/if}

  {#if !summaries && !failed}
    <ul class="grid" aria-busy="true" aria-label={t('common.loading')}>
      {#each [0, 1, 2, 3, 4, 5] as i (i)}
        <li><Skeleton shape="card" height="148px" /></li>
      {/each}
    </ul>
  {:else if ctx.decks.length === 0}
    <Card padding="lg">
      <EmptyState
        icon={Library}
        title={t('library.empty_title')}
        description={t('library.empty_body')}
        action={{ label: t('library.empty_action'), icon: Plus, onclick: () => (creating = true) }}
      />
    </Card>
  {:else if view === 'grid'}
    <ul class="grid">
      {#each ctx.decks as node (node.deck.id)}
        <li><DeckCard {node} summary={byId.get(node.deck.id) ?? null} onopen={() => open(node.deck.id)} /></li>
      {/each}
    </ul>
  {:else}
    <Card padding="none" class="tree-card">
      <DeckTreeRows nodes={ctx.decks} summaries={byId} onopen={open} />
    </Card>
  {/if}
{/if}

<NameDialog
  bind:open={creating}
  title={t('library.create_title')}
  hint={t('library.create_hint')}
  onsubmit={(name) => ctx.guarded(() => api.deck.create(name, null))}
/>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  :global(.notice.fc-card) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }
  :global(.notice.fc-card) p {
    margin: 0;
    color: var(--danger);
  }
  :global(.tree-card.fc-card) {
    overflow: hidden;
  }
</style>
