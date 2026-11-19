<script lang="ts">
  import type { DeckTreeNode as DeckNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
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

<section>
  <header>
    <h2>{t('deck.heading')}</h2>
    <button onclick={() => (adding = !adding)}>{t('deck.new')}</button>
  </header>

  {#if adding}
    <form onsubmit={submit} class="inline">
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={draft} placeholder={t('deck.name')} aria-label={t('deck.name')} autofocus />
      <button type="submit" class="primary">{t('common.save')}</button>
      <button type="button" onclick={() => (adding = false)}>{t('common.cancel')}</button>
    </form>
  {/if}

  {#if decks.length === 0}
    <p class="muted">{t('deck.empty')}</p>
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
    justify-content: space-between;
    align-items: baseline;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .inline {
    display: flex;
    gap: var(--fc-space-1);
    margin-bottom: var(--fc-space-2);
  }
</style>
