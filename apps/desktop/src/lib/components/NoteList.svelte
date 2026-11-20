<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { NoteListItem } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
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

<section>
  <h2>{t('note.list_heading')}</h2>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notes === null}
    <p class="muted">{t('common.loading')}</p>
  {:else if notes.length === 0}
    <p class="muted">{t('note.empty')}</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>{t('note.front')}</th>
          <th>{t('note.tags')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each notes as n (n.id)}
          <tr>
            <td>{n.sort_field}</td>
            <td class="muted">{n.tags.length > 0 ? n.tags.join(' ') : t('common.none')}</td>
            <td>
              <button onclick={() => (editing = n.id)}>{t('note_editor.edit')}</button>
              <button
                class="danger"
                aria-label={t('note.delete_aria', { name: n.sort_field })}
                onclick={() => (pending = n)}>{t('common.delete')}</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
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
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: var(--fc-space-2);
    border-bottom: var(--fc-border-width) solid var(--fc-border);
  }
  th {
    color: var(--fc-text-muted);
    font-weight: 500;
  }
  td:last-child {
    text-align: right;
  }
</style>
