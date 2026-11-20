<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AttributeDefDto, CardTypeDto, FieldKind, NoteTypeDto, NoteTypeInput, RenderedSides } from '$lib/api/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import CardPreview from './CardPreview.svelte';
  import { move, newField, sampleFields, toInput } from './content';
  import { moduleKnownOn, trackModules } from './modules.svelte';

  let { ctx: _ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: FieldKind[] = ['rich_text', 'text', 'image', 'audio', 'video', 'number', 'tag_list', 'link'];

  let list = $state<NoteTypeDto[]>([]);
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

  const current = $derived(list.find((n) => n.id === selected) ?? null);

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey);
  }

  async function load() {
    try {
      list = (await api.call('notetypes.list', {})).notetypes;
      if (!list.some((n) => n.id === selected)) selected = list[0]?.id ?? null;
    } catch (e) {
      fail(e);
    }
  }

  $effect(() => {
    trackModules();
    void load();
    api.call('cardtypes.list', {}).then((r) => (engines = r.card_types), fail);
    const unsub = api.on(['note_type_changed', 'undone', 'redone'], () => void load());
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

<section class="layout">
  <aside>
    <h2>{t('notetype.heading')}</h2>
    <ul>
      {#each list as n (n.id)}
        <li>
          <button class:active={n.id === selected} onclick={() => (selected = n.id)}>
            {n.name}
            {#if n.builtin}<span class="muted small-text">· {t('notetype.builtin')}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    <div class="row">
      <button onclick={() => void create(null)}>{t('notetype.new')}</button>
      {#if current}<button onclick={() => void create(current.id)}>{t('notetype.copy')}</button>{/if}
    </div>
  </aside>

  {#if draft && current}
    <div class="edit">
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <label>
        {t('notetype.name')}
        <input bind:value={draft.name} />
      </label>
      <p class="muted">{t('notetype.usage', { notes: usage })}</p>

      <h3>{t('notetype.fields')}</h3>
      <table>
        <thead>
          <tr>
            <th>{t('notetype.field_name')}</th>
            <th>{t('notetype.field_kind')}</th>
            <th>{t('notetype.required')}</th>
            <th>{t('notetype.unique')}</th>
            <th>{t('notetype.searchable')}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each draft.fields as f, i (i)}
            <tr>
              <td><input bind:value={f.name} aria-label={t('notetype.field_name')} /></td>
              <td>
                <select bind:value={f.kind} aria-label={t('notetype.field_kind')}>
                  {#each KINDS as k (k)}<option value={k}>{t(`notetype.kind.${k}`)}</option>{/each}
                </select>
              </td>
              <td><input type="checkbox" bind:checked={f.required} aria-label={t('notetype.required')} /></td>
              <td><input type="checkbox" bind:checked={f.unique} aria-label={t('notetype.unique')} /></td>
              <td><input type="checkbox" bind:checked={f.searchable} aria-label={t('notetype.searchable')} /></td>
              <td class="row">
                <button class="small" aria-label={t('notetype.move_up')} onclick={() => draft && (draft.fields = move(draft.fields, i, -1))}>↑</button>
                <button class="small" aria-label={t('notetype.move_down')} onclick={() => draft && (draft.fields = move(draft.fields, i, 1))}>↓</button>
                <button class="small" aria-label={t('notetype.remove')} onclick={() => draft && (draft.fields = draft.fields.filter((_, j) => j !== i))}>✕</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="row">
        <button onclick={() => draft && (draft.fields = [...draft.fields, newField(t('notetype.new_field'))])}>{t('notetype.add_field')}</button>
        <label class="inline">
          {t('notetype.sort_field')}
          <select bind:value={draft.sort_field}>
            {#each draft.fields.filter((f) => f.key) as f (f.key)}<option value={f.key}>{f.name}</option>{/each}
          </select>
        </label>
      </div>

      {#if classifications.length > 0}
        <div class="classes">
          <strong>{t('notetype.classifications')}</strong>
          <span>{classifications.map((c) => c.name).join(' · ')}</span>
          <span class="muted">{t('notetype.classifications_hint')}</span>
        </div>
      {/if}

      <h3>{t('notetype.templates')}</h3>
      <p class="muted syntax">{t('notetype.syntax')}</p>
      {#each draft.templates as tpl, i (i)}
        <fieldset>
          <div class="row">
            <label class="inline">
              {t('notetype.template_name')}
              <input bind:value={tpl.name} />
            </label>
            <label class="inline">
              {t('notetype.card_type')}
              <select bind:value={tpl.card_type}>
                {#each engines as e (e.id)}<option value={e.id}>{td(e.name_key)}</option>{/each}
              </select>
            </label>
            {#if draft.templates.length > 1}
              <button class="small" onclick={() => draft && (draft.templates = draft.templates.filter((_, j) => j !== i))}>{t('notetype.remove')}</button>
            {/if}
          </div>
          <label>
            {t('notetype.front')}
            <textarea rows="3" bind:value={tpl.front} spellcheck="false"></textarea>
          </label>
          <label>
            {t('notetype.back')}
            <textarea rows="3" bind:value={tpl.back} spellcheck="false"></textarea>
          </label>
        </fieldset>
      {/each}
      {#if draft.kind === 'standard'}
        <button
          onclick={() =>
            draft &&
            (draft.templates = [
              ...draft.templates,
              { key: '', name: t('notetype.new_template'), front: '', back: '{{FrontSide}}\n<hr>\n', card_type: 'classic' },
            ])}
        >
          {t('notetype.add_template')}
        </button>
      {/if}

      <label>
        {t('notetype.css')}
        <textarea rows="4" bind:value={draft.css} spellcheck="false"></textarea>
      </label>

      <div class="row">
        <button class="primary" onclick={() => void save()}>{t('notetype.save')}</button>
        {#if status}<span class="ok" role="status">{status}</span>{/if}
        <span class="spacer"></span>
        <button class="danger" onclick={() => (confirming = true)}>{t('notetype.delete')}</button>
      </div>
    </div>

    <div class="preview">
      {#if previewError}<p class="error">{previewError}</p>{/if}
      <CardPreview cards={preview} />
    </div>
  {/if}
</section>

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
    grid-template-columns: 200px minmax(360px, 1.4fr) minmax(240px, 1fr);
    gap: var(--fc-space-6);
    align-items: start;
  }
  aside ul {
    list-style: none;
    margin: 0 0 var(--fc-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  aside button {
    width: 100%;
    text-align: left;
  }
  aside button.active {
    background: var(--fc-selected);
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  label.inline {
    flex-direction: row;
    align-items: center;
    gap: var(--fc-space-2);
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  table {
    border-collapse: collapse;
  }
  td,
  th {
    padding: 2px var(--fc-space-1);
    text-align: left;
  }
  fieldset {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    padding: var(--fc-space-3);
  }
  textarea {
    font-family: ui-monospace, monospace;
  }
  .syntax {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }
  .classes {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
    padding: var(--fc-space-2);
    background: var(--fc-bg-subtle);
    border-radius: var(--fc-radius);
  }
  .small-text {
    font-size: 0.85em;
  }
  .ok {
    color: var(--fc-success);
  }
  .preview {
    position: sticky;
    top: 0;
  }
</style>
