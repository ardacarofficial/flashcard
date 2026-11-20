<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { NoteListItem } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Card, Chip, EmptyState, FileText, IconButton, Pencil, Skeleton, Trash2 } from '$lib/ui';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import NoteEditDialog from '$lib/features/cards/NoteEditDialog.svelte';

  interface Props {
    deckId: string;
  }
  let { deckId }: Props = $props();

  let notes = $state<NoteListItem[] | null>(null);
  let error = $state<string | null>(null);

  async function load(id: string) {
    try {
      notes = (await api.note.list(id)).items;
      error = null;
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  $effect(() => {
    const id = deckId;
    notes = null;
    void load(id);
    const unsub = api.on(['note', 'deck_', 'undone', 'redone', 'data_changed'], () => void load(id));
    return () => void unsub.then((u) => u());
  });

  let pending = $state<NoteListItem | null>(null);
  let editing = $state<string | null>(null);

  async function remove(id: string) {
    pending = null;
    try {
      await api.note.remove(id);
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }
</script>

<section class="note-list" aria-label={t('note.list_heading')}>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notes === null}
    <Skeleton lines={4} />
  {:else if notes.length === 0}
    <EmptyState icon={FileText} title={t('note.empty')} />
  {:else}
    <Card padding="none">
      <ul>
        {#each notes as n (n.id)}
          <li>
            <span class="front">{n.sort_field}</span>
            <span class="tags">
              {#each n.tags as tag (tag)}<Chip size="sm" label={tag} />{/each}
            </span>
            <span class="actions">
              <IconButton icon={Pencil} label={t('note_editor.edit')} onclick={() => (editing = n.id)} />
              <IconButton icon={Trash2} variant="danger" label={t('note.delete_aria', { name: n.sort_field })} onclick={() => (pending = n)} />
            </span>
          </li>
        {/each}
      </ul>
    </Card>
  {/if}

  {#if editing}
    <NoteEditDialog noteId={editing} {deckId} deckName="" onclose={() => (editing = null)} />
  {/if}

  {#if pending}
    <ConfirmDialog
      message={t('note.confirm_delete')}
      hint={t('note.undo_hint')}
      confirmLabel={t('common.delete')}
      onconfirm={() => remove(pending!.id)}
      oncancel={() => (pending = null)}
    />
  {/if}
</section>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--space-3);
    min-height: 52px;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .front {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-1);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    justify-content: flex-end;
  }
  .actions {
    display: flex;
    gap: var(--space-1);
  }
  .error {
    margin: 0 0 var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
