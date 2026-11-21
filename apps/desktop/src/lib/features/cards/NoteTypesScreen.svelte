<!--
  Settings → Study → Note types: list on the left; the chosen type's fields, card templates,
  styling and a live preview on the right. The backend validates, renders and stores.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AttributeDefDto, CardTypeDto, FieldKind, NoteTypeDto, NoteTypeInput, RenderedSides } from '$lib/api/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Checkbox,
    ChevronDown,
    ChevronUp,
    Chip,
    Copy,
    EmptyState,
    FileText,
    IconButton,
    Input,
    Plus,
    Save,
    Select,
    Textarea,
    Trash2,
    X,
  } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';
  import CardPreview from './CardPreview.svelte';
  import { move, newField, sampleFields, toInput } from './content';
  import { moduleKnownOn, trackModules } from './modules.svelte';

  let { ctx: _ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: FieldKind[] = ['rich_text', 'text', 'image', 'audio', 'video', 'number', 'tag_list', 'link'];

  let list = $state<NoteTypeDto[] | null>(null);
  let loadFailed = $state(false);
  let engines = $state<CardTypeDto[]>([]);
  let classifications = $state<AttributeDefDto[]>([]);
  let selected = $state<string | null>(null);
  let draft = $state<NoteTypeInput | null>(null);
  let usage = $state<number>(0);
  let preview = $state<RenderedSides[]>([]);
  let previewError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);
  let confirming = $state(false);

  const current = $derived(list?.find((n) => n.id === selected) ?? null);
  const kindOptions = $derived(KINDS.map((k) => ({ value: k, label: t(`notetype.kind.${k}`) })));
  const engineOptions = $derived(engines.map((e) => ({ value: e.id, label: td(e.name_key) })));
  const sortOptions = $derived((draft?.fields ?? []).filter((f) => f.key).map((f) => ({ value: f.key, label: f.name })));

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey);
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

  // Selecting a note type starts a fresh draft.
  $effect(() => {
    const nt = current;
    status = null;
    draft = nt ? toInput(nt) : null;
    if (nt) {
      api.call('notetypes.usage', { id: nt.id }).then((u) => (usage = u.notes), () => (usage = 0));
    }
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
          previewError = td(e.messageKey);
        },
      );
    }, 300);
    return () => clearTimeout(timer);
  });

  async function save() {
    if (!draft || !current) return;
    error = null;
    try {
      const saved = await api.call('notetypes.update', { id: current.id, definition: $state.snapshot(draft) });
      status = t('notetype.saved');
      await load();
      selected = saved.id;
    } catch (e) {
      fail(e);
    }
  }

  async function create(copyFrom: string | null) {
    error = null;
    try {
      const nt = await api.call('notetypes.create', {
        name: copyFrom ? null : t('notetype.new'),
        copy_from: copyFrom,
        definition: null,
        id: null,
      });
      await load();
      selected = nt.id;
    } catch (e) {
      fail(e);
    }
  }

  async function remove() {
    confirming = false;
    if (!current) return;
    try {
      await api.call('notetypes.delete', { id: current.id });
      await load();
    } catch (e) {
      fail(e);
    }
  }
</script>

