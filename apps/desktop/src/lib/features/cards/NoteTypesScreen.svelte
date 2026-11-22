<!--
  Settings → Note types (v1.3): written for a learner first. Pick a type; its fields (name,
  what they contain, required, order) sit next to a live preview of its cards. "New note
  type" starts from ready-made starting points. The template HTML / CSS and the technical
  field flags are under "Advanced: template code". Changes that lose data say what they do
  to existing notes and cards (from `notetypes.usage`) before they happen. The backend
  validates, renders and stores.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import { CLASSIC_NOTETYPE_ID } from '$lib/api/client';
  import type {
    AttributeDefDto,
    CardTypeDto,
    CreateNoteType,
    FieldKind,
    NoteTypeDto,
    NoteTypeInput,
    NoteTypeUsage,
    RenderedSides,
  } from '$lib/api/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Chip, Copy, Dialog, EmptyState, FileText, Plus, Save, Select, Trash2, Input, Undo2 } from '$lib/ui';
  import { FormRow, LoadState, SettingsAdvanced, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';
  import { move, newField, sampleFields, toInput } from './content';
  import { moduleKnownOn, trackModules } from './modules.svelte';
  import FieldRow from './notetypes/FieldRow.svelte';
  import NewTypeDialog from './notetypes/NewTypeDialog.svelte';
  import NoteTypePreview from './notetypes/NoteTypePreview.svelte';
  import TemplateCode from './notetypes/TemplateCode.svelte';
  import { templatesUsing } from './notetypes/starters';

  let { ctx: _ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: FieldKind[] = ['rich_text', 'text', 'image', 'audio', 'video', 'number', 'tag_list', 'link'];

  /** A question the page asks before (or instead of) a change that affects existing data. */
  type Ask =
    | { kind: 'remove_field'; index: number; name: string; notes: number }
    | { kind: 'field_used'; name: string; key: string; templates: string }
    | { kind: 'remove_template'; index: number; name: string }
    | { kind: 'template_in_use'; name: string; cards: number }
    | { kind: 'delete' }
    | { kind: 'switch'; to: string };

  let list = $state<NoteTypeDto[] | null>(null);
  let loadFailed = $state(false);
  let engines = $state<CardTypeDto[]>([]);
  let classifications = $state<AttributeDefDto[]>([]);
  let selected = $state<string | null>(null);
  let draft = $state<NoteTypeInput | null>(null);
  let pristine = $state('');
  let usage = $state<NoteTypeUsage | null>(null);
  /** `null` until the first render of the current definition arrived. */
  let preview = $state<RenderedSides[] | null>(null);
  let previewError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);
  let creating = $state(false);
  let ask = $state<Ask | null>(null);

  const current = $derived(list?.find((n) => n.id === selected) ?? null);
  const own = $derived((list ?? []).filter((n) => !n.builtin));
  const builtins = $derived((list ?? []).filter((n) => n.builtin));
  const engineOptions = $derived(engines.map((e) => ({ value: e.id, label: td(e.name_key) })));
  const kindOptions = $derived(KINDS.map((k) => ({ value: k, label: t(`notetype.contains.${k}`) })));
  const notes = $derived(usage?.notes ?? 0);
  const dirty = $derived(draft !== null && JSON.stringify($state.snapshot(draft)) !== pristine);

  /** What saving the draft will do to existing notes and cards (shown above the save button). */
  const consequences = $derived.by(() => {
    if (!draft || !current || notes === 0) return [] as string[];
    const out: string[] = [];
    const kept = new Set(draft.fields.map((f) => f.key));
    for (const f of current.fields) {
      if (!kept.has(f.key)) out.push(t('notetype.save_loses_field', { name: f.name, notes }));
    }
    if (draft.kind === 'standard') {
      for (const tpl of draft.templates) {
        if (!tpl.key) out.push(t('notetype.save_new_cards', { name: tpl.name, notes }));
      }
    }
    return out;
  });

  function fail(e: unknown) {
    const err = e as ApiError;
    error = td(err.messageKey);
  }

  async function load() {
    try {
      const items = (await api.call('notetypes.list', {})).notetypes;
      list = items;
      loadFailed = false;
      if (!items.some((n) => n.id === selected)) selected = items[0]?.id ?? null;
    } catch (e) {
      if (!list) loadFailed = true;
      fail(e);
    }
  }

  $effect(() => {
    trackModules();
    void load();
    api.call('cardtypes.list', {}).then((r) => (engines = r.card_types), fail);
    const unsub = api.on(['note_type_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });

  // Global note classifications (none while the `attributes` module is off).
  $effect(() => {
    classifications = [];
    if (!moduleKnownOn('attributes')) return;
    api.call('attributes.list', { scope_deck_id: null, include_deleted: false }).then(
      (r) => (classifications = r.defs.filter((d) => d.target === 'note' && d.scope_deck_id === null)),
      () => (classifications = []),
    );
  });

  // A fresh draft when another type (or a new version of this one) is shown.
  const version = $derived(current ? `${current.id}:${current.version}:${current.updated_at}` : '');
  let shownId: string | null = null;
  $effect(() => {
    void version;
    const nt = current;
    if (nt?.id !== shownId) status = null;
    shownId = nt?.id ?? null;
    const input = nt ? toInput(nt) : null;
    pristine = input ? JSON.stringify(input) : '';
    draft = input;
    if (nt) {
      api.call('notetypes.usage', { id: nt.id }).then(
        (u) => (usage = u),
        () => (usage = null),
      );
    } else usage = null;
  });

  // Live preview of the unsaved definition with sample values.
  $effect(() => {
    const def = draft ? $state.snapshot(draft) : null;
    if (!def) return;
    const timer = setTimeout(() => {
      api.call('cards.preview', { notetype_id: null, definition: def, fields: sampleFields(def), tags: [], deck_id: null }).then(
        (r) => {
          preview = r.cards;
          previewError = null;
        },
        (e: ApiError) => {
          preview = [];
          const reason = typeof e.details?.['reason'] === 'string' ? ` (${e.details['reason']})` : '';
          previewError = `${t('notetype.preview_broken')} ${td(e.messageKey)}${reason}`;
        },
      );
    }, 300);
    return () => clearTimeout(timer);
  });

  function selectType(id: string) {
    if (id === selected) return;
    if (dirty) ask = { kind: 'switch', to: id };
    else selected = id;
  }

  async function save() {
    if (!draft || !current) return;
    error = null;
    try {
      const saved = await api.call('notetypes.update', { id: current.id, definition: $state.snapshot(draft) });
      await load();
      selected = saved.id;
      status = t('notetype.saved');
    } catch (e) {
      fail(e);
    }
  }

  function discard() {
    if (!current) return;
    draft = toInput(current);
  }

  async function create(req: CreateNoteType) {
    error = null;
    try {
      const nt = await api.call('notetypes.create', req);
      creating = false;
      await load();
      selected = nt.id;
    } catch (e) {
      creating = false;
      fail(e);
    }
  }

  async function remove() {
    ask = null;
    if (!current) return;
    try {
      await api.call('notetypes.delete', { id: current.id });
      await load();
    } catch (e) {
      fail(e);
    }
  }

  function addField() {
    if (draft) draft.fields = [...draft.fields, newField(t('notetype.new_field'))];
  }

  function askRemoveField(i: number) {
    const f = draft?.fields[i];
    if (!draft || !f) return;
    const name = f.name.trim() || t('notetype.field_n', { n: i + 1 });
    // A field that was never saved holds nothing yet.
    if (f.key) {
      const used = templatesUsing(f.key, draft.templates);
      if (used.length) {
        ask = { kind: 'field_used', name, key: f.key, templates: used.map((x) => `“${x.name}”`).join(', ') };
        return;
      }
      if (notes > 0) {
        ask = { kind: 'remove_field', index: i, name, notes };
        return;
      }
    }
    draft.fields = draft.fields.filter((_, j) => j !== i);
  }

  function askRemoveTemplate(i: number) {
    const tpl = draft?.templates[i];
    if (!draft || !tpl) return;
    const cards = tpl.key ? (usage?.cards_by_template[tpl.key] ?? 0) : 0;
    if (cards > 0) ask = { kind: 'template_in_use', name: tpl.name, cards };
    else if (tpl.key) ask = { kind: 'remove_template', index: i, name: tpl.name };
    else draft.templates = draft.templates.filter((_, j) => j !== i);
  }

  function confirmAsk() {
    const a = ask;
    ask = null;
    if (!a || !draft) return;
    if (a.kind === 'remove_field') draft.fields = draft.fields.filter((_, j) => j !== a.index);
    else if (a.kind === 'remove_template') draft.templates = draft.templates.filter((_, j) => j !== a.index);
    else if (a.kind === 'switch') selected = a.to;
  }

  /** The danger section's text: what deleting this type would do, or why it cannot. */
  const deleteText = $derived.by(() => {
    if (!current) return '';
    if (current.id === CLASSIC_NOTETYPE_ID) return t('notetype.delete_classic');
    if (notes > 0) return t('notetype.delete_in_use', { notes });
    const free = t('notetype.delete_free');
    return current.builtin === 'subtitle' ? `${t('notetype.delete_subtitle')} ${free}` : free;
  });
  const canDelete = $derived(!!current && current.id !== CLASSIC_NOTETYPE_ID && usage !== null && notes === 0);

  const summary = $derived.by(() => {
    if (!current || !draft) return '';
    const used = usage === null ? '' : t('notetype.used_by', { notes });
    const makes = draft.kind === 'cloze' ? t('notetype.makes_cloze') : t('notetype.makes_cards', { cards: draft.templates.length });
    return `${used} ${makes}`.trim();
  });
</script>

<SettingsPage title={t('notetype.heading')} subtitle={t('notetype.intro')}>
  {#snippet actions()}
    <Button variant="primary" icon={Plus} onclick={() => (creating = true)}>{t('notetype.new')}</Button>
  {/snippet}

  {#if error}<p class="banner" role="alert">{error}</p>{/if}

  {#if list === null}
    <SettingsSection>
      <LoadState state={loadFailed ? 'error' : 'loading'} retry={() => void load()} lines={5} />
    </SettingsSection>
  {:else if list.length === 0}
    <SettingsSection>
      <EmptyState icon={FileText} title={t('notetype.empty')} action={{ label: t('notetype.new'), icon: Plus, onclick: () => (creating = true) }} />
    </SettingsSection>
  {:else}
    <SettingsSection title={t('notetype.pick_title')} description={t('notetype.pick_desc')}>
      <nav class="picker" aria-label={t('notetype.list')}>
        {#each [{ key: 'own', items: own }, { key: 'builtin', items: builtins }] as group (group.key)}
          {#if group.items.length > 0}
            <div class="group">
              <h3 class="group-title">{group.key === 'own' ? t('notetype.group_own') : t('notetype.group_builtin')}</h3>
              <ul class="types">
                {#each group.items as n (n.id)}
                  <li>
                    <button
                      type="button"
                      class="type"
                      class:active={n.id === selected}
                      aria-current={n.id === selected ? 'true' : undefined}
                      data-testid="notetype-option"
                      onclick={() => selectType(n.id)}
                    >
                      <span class="tname">{n.name}</span>
                      <span class="tmeta">
                        {#if n.builtin}<span class="tag">{t('notetype.builtin')}</span>{/if}
                        {t('notetype.field_count', { fields: n.fields.length })}
                      </span>
                    </button>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        {/each}
      </nav>
    </SettingsSection>

    {#if draft && current}
      <SettingsSection title={current.name} description={summary} body="rows">
        {#snippet actions()}
          <Button size="sm" icon={Copy} onclick={() => void create({ name: null, copy_from: current.id, definition: null, id: null })}>{t('notetype.copy_action')}</Button>
        {/snippet}
        <FormRow label={t('notetype.name')} description={t('notetype.name_hint')} labelFor="fc-nt-name">
          <Input id="fc-nt-name" bind:value={draft.name} label={t('notetype.name')} hideLabel />
        </FormRow>
      </SettingsSection>

      <div class="work">
        <SettingsSection title={t('notetype.fields')} description={t('notetype.fields_desc')}>
          {#snippet actions()}
            <Button size="sm" icon={Plus} onclick={addField}>{t('notetype.add_field')}</Button>
          {/snippet}
          <ol class="fields" aria-label={t('notetype.fields')}>
            {#each draft.fields as f, i (i)}
              <FieldRow
                bind:name={f.name}
                bind:required={f.required}
                kind={f.kind}
                index={i}
                count={draft.fields.length}
                onmove={(d) => draft && (draft.fields = move(draft.fields, i, d))}
                onremove={() => askRemoveField(i)}
              >
                {#snippet contains()}
                  {#if f.key}
                    <span class="ctext">{t(`notetype.contains.${f.kind}`)}</span>
                  {:else}
                    <Select size="sm" options={kindOptions} bind:value={f.kind} label={t('notetype.contains_pick', { name: f.name })} hideLabel />
                  {/if}
                {/snippet}
              </FieldRow>
            {/each}
          </ol>
          {#if classifications.length > 0}
            <div class="classes">
              <strong>{t('notetype.classifications')}</strong>
              <div class="chips">{#each classifications as c (c.id)}<Chip size="sm" label={c.name} />{/each}</div>
              <span class="muted">{t('notetype.classifications_hint')}</span>
            </div>
          {/if}
        </SettingsSection>

        <SettingsSection title={t('notetype.preview_heading')} description={t('notetype.preview_desc')}>
          <NoteTypePreview cards={preview ?? []} loading={preview === null && !previewError} error={previewError} />
        </SettingsSection>
      </div>

      <SettingsAdvanced id="notetypes.code" title={t('notetype.code.title')} summary={t('notetype.code.summary')}>
        <TemplateCode bind:draft {engineOptions} {notes} onremovetemplate={askRemoveTemplate} />
      </SettingsAdvanced>

      <div class="savebar" class:dirty>
        <div class="save-text">
          {#if dirty}
            <span class="pending">{t('notetype.unsaved')}</span>
            {#each consequences as c, i (i)}<span class="effect">{c}</span>{/each}
          {:else if status}
            <span class="ok" role="status">{status}</span>
          {/if}
        </div>
        <div class="save-actions">
          {#if dirty}<Button variant="ghost" icon={Undo2} onclick={discard}>{t('notetype.discard')}</Button>{/if}
          <Button variant="primary" icon={Save} disabled={!dirty} onclick={() => void save()}>{t('notetype.save')}</Button>
        </div>
      </div>

      <SettingsSection title={t('notetype.delete')} tone="danger" body="rows">
        <FormRow label={current.name} description={deleteText}>
          <Button variant="danger" icon={Trash2} disabled={!canDelete} aria-label={t('notetype.delete_row', { name: current.name })} onclick={() => (ask = { kind: 'delete' })}>{t('common.delete')}</Button>
        </FormRow>
      </SettingsSection>
    {/if}
  {/if}
</SettingsPage>

{#if creating && list}
  <NewTypeDialog types={list} oncreate={create} onclose={() => (creating = false)} />
{/if}

{#if ask?.kind === 'delete' && current}
  <ConfirmDialog
    message={t('notetype.confirm_delete', { name: current.name })}
    hint={deleteText}
    confirmLabel={t('common.delete')}
    onconfirm={() => void remove()}
    oncancel={() => (ask = null)}
  />
{:else if ask?.kind === 'remove_field'}
  <ConfirmDialog
    message={t('notetype.remove_field_title', { name: ask.name })}
    hint={t('notetype.remove_field_body', { name: ask.name, notes: ask.notes })}
    confirmLabel={t('notetype.remove_field_confirm')}
    onconfirm={confirmAsk}
    oncancel={() => (ask = null)}
  />
{:else if ask?.kind === 'remove_template'}
  <ConfirmDialog
    message={t('notetype.remove_template_title', { name: ask.name })}
    hint={t('notetype.remove_template_body')}
    confirmLabel={t('notetype.remove_template_confirm')}
    onconfirm={confirmAsk}
    oncancel={() => (ask = null)}
  />
{:else if ask?.kind === 'switch'}
  <ConfirmDialog
    message={t('notetype.switch_title')}
    hint={t('notetype.switch_body', { name: current?.name ?? '' })}
    confirmLabel={t('notetype.discard_confirm')}
    onconfirm={confirmAsk}
    oncancel={() => (ask = null)}
  />
{:else if ask?.kind === 'field_used' || ask?.kind === 'template_in_use'}
  <Dialog
    open
    role="alertdialog"
    size="sm"
    title={ask.kind === 'field_used' ? t('notetype.field_used_title', { name: ask.name }) : t('notetype.template_in_use_title', { name: ask.name })}
    description={ask.kind === 'field_used'
      ? t('notetype.field_used_body', { name: ask.name, templates: ask.templates, code: `{{${ask.key}}}` })
      : t('notetype.template_in_use_body', { cards: ask.cards })}
    onclose={() => (ask = null)}
  >
    {#snippet footer()}
      <Button variant="primary" onclick={() => (ask = null)}>{t('notetype.understood')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .group-title {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .types {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(10.5rem, 1fr));
    gap: var(--space-2);
  }
  .type {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .type:hover {
    background: var(--surface-2);
  }
  .type.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .type:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .tname {
    max-width: 100%;
    overflow-wrap: anywhere;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }
  .tmeta {
    display: flex;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .tag {
    color: var(--accent-text);
  }
  .work {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(15rem, 17rem);
    gap: var(--space-6);
    align-items: start;
  }
  /* The preview stays in view while you scroll through a long field list. */
  .work > :global(:last-child) {
    position: sticky;
    top: var(--space-4);
  }
  @media (max-width: 900px) {
    .work {
      grid-template-columns: minmax(0, 1fr);
    }
    .work > :global(:last-child) {
      position: static;
    }
  }
  @container fc-settings (max-width: 600px) {
    .work {
      grid-template-columns: minmax(0, 1fr);
    }
    .work > :global(:last-child) {
      position: static;
    }
  }
  .fields {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  .ctext {
    color: var(--text-2);
  }
  .classes {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    margin-top: var(--space-3);
    font-size: var(--text-sm);
  }
  .classes strong {
    font-weight: var(--weight-medium);
  }
  .classes .muted {
    flex-basis: 100%;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .muted {
    color: var(--text-2);
  }
  /* Sticky save bar: says what saving will do to existing notes before you press it. */
  .savebar {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-3) 0 var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--surface-0);
  }
  .savebar.dirty {
    border-top-color: var(--accent);
  }
  .save-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1 1 16rem;
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .pending {
    font-weight: var(--weight-medium);
  }
  .effect {
    color: var(--text-2);
  }
  .save-actions {
    display: flex;
    gap: var(--space-2);
    margin-inline-start: auto;
  }
  .ok {
    color: var(--success);
  }
  .banner {
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
