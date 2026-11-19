<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AttributeDefDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import AttributeField from './AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from './attribute-field';

  interface Props {
    deckId: string;
    deckName: string;
  }
  let { deckId, deckName }: Props = $props();

  let defs = $state<AttributeDefDto[]>([]);
  let values = $state<Record<string, FieldValue>>({});
  let front = $state('');
  let back = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  // Reload the deck's effective classifications whenever the deck changes.
  $effect(() => {
    const id = deckId;
    let cancelled = false;
    error = null;
    defs = [];
    values = {};
    api.attribute.effective(id).then(
      (list) => {
        if (cancelled) return;
        const d = list.attributes.map((a) => a.def);
        defs = [...d].sort((a, b) => a.position - b.position);
        values = Object.fromEntries(d.map((def) => [def.id, initialValue(def)]));
      },
      (e: ApiError) => {
        if (!cancelled) error = td(e.messageKey);
      },
    );
    return () => {
      cancelled = true;
    };
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    status = null;
    try {
      await api.note.createClassic(deckId, front, back, toAssignments($state.snapshot(values)));
      front = '';
      back = '';
      status = t('note.added');
    } catch (err) {
      error = td((err as ApiError).messageKey);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h2>{t('note.add_heading')}</h2>
  <p class="muted">{t('deck.current')}: {deckName}</p>

  <form onsubmit={submit} novalidate>
    <label>
      {t('note.front')}
      <textarea bind:value={front} rows="3"></textarea>
    </label>
    <label>
      {t('note.back')}
      <textarea bind:value={back} rows="3"></textarea>
    </label>

    {#if defs.length > 0}
      <fieldset>
        <legend>{t('note.classifications')}</legend>
        {#each defs as def (def.id)}
          <AttributeField {def} bind:value={values[def.id]!} />
        {/each}
      </fieldset>
    {/if}

    <div class="actions">
      <button type="submit" class="primary" disabled={busy}>{t('note.add')}</button>
      {#if status}<span class="ok" role="status">{status}</span>{/if}
      {#if error}<span class="error" role="alert">{error}</span>{/if}
    </div>
  </form>
</section>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
    max-width: 640px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  fieldset {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--fc-space-3);
    padding: var(--fc-space-3);
  }
  legend {
    color: var(--fc-text-muted);
    padding: 0 var(--fc-space-1);
  }
  .actions {
    display: flex;
    gap: var(--fc-space-3);
    align-items: center;
  }
  .ok {
    color: var(--fc-success);
  }
</style>