<SettingsPage title={t('notetype.heading')} subtitle={t('notetype.intro')} width="wide">
  {#snippet actions()}
    {#if current}<Button icon={Copy} onclick={() => void create(current.id)}>{t('notetype.copy')}</Button>{/if}
    <Button variant="primary" icon={Plus} onclick={() => void create(null)}>{t('notetype.new')}</Button>
  {/snippet}

  {#if error}<p class="banner" role="alert">{error}</p>{/if}

  {#if list === null}
    <SettingsSection>
      <LoadState state={loadFailed ? 'error' : 'loading'} retry={() => void load()} lines={5} />
    </SettingsSection>
  {:else if list.length === 0}
    <SettingsSection>
      <EmptyState icon={FileText} title={t('notetype.empty')} action={{ label: t('notetype.new'), icon: Plus, onclick: () => void create(null) }} />
    </SettingsSection>
  {:else}
    <div class="layout">
      <nav aria-labelledby="nt-list-title">
        <h2 class="side-title" id="nt-list-title">{t('notetype.list')} <span class="num">{list.length}</span></h2>
        <ul class="types">
          {#each list as n (n.id)}
            <li>
              <button type="button" class="type" class:active={n.id === selected} aria-current={n.id === selected ? 'true' : undefined} onclick={() => (selected = n.id)}>
                <span class="tname">{n.name}</span>
                {#if n.builtin}<span class="tag">{t('notetype.builtin')}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
      </nav>

      {#if draft && current}
        <div class="editor">
          <SettingsSection title={current.name} description={t('notetype.usage', { notes: usage })} body="rows">
            {#snippet actions()}
              <Button variant="ghost" size="sm" icon={Trash2} onclick={() => (confirming = true)}>{t('notetype.delete')}</Button>
            {/snippet}
            <FormRow label={t('notetype.name')}>
              <Input bind:value={draft.name} label={t('notetype.name')} hideLabel />
            </FormRow>
            <FormRow label={t('notetype.sort_field')} description={t('notetype.sort_field_hint')}>
              <Select options={sortOptions} bind:value={draft.sort_field} label={t('notetype.sort_field')} hideLabel />
            </FormRow>
          </SettingsSection>

          <SettingsSection title={t('notetype.fields')}>
            {#snippet actions()}
              <Button size="sm" icon={Plus} onclick={() => draft && (draft.fields = [...draft.fields, newField(t('notetype.new_field'))])}>{t('notetype.add_field')}</Button>
            {/snippet}
            <!-- One dense row per field: name, kind, the three switches under column heads, order. -->
            <div class="ftable" role="group" aria-label={t('notetype.fields')}>
              <div class="frow fhead" aria-hidden="true">
                <span class="n">#</span>
                <span>{t('notetype.field_name')}</span>
                <span>{t('notetype.field_kind')}</span>
                <span class="c">{t('notetype.required')}</span>
                <span class="c">{t('notetype.unique')}</span>
                <span class="c">{t('notetype.searchable')}</span>
                <span></span>
              </div>
              <ol class="fields">
                {#each draft.fields as f, i (i)}
                  <li class="frow">
                    <span class="n num">{i + 1}</span>
                    <Input size="sm" bind:value={f.name} label={t('notetype.field_name')} hideLabel />
                    <Select size="sm" options={kindOptions} bind:value={f.kind} label={t('notetype.field_kind')} hideLabel />
                    <span class="c"><Checkbox bind:checked={f.required} ariaLabel={`${t('notetype.required')}: ${f.name}`} /></span>
                    <span class="c"><Checkbox bind:checked={f.unique} ariaLabel={`${t('notetype.unique')}: ${f.name}`} /></span>
                    <span class="c"><Checkbox bind:checked={f.searchable} ariaLabel={`${t('notetype.searchable')}: ${f.name}`} /></span>
                    <span class="acts">
                      <IconButton size="sm" icon={ChevronUp} label={t('notetype.move_up')} disabled={i === 0} onclick={() => draft && (draft.fields = move(draft.fields, i, -1))} />
                      <IconButton size="sm" icon={ChevronDown} label={t('notetype.move_down')} disabled={i === draft.fields.length - 1} onclick={() => draft && (draft.fields = move(draft.fields, i, 1))} />
                      <IconButton size="sm" icon={X} label={t('notetype.remove')} onclick={() => draft && (draft.fields = draft.fields.filter((_, j) => j !== i))} />
                    </span>
                  </li>
                {/each}
              </ol>
            </div>
            {#if classifications.length > 0}
              <div class="classes">
                <strong>{t('notetype.classifications')}</strong>
                <div class="chips">{#each classifications as c (c.id)}<Chip size="sm" label={c.name} />{/each}</div>
                <span class="muted">{t('notetype.classifications_hint')}</span>
              </div>
            {/if}
          </SettingsSection>

          <SettingsSection title={t('notetype.templates')} description={t('notetype.syntax_intro')}>
            {#snippet actions()}
              {#if draft?.kind === 'standard'}
                <Button
                  size="sm"
                  icon={Plus}
                  onclick={() =>
                    draft &&
                    (draft.templates = [
                      ...draft.templates,
                      { key: '', name: t('notetype.new_template'), front: '', back: '{{FrontSide}}\n<hr>\n', card_type: 'classic' },
                    ])}>{t('notetype.add_template')}</Button
                >
              {/if}
            {/snippet}
            <p class="syntax"><code>{t('notetype.syntax')}</code></p>
            <div class="templates">
              {#each draft.templates as tpl, i (i)}
                <div class="tpl">
                  <div class="tpl-head">
                    <Input size="sm" bind:value={tpl.name} label={t('notetype.template_name')} />
                    <Select size="sm" options={engineOptions} bind:value={tpl.card_type} label={t('notetype.card_type')} />
                    {#if draft.templates.length > 1}
                      <IconButton size="sm" icon={Trash2} label={t('notetype.remove_template', { name: tpl.name })} onclick={() => draft && (draft.templates = draft.templates.filter((_, j) => j !== i))} />
                    {/if}
                  </div>
                  <div class="sides">
                    <Textarea class="code" rows={4} bind:value={tpl.front} spellcheck="false" label={t('notetype.front')} />
                    <Textarea class="code" rows={4} bind:value={tpl.back} spellcheck="false" label={t('notetype.back')} />
                  </div>
                </div>
              {/each}
            </div>
          </SettingsSection>

          <SettingsSection title={t('notetype.preview_heading')}>
            {#if previewError}<p class="banner" role="alert">{previewError}</p>{/if}
            <CardPreview cards={preview} hideTitle />
          </SettingsSection>

          <SettingsSection title={t('notetype.css')}>
            <Textarea class="code" rows={5} bind:value={draft.css} spellcheck="false" label={t('notetype.css')} hideLabel />
          </SettingsSection>

          <div class="savebar">
            {#if status}<span class="ok" role="status">{status}</span>{/if}
            <Button variant="primary" icon={Save} onclick={() => void save()}>{t('notetype.save')}</Button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</SettingsPage>

{#if confirming && current}
  <ConfirmDialog
    message={t('notetype.confirm_delete', { name: current.name })}
    hint={t('note.undo_hint')}
    confirmLabel={t('common.delete')}
    onconfirm={() => void remove()}
    oncancel={() => (confirming = false)}
  />
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(10rem, 13rem) minmax(0, 1fr);
    gap: var(--space-6);
    align-items: start;
  }
  nav {
    position: sticky;
    top: 0;
  }
  .side-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0;
    padding: 0 var(--space-2) var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .types {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .type {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    height: 30px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  .type:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .type.active {
    background: var(--surface-3);
    color: var(--text-1);
    font-weight: var(--weight-medium);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .type:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .tname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    flex: none;
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    min-width: 0;
  }
  .ftable {
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  .fields {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .frow {
    display: grid;
    grid-template-columns: 1.5rem minmax(7rem, 1.4fr) minmax(7rem, 1fr) repeat(3, 4.5rem) 88px;
    gap: var(--space-2);
    align-items: center;
    min-height: var(--row-h);
    padding: 3px var(--space-2);
    border-top: 1px solid var(--border);
  }
  .frow :global(.fc-field) {
    min-width: 0;
  }
  .fhead {
    min-height: 28px;
    border-top: none;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
  }
  .n {
    color: var(--text-2);
    font-size: var(--text-xs);
    text-align: end;
  }
  .c {
    display: flex;
    justify-content: center;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acts {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
  }
  .classes {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-2);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .muted {
    color: var(--text-2);
  }
  .syntax {
    margin: 0 0 var(--space-4);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
  .syntax code {
    font-family: var(--font-mono);
  }
  .templates {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .tpl {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .tpl-head {
    display: flex;
    gap: var(--space-3);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .tpl-head :global(.fc-field) {
    flex: 1 1 12rem;
  }
  .sides {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: var(--space-3);
  }
  .editor :global(.code textarea),
  .editor :global(textarea.code) {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  /*
   * Sticky save bar: a real bar (top rule, page background, space above it) that reserves its
   * own room at the end of the editor, so the last section scrolls fully clear of it.
   */
  .savebar {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-4);
    padding: var(--space-3) 0 var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--surface-0);
  }
  .ok {
    color: var(--success);
    font-size: var(--text-sm);
  }
  .banner {
    margin: 0 0 var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .syntax code {
    font-variant-ligatures: none;
  }
  @media (max-width: 1023.98px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    nav {
      position: static;
    }
    .types {
      flex-direction: row;
      flex-wrap: wrap;
    }
    .type {
      width: auto;
    }
  }
</style>
