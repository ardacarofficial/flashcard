<!--
  Home (placeholder, v1.1 shell lane): built only from $lib/ui to show the design system in
  place. The real Home (today's counts, streak, goal; docs/11 §3) is a later lane.
  Counts are shown as the backend computed them (`deck.tree`); nothing is summed here.
-->
<script lang="ts">
  import type { ScreenCtx } from '$lib/features';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Card, EmptyState, GraduationCap, Library, PageHeader, ProgressBar } from '$lib/ui';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const open = (route: string) => ctx.navigate?.(route);

  function study(deckId: string | null) {
    ctx.selectDeck(deckId);
    open('/study');
  }
</script>

<PageHeader title={t('home.title')}>
  {#snippet actions()}
    <Button variant="primary" size="lg" icon={GraduationCap} onclick={() => study(ctx.selectedDeck)}>{t('home.start')}</Button>
  {/snippet}
</PageHeader>

<section aria-labelledby="home-decks">
  <h2 id="home-decks" class="section-title">{t('home.decks')}</h2>
  {#if ctx.decks.length === 0}
    <Card>
      <EmptyState
        icon={Library}
        title={t('home.empty_title')}
        description={t('home.empty_body')}
        action={{ label: t('home.empty_action'), onclick: () => open('/decks') }}
      />
    </Card>
  {:else}
    <ul class="grid">
      {#each ctx.decks as node (node.deck.id)}
        <li>
          <Card onclick={() => study(node.deck.id)} padding="md">
            <span class="deck-name">{node.deck.name}</span>
            <span class="deck-meta">{t('home.deck_total', { total: node.counts.total })}</span>
            <span class="deck-due">{t('home.deck_counts', { due: node.counts.due, learning: node.counts.learning, new: node.counts.new })}</span>
            <ProgressBar
              size="sm"
              label={node.deck.name}
              max={node.counts.total}
              segments={[
                { value: node.counts.due, label: t('home.count_due'), tone: 'accent' },
                { value: node.counts.learning, label: t('home.count_learning'), tone: 'warning' },
                { value: node.counts.new, label: t('home.count_new'), tone: 'neutral' },
              ]}
            />
          </Card>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .section-title {
    margin: 0 0 var(--space-3);
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .grid :global(.fc-card) {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    height: 100%;
  }
  .deck-name {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    overflow-wrap: anywhere;
  }
  .deck-meta,
  .deck-due {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .deck-due {
    margin-bottom: var(--space-2);
  }
</style>
