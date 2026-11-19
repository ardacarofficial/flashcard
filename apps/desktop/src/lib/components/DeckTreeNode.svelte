<script lang="ts">
  import type { DeckNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import DeckTreeNode from './DeckTreeNode.svelte';

  interface Props {
    node: DeckNode;
    depth: number;
    selectedId: string | null;
    onselect: (id: string) => void;
    oncreate: (name: string, parentId: string | null) => Promise<boolean>;
    onrename: (id: string, name: string) => Promise<boolean>;
    ondelete: (id: string) => Promise<boolean>;
  }
  let { node, depth, selectedId, onselect, oncreate, onrename, ondelete }: Props = $props();

  type Mode = 'idle' | 'rename' | 'add' | 'confirm';
  let mode = $state<Mode>('idle');
  let draft = $state('');
  let expanded = $state(true);

  function start(m: Mode) {
    mode = m;
    draft = m === 'rename' ? node.name : '';
  }
  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const ok = mode === 'rename' ? await onrename(node.id, draft) : await oncreate(draft, node.id);
    if (ok) {
      mode = 'idle';
      expanded = true;
    }
  }
  async function confirmDelete() {
    if (await ondelete(node.id)) mode = 'idle';
  }
</script>

<li>
  <div class="row" class:selected={selectedId === node.id} style:padding-left="calc({depth} * var(--fc-space-4))">
    {#if node.children.length > 0}
      <button
        class="toggle"
        aria-expanded={expanded}
        aria-label={expanded ? t('deck.collapse') : t('deck.expand')}
        onclick={() => (expanded = !expanded)}>{expanded ? '▾' : '▸'}</button
      >
    {:else}
      <span class="toggle-spacer"></span>
    {/if}

    {#if mode === 'rename'}
      <form onsubmit={submit} class="inline">
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={draft} aria-label={t('deck.name')} autofocus />
        <button type="submit" class="primary">{t('common.save')}</button>
        <button type="button" onclick={() => (mode = 'idle')}>{t('common.cancel')}</button>
      </form>
    {:else}
      <button class="name" onclick={() => onselect(node.id)}>{node.name}</button>
      <span class="actions">
        <button title={t('deck.new_sub')} aria-label={t('deck.new_sub')} onclick={() => start('add')}>+</button>
        <button onclick={() => start('rename')}>{t('common.rename')}</button>
        <button class="danger" onclick={() => start('confirm')}>{t('common.delete')}</button>
      </span>
    {/if}
  </div>

  {#if mode === 'confirm'}
    <div class="inline confirm" style:padding-left="calc({depth + 1} * var(--fc-space-4))">
      <span>{t('deck.confirm_delete', { name: node.name })}</span>
      <button class="danger" onclick={confirmDelete}>{t('common.delete')}</button>
      <button onclick={() => (mode = 'idle')}>{t('common.cancel')}</button>
    </div>
  {:else if mode === 'add'}
    <form onsubmit={submit} class="inline" style:padding-left="calc({depth + 1} * var(--fc-space-4))">
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={draft} placeholder={t('deck.name')} aria-label={t('deck.name')} autofocus />
      <button type="submit" class="primary">{t('common.save')}</button>
      <button type="button" onclick={() => (mode = 'idle')}>{t('common.cancel')}</button>
    </form>
  {/if}

  {#if expanded && node.children.length > 0}
    <ul>
      {#each node.children as child (child.id)}
        <DeckTreeNode node={child} depth={depth + 1} {selectedId} {onselect} {oncreate} {onrename} {ondelete} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--fc-space-1);
    border-radius: var(--fc-radius);
  }
  .row.selected {
    background: var(--fc-selected);
  }
  .name {
    flex: 1;
    text-align: left;
    border: none;
    background: transparent;
  }
  .toggle,
  .toggle-spacer {
    width: 1.5em;
    flex: none;
    border: none;
    background: transparent;
    padding: 0;
  }
  .actions {
    display: none;
    gap: var(--fc-space-1);
  }
  .row:hover .actions,
  .row:focus-within .actions {
    display: flex;
  }
  .inline {
    display: flex;
    gap: var(--fc-space-1);
    align-items: center;
    padding: var(--fc-space-1) 0;
  }
</style>
