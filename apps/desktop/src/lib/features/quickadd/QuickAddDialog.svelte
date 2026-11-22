<!--
  Quick Add (v1.2, design brief flow 1): add one card after another without leaving the page.
  Deck and note type are remembered; the note type's fields come in Tab order, Enter moves on
  and adds on the last field, Ctrl+Enter adds from anywhere. Adding keeps the dialog open,
  clears the fields, puts the cursor back in the first one and counts "Added · n" with an Undo
  (the app's normal `oplog.undo`). Each card is one ordinary `notes.create` command.
  v1.3: the term, meaning and example sentence come first (other fields wait under "More"),
  a duplicate is explained where it happens, adds end with the add page's shared confirmation,
  and the footer switches to the page's other ways (bulk, paste, file) on the chosen deck.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type { AttributeDefDto, FieldDef, NoteTypeDto, RenderedSides } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { deckPaths } from '$lib/components/deck-paths';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, ChevronRight, Dialog, Icon, Input, Layers, Select, Skeleton, Tag, Textarea } from '$lib/ui';
  import CardPreview from '../cards/CardPreview.svelte';
  import FieldInput from '../cards/FieldInput.svelte';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import { draftKeeper } from '../maintenance/drafts';
  import { classicColumn, editableFields, splitTags } from '../notes/create';
  import AddedSummary from '../notes/AddedSummary.svelte';
  import { requestAddWay, type AddWay } from '../notes/add-way.svelte';
  import { fieldRole, primaryFields } from '../notes/roles';
  import type { ScreenCtx } from '../types';

  interface Props {
    open: boolean;
    ctx: ScreenCtx;
    /** Preselected deck (the opener's, e.g. a Library row's "Add card"). */
    deck?: string | null;
  }
  let { open = $bindable(), ctx, deck = null }: Props = $props();

  const LAST_TYPE = 'fc.cards.last_notetype';
  const LAST_DECK = 'fc.quickadd.last_deck';
  /** Pages showing one deck: Quick Add opened there adds to that deck. */
  const DECK_ROUTES = ['/decks', '/notes'];

  let notetypes = $state<NoteTypeDto[] | null>(null);
  let loadError = $state<string | null>(null);
  let ntId = $state<string>(read(LAST_TYPE) ?? CLASSIC_NOTETYPE_ID);
  let deckId = $state<string | null>(null);
  let values = $state<Record<string, string>>({});
  let tagText = $state('');
  let defs = $state<AttributeDefDto[]>([]);
  let attrs = $state<Record<string, FieldValue>>({});
  let moreOpen = $state(false);
  let preview = $state<RenderedSides[]>([]);
  let busy = $state(false);
  /** Error shown under one field (`details.field` of the backend's error). */
  let fieldErrors = $state<Record<string, string>>({});
  let error = $state<string | null>(null);
  /** The last add failed as a duplicate: offer "Add anyway". */
  let duplicate = $state(false);
  /** Cards added since the dialog opened. */
  let added = $state(0);
  /** The last action here was an add (Undo would revert exactly that card). */
  let canUndo = $state(false);
  let status = $state<string | null>(null);
  /** First Esc with typed text asks; the second one closes. */
  let confirming = $state(false);
  let body = $state<HTMLDivElement | null>(null);

  const nt = $derived(notetypes?.find((n) => n.id === ntId) ?? null);
  const inline = $derived(new Set(editableFields(nt).map((f) => f.key)));
  /** Fields in the note type's order; text kinds are typed inline, media get their editor. */
  const allFields = $derived(nt?.fields ?? []);
  /** Up front: term, meaning, example sentence (+ required); the rest under "More". */
  const fields = $derived(primaryFields(nt));
  const extraFields = $derived(allFields.filter((f) => !fields.includes(f)));
  const textKeys = $derived([...fields, ...extraFields].filter((f) => inline.has(f.key)).map((f) => f.key));
  const engines = $derived(nt ? nt.templates.map((x) => x.card_type) : []);
  const paths = $derived(deckPaths(ctx.decks));
  const deckOptions = $derived(paths.map((p) => ({ value: p.id, label: p.path })));
  const typeOptions = $derived((notetypes ?? []).map((n) => ({ value: n.id, label: n.name })));
  const dirty = $derived(Object.values(values).some((v) => v.trim()));
  const canAdd = $derived(!!nt && !!deckId && !busy);

  function read(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  function write(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* ignore */
    }
  }

  function caption(f: FieldDef): string {
    const col = classicColumn(nt, textKeys.indexOf(f.key));
    if (col) return t(`create.${col}`);
    // An example sentence is welcome, never required (say so).
    return fieldRole(nt, f) === 'example' && !f.required ? t('quickadd.optional', { name: f.name }) : f.name;
  }

  /** The term typed in (for the duplicate message). */
  const termText = $derived.by(() => {
    const key = fields.find((f) => fieldRole(nt, f) === 'term')?.key ?? textKeys[0];
    return key ? (values[key] ?? '').trim() : '';
  });
  /** Field names that decide "already there". */
  const uniqueNames = $derived((nt?.fields ?? []).filter((f) => f.unique).map((f) => f.name).join(', '));

  function blankValues(type: NoteTypeDto | null): Record<string, string> {
    return Object.fromEntries((type?.fields ?? []).map((f) => [f.key, f.default_value ?? '']));
  }

  async function loadTypes() {
    loadError = null;
    try {
      const r = await api.call('notetypes.list', {});
      notetypes = r.notetypes;
      if (!r.notetypes.some((n) => n.id === ntId)) ntId = CLASSIC_NOTETYPE_ID;
    } catch (e) {
      loadError = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    }
  }

  // Load once (the dialog stays mounted after the first open).
  trackModules();
  void loadTypes();

  // Every open: a fresh count, and the deck — the opener's, the page's, else the last one used.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      added = 0;
      canUndo = false;
      status = null;
      error = null;
      confirming = false;
      const ok = (id: string | null | undefined) => (id && paths.some((p) => p.id === id) ? id : null);
      const onDeckPage = ctx.route && DECK_ROUTES.includes(ctx.route) ? ok(ctx.selectedDeck) : null;
      deckId = ok(deck) ?? onDeckPage ?? ok(read(LAST_DECK)) ?? ok(deckId) ?? ok(ctx.selectedDeck) ?? paths[0]?.id ?? null;
      void focusFirst();
    });
  });

  // New note type: its fields, defaults included (what is typed into a same-named field stays).
  $effect(() => {
    const type = nt;
    untrack(() => {
      const next = blankValues(type);
      for (const k of Object.keys(next)) if (values[k]?.trim()) next[k] = values[k]!;
      values = next;
      fieldErrors = {};
    });
    if (type && open) void focusFirst();
  });

  // Classifications of the chosen deck (the `attributes` module only).
  $effect(() => {
    const deck = deckId;
    if (!deck || !moduleKnownOn('attributes')) {
      defs = [];
      return;
    }
    let cancelled = false;
    api.call('attributes.effective', { deck_id: deck, target: 'note' }).then(
      (list) => {
        if (cancelled) return;
        const d = list.attributes.map((a) => a.def).sort((a, b) => a.position - b.position);
        defs = d;
        untrack(() => {
          attrs = Object.fromEntries(d.map((def) => [def.id, attrs[def.id] ?? initialValue(def)]));
        });
      },
      () => (defs = []),
    );
    return () => {
      cancelled = true;
    };
  });

  // Live preview, only while "More" is open (debounced).
  $effect(() => {
    if (!open || !moreOpen || !nt || !deckId) return;
    const snapshot = $state.snapshot(values);
    const id = nt.id;
    const deck = deckId;
    const timer = setTimeout(() => {
      api.call('cards.preview', { notetype_id: id, definition: null, fields: snapshot, tags: [], deck_id: deck }).then(
        (r) => (preview = r.cards),
        () => (preview = []),
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  // v1.0 crash safety: what is typed survives (restored once, dropped on add or discard).
  const draft = draftKeeper<{ notetype_id: string; fields: Record<string, string> }>('notes.quick_add');
  let restored = $state(false);
  void draft.restore().then((d) => {
    if (d && d.notetype_id === untrack(() => ntId)) values = { ...untrack(() => values), ...d.fields };
    restored = true;
  });
  $effect(() => {
    if (!restored) return;
    const snapshot = $state.snapshot(values);
    draft.save(Object.values(snapshot).some((v) => v.trim()) ? { notetype_id: ntId, fields: snapshot } : null);
  });

  async function focusFirst() {
    await tick();
    const el = body?.querySelector<HTMLElement>('[data-qa-field] textarea, [data-qa-field] input, [data-qa-field] button');
    el?.focus();
  }

  function chooseType(id: string) {
    ntId = id;
    write(LAST_TYPE, id);
  }

  function chooseDeck(id: string | null) {
    deckId = id;
    if (id) write(LAST_DECK, id);
  }

  async function add(allowDuplicate = false) {
    if (!nt || !deckId || busy) return;
    if (!dirty) {
      error = t('quickadd.empty');
      void focusFirst();
      return;
    }
    busy = true;
    error = null;
    fieldErrors = {};
    duplicate = false;
    try {
      await api.call('notes.create', {
        notetype_id: nt.id,
        deck_id: deckId,
        fields: $state.snapshot(values),
        tags: splitTags(tagText),
        attributes: toAssignments($state.snapshot(attrs)),
        allow_duplicate: allowDuplicate || null,
      });
      write(LAST_DECK, deckId);
      added += 1;
      canUndo = true;
      status = t('quickadd.added_status', { count: formatCount(added) });
      // Tags, deck, type and classifications stay for the next card; the fields clear.
      values = blankValues(nt);
      preview = [];
      await draft.discard();
      void focusFirst();
    } catch (e) {
      const err = e as ApiError;
      const message = td(err.messageKey ?? 'errors.internal.unexpected');
      const field = typeof err.details?.field === 'string' ? err.details.field : null;
      if (err.code === 'CONFLICT_NOTE_DUPLICATE') {
        duplicate = true;
        error = termText
          ? t('quickadd.duplicate', { term: termText, fields: uniqueNames || nt.name })
          : message;
      } else if (field && field in values) {
        fieldErrors = { [field]: message };
        await tick();
        body?.querySelector<HTMLElement>(`[data-qa-field="${field}"] textarea, [data-qa-field="${field}"] input`)?.focus();
      } else {
        error = message;
      }
    } finally {
      busy = false;
    }
  }

  async function undo() {
    if (!canUndo) return;
    canUndo = false;
    try {
      await api.call('oplog.undo', {});
      added = Math.max(0, added - 1);
      status = t('quickadd.undone');
    } catch (e) {
      error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    }
    void focusFirst();
  }

  function close(discard = false) {
    if (dirty && !discard) {
      confirming = true;
      return;
    }
    if (discard) {
      values = blankValues(nt);
      void draft.discard();
    }
    confirming = false;
    open = false;
  }

  function bulk() {
    go('bulk');
  }

  /** The add page's other ways, on the deck chosen here (typed text asks first). */
  function go(way: AddWay) {
    if (dirty && !confirming) {
      confirming = true;
      return;
    }
    close(true);
    requestAddWay(way, deckId);
    if (deckId) ctx.selectDeck(deckId);
    ctx.navigate?.('/notes/new');
  }

  const ways = $derived([
    { id: 'bulk' as const, label: t('add.way.bulk') },
    { id: 'paste' as const, label: t('add.way.paste') },
    { id: 'file' as const, label: t('add.way.file') },
  ]);

  function edited(key: string) {
    confirming = false;
    duplicate = false;
    error = null;
    if (fieldErrors[key]) fieldErrors = {};
  }

  /** Enter: next text field; on the last one it adds. Shift+Enter is a new line. */
  function onFieldKey(e: KeyboardEvent, key: string) {
    if (e.key !== 'Enter' || e.shiftKey || e.isComposing || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    const i = textKeys.indexOf(key);
    const next = textKeys[i + 1];
    if (next) body?.querySelector<HTMLElement>(`[data-qa-field="${next}"] textarea`)?.focus();
    else void add();
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      void add(duplicate);
    } else if (e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault();
      e.stopPropagation();
      close(confirming);
    }
  }
</script>

<Dialog bind:open title={t('quickadd.title')} dismissible={false} size="md">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="qa" bind:this={body} {onkeydown}>
    {#if loadError}
      <p class="error" role="alert">{loadError}</p>
      <Button size="sm" onclick={() => void loadTypes()}>{t('create.retry')}</Button>
    {:else if paths.length === 0}
      <div class="no-deck">
        <p>{t('quickadd.no_deck')}</p>
        <Button size="sm" variant="primary" icon={Layers} onclick={bulk}>{t('quickadd.new_deck')}</Button>
      </div>
    {:else}
      <div class="where">
        <Select size="sm" label={t('quickadd.deck')} options={deckOptions} value={deckId} onchange={chooseDeck} />
        <Select size="sm" label={t('quickadd.notetype')} options={typeOptions} value={ntId} onchange={chooseType} disabled={!notetypes} />
      </div>

      {#if !nt}
        <div aria-busy="true" aria-label={t('quickadd.loading')}><Skeleton lines={2} /></div>
      {:else}
        <div class="fields">
          {#each fields as f (f.key)}
            <div class="field" data-qa-field={f.key}>
              {#if inline.has(f.key)}
                <Textarea
                  label={caption(f)}
                  required={f.required}
                  rows={1}
                  autoGrow
                  maxRows={8}
                  value={values[f.key] ?? ''}
                  error={fieldErrors[f.key] ?? null}
                  oninput={(e) => {
                    values[f.key] = e.currentTarget.value;
                    edited(f.key);
                  }}
                  onkeydown={(e) => onFieldKey(e, f.key)}
                />
              {:else}
                <!-- A getter, not `values[key]!`: right after a note type switch the new key is not in `values` yet. -->
                <FieldInput
                  field={f}
                  bind:value={() => values[f.key] ?? '', (v) => (values[f.key] = v)}
                  {values}
                  {engines}
                />
                {#if fieldErrors[f.key]}<p class="error small" role="alert">{fieldErrors[f.key]}</p>{/if}
              {/if}
            </div>
          {/each}
        </div>

        <div class="tags">
          <Input size="sm" icon={Tag} label={t('quickadd.tags')} hideLabel placeholder={`${t('quickadd.tags')} · ${t('quickadd.tags_placeholder')}`} bind:value={tagText} />
        </div>

        <div class="more">
          <button type="button" class="more-toggle" aria-expanded={moreOpen} onclick={() => (moreOpen = !moreOpen)}>
            <span class="chev" class:open={moreOpen}><Icon icon={ChevronRight} size={16} /></span>
            <span>{t('quickadd.more')}</span>
            <span class="more-hint">{extraFields.length > 0 ? t('quickadd.more_hint_fields', { fields: extraFields.map((f) => f.name).join(', ') }) : t('quickadd.more_hint')}</span>
          </button>
          {#if moreOpen}
            <div class="more-body">
              {#if extraFields.length > 0}
                <div class="fields">
                  {#each extraFields as f (f.key)}
                    <div class="field" data-qa-field={f.key}>
                      {#if inline.has(f.key)}
                        <Textarea
                          label={caption(f)}
                          required={f.required}
                          rows={1}
                          autoGrow
                          maxRows={8}
                          value={values[f.key] ?? ''}
                          error={fieldErrors[f.key] ?? null}
                          oninput={(e) => {
                            values[f.key] = e.currentTarget.value;
                            edited(f.key);
                          }}
                          onkeydown={(e) => onFieldKey(e, f.key)}
                        />
                      {:else}
                        <FieldInput field={f} bind:value={() => values[f.key] ?? '', (v) => (values[f.key] = v)} {values} {engines} />
                        {#if fieldErrors[f.key]}<p class="error small" role="alert">{fieldErrors[f.key]}</p>{/if}
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
              {#if defs.length > 0}
                <div class="attrs">
                  {#each defs as def (def.id)}
                    <AttributeField size="sm" {def} bind:value={attrs[def.id]!} />
                  {/each}
                </div>
              {/if}
              <CardPreview cards={preview} />
            </div>
          {/if}
        </div>
      {/if}

      {#if error}
        <div class="msg" role="alert">
          <span class="error">{error}</span>
          {#if duplicate}<Button size="sm" onclick={() => void add(true)}>{t('quickadd.add_anyway')}</Button>{/if}
        </div>
      {/if}
      {#if added > 0}
        <div class="done" data-testid="quick-add-count">
          {#key added}
            <AddedSummary
              compact
              {ctx}
              {added}
              {deckId}
              onundo={canUndo ? undo : null}
              undoneText={t('quickadd.undone')}
            />
          {/key}
        </div>
      {/if}
      <p class="keys">{t('quickadd.keys_hint')}</p>
    {/if}
    <p class="sr-only" role="status">{status ?? ''}</p>
  </div>

  {#snippet footer()}
    <div class="foot">
      {#if confirming}
        <p class="confirm" role="alert">{t('quickadd.discard')}</p>
        <Button size="sm" variant="ghost" onclick={() => (confirming = false)}>{t('quickadd.keep_editing')}</Button>
        <Button size="sm" variant="danger" onclick={() => close(true)}>{t('quickadd.discard_confirm')}</Button>
      {:else}
        <div class="left">
          {#if ctx.navigate && paths.length > 0}
            <span class="other" id="qa-ways">{t('quickadd.other_ways')}</span>
            <span class="ways" role="group" aria-labelledby="qa-ways">
              {#each ways as w (w.id)}
                <Button size="sm" variant="ghost" onclick={() => go(w.id)}>{w.label}</Button>
              {/each}
            </span>
          {/if}
        </div>
        <Button size="sm" variant="ghost" kbd="Esc" onclick={() => close()}>{t('quickadd.close')}</Button>
        {#if paths.length > 0}
          <Button size="sm" variant="primary" kbd="Mod+Enter" loading={busy} disabled={!canAdd} onclick={() => void add(duplicate)}>
            {t('quickadd.add')}
          </Button>
        {/if}
      {/if}
    </div>
  {/snippet}
</Dialog>

<svelte:window onkeydown={(e) => open && !body?.contains(e.target as Node) && onkeydown(e)} />

<style>
  .qa {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .where {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: var(--space-2);
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .fields :global(textarea) {
    resize: none;
    min-height: var(--control-h-lg);
    font-size: var(--text-md);
  }
  .tags :global(input) {
    font-size: var(--text-sm);
  }
  .more-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-1) 2px 0;
    border: 0;
    border-radius: var(--radius-control);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }
  .more-toggle:hover {
    color: var(--text-1);
  }
  .more-toggle:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .more-hint {
    color: var(--text-2);
    font-weight: var(--weight-regular);
    margin-left: var(--space-1);
  }
  .chev {
    display: inline-flex;
    transition: transform var(--motion-fast) var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .more-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-0);
  }
  .attrs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--space-2) var(--space-3);
  }
  .msg {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .small {
    font-size: var(--text-xs);
    margin-top: var(--space-1);
  }
  .keys {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .no-deck {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }
  .no-deck p {
    margin: 0;
    color: var(--text-2);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
  }
  .left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-right: auto;
    min-width: 0;
  }
  .other {
    color: var(--text-2);
    font-size: var(--text-xs);
    white-space: nowrap;
  }
  .ways {
    display: inline-flex;
    flex-wrap: wrap;
  }
  .done {
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .confirm {
    margin: 0 auto 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
</style>
