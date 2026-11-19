<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { NoteSummary } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';

  interface Props {
    deckId: string;
  }
  let { deckId }: Props = $props();

  let notes = $state<NoteSummary[] | null>(null);
  let error = $state<string | null>(null);

  async function load(id: string) {
    try {
      notes = await api.note.list(id);
      error = null;
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  $effect(() => {
    const id = deckId;
    notes = null;
    void load(id);
    const unsub = api.on('note.', () => void load(id));
    return () => void unsub.then((u) => u());
  });

  async function remove(id: string) {
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
          <th>{t('note.back')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each notes as n (n.id)}
          <tr>
            <td>{n.fields.front ?? n.sort_field}</td>
            <td class="muted">{n.fields.back ?? t('common.none')}</td>
            <td><button class="danger" onclick={() => remove(n.id)}>{t('common.delete')}</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
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
