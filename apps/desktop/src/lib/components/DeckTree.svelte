<!--
  An editable deck tree (v0.1 component, v1.2 dense restyle on `$lib/ui`): select, create,
  rename and delete decks inline. The Library's tree-table (`features/decks/DeckTreeRows`)
  is the main deck view; this stays for embedding.
-->
<script lang="ts">
  import type { DeckTreeNode as DeckNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Input, Plus } from '$lib/ui';
  import DeckTreeNode from './DeckTreeNode.svelte';

  interface Props {
    decks: DeckNode[];
    selectedId: string | null;
    onselect: (id: string) => void;
    oncreate: (name: string, parentId: string | null) => Promise<boolean>;
    onrename: (id: string, name: string) => Promise<boolean>;
    ondelete: (id: string) => Promise<boolean>;
  }
  let { decks, selectedId, onselect, oncreate, onrename, ondelete }: Props = $props();

  let adding = $state(false);
  let draft = $state('');

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (await oncreate(draft, null)) {
      adding = false;
      draft = '';
    }
  }
</script>

<section class="deck-tree">
  <header>
    <h2>{t('deck.heading')}</h2>
    <Button size="sm" variant="ghost" icon={Plus} onclick={() => (adding = !adding)}>{t('deck.new')}</Button>
  </header>

  {#if adding}
    <form onsubmit={submit} class="inline">
      <Input size="sm" label={t('deck.name')} hideLabel placeholder={t('deck.name')} bind:value={draft} />
      <Button size="sm" variant="primary" type="submit">{t('common.save')}</Button>
      <Button size="sm" variant="ghost" onclick={() => (adding = false)}>{t('common.cancel')}</Button>
    </form>
  {/if}

  {#if decks.length === 0}
    <p class="empty">{t('deck.empty')}</p>
  {:else}
    <ul>
      {#each decks as node (node.deck.id)}
        <DeckTreeNode {node} depth={0} {selectedId} {onselect} {oncreate} {onrename} {ondelete} />
      {/each}
    </ul>
  {/if}
</section>

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: var(--row-h);
  }
  h2 {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-bottom: var(--space-2);
  }
  .inline :global(.fc-field) {
    flex: 1;
  }
  .empty {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
