<script lang="ts">
  import { untrack } from 'svelte';
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type { AttributeDefDto, LinkedNote, NoteTypeDto, RenderedSides } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { t, td } from '$lib/i18n/index.svelte';
  import CardMetaPanel from './CardMetaPanel.svelte';
  import CardPreview from './CardPreview.svelte';
  import FieldInput from './FieldInput.svelte';
  import { moduleOn, trackModules } from './modules.svelte';

  interface Props {
    deckId: string;
    deckName: string;
    /** Edit this note instead of adding a new one. */
    noteId?: string | null;
    onsaved?: () => void;
    onopennote?: (id: string) => void;
  }
  let { deckId, deckName, noteId = null, onsaved, onopennote }: Props = $props();

  const LAST_TYPE = 'fc.cards.last_notetype';

  let notetypes = $state<NoteTypeDto[]>([]);
  let ntId = $state<string>(lastType());
  /** Deck whose classifications apply: the given deck, or the edited note's home deck. */
  let attrDeck = $state<string | null>(null);
  const effectiveDeck = $derived(attrDeck ?? deckId);
  let values = $state<Record<string, string>>({});
  let defs = $state<AttributeDefDto[]>([]);
  let attrs = $state<Record<string, FieldValue>>({});
  /** Attribute values of the edited note (applied once its deck's definitions load). */
  let noteAttrs = $state<Record<string, FieldValue> | null>(null);
  let preview = $state<RenderedSides[]>([]);
  let backlinks = $state<LinkedNote[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  const nt = $derived(notetypes.find((n) => n.id === ntId) ?? null);
  const engines = $derived(nt ? nt.templates.map((x) => x.card_type) : []);

  function lastType(): string {
    try {
      return localStorage.getItem(LAST_TYPE) ?? CLASSIC_NOTETYPE_ID;
    } catch {
      return CLASSIC_NOTETYPE_ID;
    }
  }

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey);
  }

  $effect(() => {
    trackModules();
    api.call('notetypes.list', {}).then((r) => {
      notetypes = r.notetypes;
      if (!r.notetypes.some((n) => n.id === ntId)) ntId = CLASSIC_NOTETYPE_ID;
    }, fail);
  });

  // Edit mode: load the note.
  $effect(() => {
    const id = noteId;
    if (!id) return;
    api.call('notes.get', { id }).then((n) => {
      ntId = n.notetype_id;
      attrDeck = n.home_deck_id;
      values = { ...n.fields };
      noteAttrs = Object.fromEntries(n.attributes.map((a) => [a.attr_id, a.value ?? null]));
    }, fail);
    if (moduleOn('links')) {
      api.call('links.backlinks', { note_id: id }).then((r) => (backlinks = r.items), () => (backlinks = []));
    }
  });

  // New notes start with every field of the chosen type (defaults included).
  $effect(() => {
    if (noteId || !nt) return;
    const fieldsOf = nt.fields;
    untrack(() => {
      const next: Record<string, string> = {};
      for (const f of fieldsOf) next[f.key] = values[f.key] ?? f.default_value ?? '';
      values = next;
    });
  });

  // Classifications that apply to the deck appear next to the fields.
  $effect(() => {
    const id = effectiveDeck;
    const from = noteAttrs;
    let cancelled = false;
    api.attribute.effective(id).then((list) => {
      if (cancelled) return;
      const d = list.attributes.map((a) => a.def).sort((a, b) => a.position - b.position);
      defs = d;
      untrack(() => {
        attrs = Object.fromEntries(
          d.map((def) => [def.id, from ? (from[def.id] ?? null) : (attrs[def.id] ?? initialValue(def))]),
        );
      });
    }, fail);
    return () => {
      cancelled = true;
    };
  });

  // Live card preview (debounced).
  $effect(() => {
    const fields = $state.snapshot(values);
    const id = ntId;
    const deck = deckId;
    const timer = setTimeout(() => {
      api.call('cards.preview', { notetype_id: id, definition: null, fields, tags: [], deck_id: deck }).then(
        (r) => (preview = r.cards),
        () => (preview = []),
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  function chooseType(id: string) {
    ntId = id;
    try {
      localStorage.setItem(LAST_TYPE, id);
    } catch {
      /* ignore */
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    status = null;
    const fields = $state.snapshot(values);
    const attributes = toAssignments($state.snapshot(attrs));
    try {
      if (noteId) {
        const current = await api.call('notes.get', { id: noteId });
        await api.call('notes.update', { id: noteId, deck_id: null, fields, tags: current.tags, attributes });
        status = t('note_editor.saved');
      } else {
        await api.call('notes.create', { notetype_id: ntId, deck_id: deckId, fields, tags: [], attributes, allow_duplicate: null });
        values = {};
        status = t('note.added');
      }
      onsaved?.();
    } catch (err) {
      fail(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="editor">
  <div class="form-col">
    <h2>{noteId ? t('note_editor.edit_heading') : t('note.add_heading')}</h2>
    {#if !noteId}<p class="muted">{t('deck.current')}: {deckName}</p>{/if}

    <form onsubmit={submit} novalidate>
      <label>
        {t('notetype.type')}
        <select value={ntId} disabled={!!noteId} onchange={(e) => chooseType(e.currentTarget.value)}>
          {#each notetypes as n (n.id)}
            <option value={n.id}>{n.name}</option>
          {/each}
        </select>
      </label>

      {#if nt}
        {#each nt.fields as field (field.key)}
          <FieldInput {field} bind:value={values[field.key]!} {values} {engines} />
        {/each}
      {/if}

      {#if defs.length > 0}
        <fieldset>
          <legend>{t('note.classifications')}</legend>
          {#each defs as def (def.id)}
            <AttributeField {def} bind:value={attrs[def.id]!} />
          {/each}
        </fieldset>
      {/if}

      <div class="actions">
        <button type="submit" class="primary" disabled={busy || !nt}>
          {noteId ? t('note_editor.save') : t('note.add')}
        </button>
        {#if status}<span class="ok" role="status">{status}</span>{/if}
        {#if error}<span class="error" role="alert">{error}</span>{/if}
      </div>
    </form>
  </div>

  <div class="side-col">
    <CardPreview cards={preview} {onopennote} />
    {#if noteId}
      <CardMetaPanel {noteId} />
      {#if moduleOn('links')}
        <section>
          <h3>{t('cards.backlinks')}</h3>
          {#if backlinks.length === 0}
            <p class="muted">{t('cards.no_backlinks')}</p>
          {:else}
            <ul class="links">
              {#each backlinks as b (b.id)}
                <li><button class="small" onclick={() => onopennote?.(b.id)}>{b.title}</button></li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
    {/if}
  </div>
</section>

<style>
  .editor {
    display: grid;
    grid-template-columns: minmax(280px, 1fr) minmax(260px, 1fr);
    gap: var(--fc-space-6);
    align-items: start;
  }
  form,
  .side-col {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
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
  .links {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-1);
  }
</style>
