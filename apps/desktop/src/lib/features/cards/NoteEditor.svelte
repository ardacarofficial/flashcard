<script lang="ts">
  import { untrack } from 'svelte';
  import { draftKeeper } from '../maintenance/drafts';
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type { AttributeDefDto, LinkedNote, NoteTypeDto, RenderedSides } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { t, td } from '$lib/i18n/index.svelte';
  import CardMetaPanel from './CardMetaPanel.svelte';
  import CardPreview from './CardPreview.svelte';
  import FieldInput from './FieldInput.svelte';
  import { moduleKnownOn, moduleOn, trackModules } from './modules.svelte';
  import { Button, Chip, CircleAlert, CircleCheck, Icon, Select, Skeleton } from '$lib/ui';

  interface Props {
    deckId: string;
    deckName: string;
    /** Edit this note instead of adding a new one. */
    noteId?: string | null;
    onsaved?: () => void;
    onopennote?: (id: string) => void;
    /** v1.1 Create screen: the note type is chosen outside (no type picker here). */
    fixedType?: string | null;
    /** Inside a titled dialog: no heading of its own. */
    bare?: boolean;
    /** v1.2: tighter fields (sm inputs, 2-row text areas), e.g. inside a small dialog. */
    compact?: boolean;
    /** Put the caret in the first field once the note type is known (edit dialog). */
    autofocus?: boolean;
  }
  let { deckId, deckName, noteId = null, onsaved, onopennote, fixedType = null, bare = false, compact = false, autofocus = false }: Props = $props();

  let form = $state<HTMLFormElement | null>(null);

  /** Ctrl/⌘+Enter saves from any field (the page's own Ctrl+Enter does not also fire). */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !busy) {
      e.preventDefault();
      e.stopPropagation();
      form?.requestSubmit();
    }
  }

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

  $effect(() => {
    if (fixedType) ntId = fixedType;
  });
  const nt = $derived(notetypes.find((n) => n.id === ntId) ?? null);

  // Keyboard flow (v1.2): the edit dialog opens with the caret in the first field, once.
  let focused = false;
  $effect(() => {
    if (!autofocus || focused || !nt || !form) return;
    focused = true;
    queueMicrotask(() => form?.querySelector<HTMLElement>('textarea:not([disabled]), input:not([disabled]):not([type=hidden]), [contenteditable=true]')?.focus());
  });
  const typeOptions = $derived(notetypes.map((n) => ({ value: n.id, label: n.name })));
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

  // v1.0: what is typed into a new note survives a crash (form draft, restored on open).
  const draft = draftKeeper<{ notetype_id: string; fields: Record<string, string> }>('notes.add');
  let draftRestored = $state(false);
  $effect(() => {
    if (noteId) return;
    void draft.restore().then((d) => {
      if (d && d.notetype_id === untrack(() => ntId)) values = { ...untrack(() => values), ...d.fields };
      draftRestored = true;
    });
  });
  $effect(() => {
    if (noteId || !draftRestored) return;
    const fields = $state.snapshot(values);
    const id = ntId;
    draft.save(Object.values(fields).some((v) => v.trim()) ? { notetype_id: id, fields } : null);
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

  // Classifications that apply to the deck appear next to the fields (none while the
  // `attributes` module is off: then nothing is sent and stored values stay untouched).
  $effect(() => {
    const id = effectiveDeck;
    const from = noteAttrs;
    if (!moduleKnownOn('attributes')) {
      defs = [];
      attrs = {};
      return;
    }
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
        await draft.discard();
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

<section class="editor" class:compact>
  <div class="form-col">
    {#if !fixedType && !bare}
      <header class="head">
        <h2>{noteId ? t('note_editor.edit_heading') : t('note.add_heading')}</h2>
        {#if !noteId}<p class="muted">{t('deck.current')}: <span class="deck">{deckName}</span></p>{/if}
      </header>
    {/if}

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions (Ctrl+Enter from any field) -->
    <form bind:this={form} onsubmit={submit} {onkeydown} novalidate>
      {#if !fixedType}
        <Select size="sm" label={t('notetype.type')} options={typeOptions} value={ntId} disabled={!!noteId} onchange={chooseType} />
      {/if}

      {#if nt}
        {#each nt.fields as field (field.key)}
          <FieldInput {field} bind:value={values[field.key]!} {values} {engines} {compact} />
        {/each}
      {:else}
        <Skeleton lines={3} />
      {/if}

      {#if defs.length > 0}
        <section class="attrs-block" aria-labelledby="note-attrs-title">
          <h3 id="note-attrs-title">{t('note.classifications')}</h3>
          <div class="attrs">
            {#each defs as def (def.id)}
              <AttributeField {def} bind:value={attrs[def.id]!} />
            {/each}
          </div>
        </section>
      {/if}

      <div class="actions">
        <Button variant="primary" type="submit" kbd="Mod+Enter" loading={busy} disabled={busy || !nt}>
          {noteId ? t('note_editor.save') : t('note.add')}
        </Button>
        {#if status}<span class="ok" role="status"><Icon icon={CircleCheck} size={14} /> {status}</span>{/if}
        {#if error}<span class="error" role="alert"><Icon icon={CircleAlert} size={14} /> {error}</span>{/if}
      </div>
    </form>
  </div>

  <aside class="side-col">
    <CardPreview cards={preview} {onopennote} />
    {#if noteId}
      <CardMetaPanel {noteId} />
      {#if moduleOn('links')}
        <section class="links-block">
          <h3>{t('cards.backlinks')}</h3>
          {#if backlinks.length === 0}
            <p class="muted">{t('cards.no_backlinks')}</p>
          {:else}
            <ul class="links">
              {#each backlinks as b (b.id)}
                <li><Chip size="sm" label={b.title} onclick={() => onopennote?.(b.id)} /></li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
    {/if}
  </aside>
</section>

<style>
  .editor {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(220px, 2fr);
    gap: var(--space-6);
    align-items: start;
  }
  .head {
    margin-bottom: var(--space-3);
  }
  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .muted {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .deck {
    color: var(--text-1);
  }
  form,
  .side-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .side-col {
    position: sticky;
    top: 0;
    gap: var(--space-4);
    padding-inline-start: var(--space-6);
    border-inline-start: 1px solid var(--border);
  }
  .attrs-block {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .attrs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--space-2) var(--space-3);
  }
  .actions {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    flex-wrap: wrap;
    padding-top: var(--space-1);
  }
  .ok,
  .error {
    display: inline-flex;
    gap: var(--space-1);
    align-items: center;
    font-size: var(--text-xs);
  }
  .ok {
    color: var(--success);
  }
  .error {
    color: var(--danger);
  }
  .links {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .compact form {
    gap: var(--space-2);
  }
  @media (max-width: 900px) {
    .editor {
      grid-template-columns: minmax(0, 1fr);
    }
    .side-col {
      position: static;
      padding-inline-start: 0;
      border-inline-start: none;
      padding-top: var(--space-4);
      border-top: 1px solid var(--border);
    }
  }
</style>
