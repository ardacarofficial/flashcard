<!-- One row of `DeckTree` (v1.2 dense restyle): expand, select, and inline add / rename / delete. -->
<script lang="ts">
  import type { DeckTreeNode as DeckNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, ChevronDown, ChevronRight, IconButton, Input, Pencil, Plus, Trash2 } from '$lib/ui';
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
    draft = m === 'rename' ? node.deck.name : '';
  }
  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const ok = mode === 'rename' ? await onrename(node.deck.id, draft) : await oncreate(draft, node.deck.id);
    if (ok) {
      mode = 'idle';
      expanded = true;
    }
  }
  async function confirmDelete() {
    if (await ondelete(node.deck.id)) mode = 'idle';
  }
</script>

{#snippet nameForm()}
  <form onsubmit={submit} class="inline">
    <Input size="sm" label={t('deck.name')} hideLabel placeholder={t('deck.name')} bind:value={draft} />
    <Button size="sm" variant="primary" type="submit">{t('common.save')}</Button>
    <Button size="sm" variant="ghost" onclick={() => (mode = 'idle')}>{t('common.cancel')}</Button>
  </form>
{/snippet}

<li>
  <div class="row" class:selected={selectedId === node.deck.id} style:--depth={depth}>
    {#if node.children.length > 0}
      <IconButton
        size="sm"
        icon={expanded ? ChevronDown : ChevronRight}
        label={expanded ? t('deck.collapse') : t('deck.expand')}
        aria-expanded={expanded}
        onclick={() => (expanded = !expanded)}
      />
    {:else}
      <span class="spacer" aria-hidden="true"></span>
    {/if}

    {#if mode === 'rename'}
      {@render nameForm()}
    {:else}
      <button type="button" class="name" onclick={() => onselect(node.deck.id)}>{node.deck.name}</button>
      <span class="actions">
        <IconButton size="sm" icon={Plus} label={t('deck.new_sub')} onclick={() => start('add')} />
        <IconButton size="sm" icon={Pencil} label={t('common.rename')} onclick={() => start('rename')} />
        <IconButton size="sm" icon={Trash2} variant="danger" label={t('common.delete')} onclick={() => start('confirm')} />
      </span>
    {/if}
  </div>

  {#if mode === 'confirm'}
    <div class="inline confirm" style:--depth={depth + 1}>
      <span>{t('deck.confirm_delete', { name: node.deck.name })}</span>
      <Button size="sm" variant="danger" onclick={confirmDelete}>{t('common.delete')}</Button>
      <Button size="sm" variant="ghost" onclick={() => (mode = 'idle')}>{t('common.cancel')}</Button>
    </div>
  {:else if mode === 'add'}
    <div class="nested" style:--depth={depth + 1}>{@render nameForm()}</div>
  {/if}

  {#if expanded && node.children.length > 0}
    <ul>
      {#each node.children as child (child.deck.id)}
        <DeckTreeNode node={child} depth={depth + 1} {selectedId} {onselect} {oncreate} {onrename} {ondelete} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row,
  .confirm,
  .nested {
    padding-inline-start: calc(var(--depth, 0) * 16px);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--row-h);
    border-radius: var(--radius-control);
  }
  .row:hover,
  .row:focus-within {
    background: var(--surface-2);
  }
  .row.selected {
    background: var(--surface-3);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .spacer {
    flex: none;
    width: var(--control-h-sm);
  }
  .name {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  .name:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .actions {
    display: flex;
    gap: 2px;
    opacity: 0;
  }
  .row:hover .actions,
  .row:focus-within .actions {
    opacity: 1;
  }
  .inline {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-1);
    padding-block: var(--space-1);
    font-size: var(--text-sm);
  }
  .inline :global(.fc-field) {
    flex: 1;
  }
</style>
