<!--
  The deck page's term list (v1.2): a dense table — term | definition, classification (level)
  chips, mastery, flags — with row actions on hover/focus (⚑ flag menu, edit). Inline edit:
  the pencil or a double-click turns the row into two fields; Ctrl+Enter saves, Esc cancels.
  "Add card" opens Quick Add on this deck. Rows come from `notes.terms`; an edit is a
  `notes.update` of the row's `term_field` / `definition_field` (v1.3: chosen by field role,
  so "Definition" is the meaning, never the surface form), a flag `cards.set_flag`. A row of
  a note type without roles is `ambiguous`: editing it opens the full note editor instead of
  guessing which fields the two columns are.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { api } from '$lib/api';
  import type { FlagDto, NoteTerm } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Check, Chip, EmptyState, Flag, IconButton, List, Menu, Pencil, Plus, Skeleton, Textarea, type MenuItem } from '$lib/ui';
  import { defaultFlagKey, flagBit, hasFlag } from '../cards/flags';
  import { masteryTone } from './library';
  import NoteEditDialog from '../cards/NoteEditDialog.svelte';

  interface Props {
    terms: NoteTerm[];
    status: 'loading' | 'ready' | 'failed';
    flags: FlagDto[];
    flagsOn: boolean;
    hasMore: boolean;
    loadingMore: boolean;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
    onmore: () => void;
    onretry: () => void;
    onadd: (() => void) | null;
    onchange: (row: NoteTerm) => void;
  }
  let { terms, status, flags, flagsOn, hasMore, loadingMore, guarded, onmore, onretry, onadd, onchange }: Props = $props();

  let editing = $state<string | null>(null);
  let front = $state('');
  let back = $state('');
  let saving = $state(false);
  let editRow = $state<HTMLElement | null>(null);
  /** The note open in the full editor (ambiguous rows) and the deck it is shown with. */
  let fullEdit = $state<{ row: NoteTerm; deckId: string } | null>(null);

  const flagName = (f: FlagDto) => f.name ?? td(defaultFlagKey(f.flag));
  const MASTERY = { new: 'deck_page.mastery_new', learning: 'deck_page.mastery_learning', mastered: 'deck_page.mastery_mastered' } as const;
  const masteryLabel = (row: NoteTerm) => t(MASTERY[row.mastery]);

  async function edit(row: NoteTerm) {
    if (row.ambiguous) {
      await guarded(async () => {
        const note = await api.call('notes.get', { id: row.note_id });
        fullEdit = { row, deckId: note.home_deck_id };
      });
      return;
    }
    editing = row.note_id;
    front = row.front.value;
    back = row.back?.value ?? '';
    await tick();
    editRow?.querySelector('textarea')?.focus();
  }

  async function save(row: NoteTerm) {
    if (saving) return;
    saving = true;
    const ok = await guarded(async () => {
      const note = await api.call('notes.get', { id: row.note_id });
      const fields = { ...note.fields, [row.term_field]: front };
      if (row.definition_field) fields[row.definition_field] = back;
      await api.call('notes.update', { id: row.note_id, fields, tags: note.tags, attributes: note.attributes });
    });
    saving = false;
    if (!ok) return;
    onchange({
      ...row,
      front: { ...row.front, value: front, text: front },
      back: row.back ? { ...row.back, value: back, text: back } : null,
    });
    editing = null;
  }

  /** After the full editor closes: the row shows what is stored now. */
  async function closeFullEdit() {
    const row = fullEdit?.row;
    fullEdit = null;
    if (!row) return;
    await guarded(async () => {
      const note = await api.call('notes.get', { id: row.note_id });
      const cell = (f: { key: string } | null) => {
        if (!f) return null;
        const value = note.fields[f.key] ?? '';
        return { key: f.key, value, text: plain(value) };
      };
      onchange({ ...row, front: cell(row.front)!, back: cell(row.back) });
    });
  }

  /** Visible text of a stored value (tags stripped), as `notes.terms` sends it. Parsed in an
   * inert document: nothing in the value loads or runs. */
  function plain(value: string): string {
    return new DOMParser().parseFromString(value, 'text/html').body.textContent ?? '';
  }

  function onEditKey(e: KeyboardEvent, row: NoteTerm) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      editing = null;
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save(row);
    }
  }

  async function toggle(row: NoteTerm, flag: number) {
    const on = !hasFlag(row.flags, flag);
    await guarded(async () => {
      await api.call('cards.set_flag', { ids: row.card_ids, flag, on });
      onchange({ ...row, flags: on ? row.flags | flagBit(flag) : row.flags & ~flagBit(flag) });
    });
  }

  function flagItems(row: NoteTerm): MenuItem[] {
    return flags.map((f) => ({
      id: `flag-${f.flag}`,
      label: flagName(f),
      icon: hasFlag(row.flags, f.flag) ? Check : Flag,
      onselect: () => void toggle(row, f.flag),
    }));
  }
</script>

{#if fullEdit}
  <NoteEditDialog noteId={fullEdit.row.note_id} deckId={fullEdit.deckId} deckName="" onclose={() => void closeFullEdit()} />
{/if}

<section class="terms" aria-labelledby="deck-terms" data-testid="term-list">
  <div class="head">
    <h2 id="deck-terms" class="title">
      {t('deck_page.terms')}
      {#if status === 'ready'}<span class="count num">{formatCount(terms.length)}{hasMore ? '+' : ''}</span>{/if}
    </h2>
    {#if onadd && status === 'ready' && terms.length > 0}
      <Button size="sm" icon={Plus} kbd="N" onclick={onadd}>{t('deck_page.add_card')}</Button>
    {/if}
  </div>

  {#if status === 'loading'}
    <div class="table">
      {#each [0, 1, 2, 3] as i (i)}
        <div class="row skeleton"><Skeleton shape="text" /><Skeleton shape="text" /></div>
      {/each}
    </div>
  {:else if status === 'failed'}
    <div class="failed" role="alert">
      <span>{t('deck_page.terms_failed')}</span>
      <Button size="sm" variant="ghost" onclick={onretry}>{t('deck_page.retry')}</Button>
    </div>
  {:else if terms.length === 0}
    <div class="table empty">
      <EmptyState
        size="sm"
        icon={List}
        title={t('deck_page.terms_empty')}
        action={onadd ? { label: t('deck_page.terms_empty_action'), icon: Plus, onclick: onadd } : undefined}
      />
    </div>
  {:else}
    <div class="table">
      <div class="row header" aria-hidden="true">
        <span>{t('deck_page.term')}</span>
        <span>{t('deck_page.definition')}</span>
        <span class="end">{t('deck_page.state')}</span>
        <span></span>
      </div>
      <ul class="list">
        {#each terms as row (row.note_id)}
          {#if editing === row.note_id}
            <li class="row editing" bind:this={editRow}>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="edit" onkeydown={(e) => onEditKey(e, row)}>
                <Textarea label={t('deck_page.edit_front')} hideLabel bind:value={front} autoGrow rows={1} maxRows={6} />
                {#if row.back}
                  <Textarea label={t('deck_page.edit_back')} hideLabel bind:value={back} autoGrow rows={1} maxRows={6} />
                {:else}
                  <span></span>
                {/if}
                <div class="edit-actions">
                  <span class="edit-hint">{t('deck_page.edit_hint')}</span>
                  <Button variant="ghost" size="sm" onclick={() => (editing = null)}>{t('common.cancel')}</Button>
                  <Button variant="primary" size="sm" kbd="Mod+Enter" loading={saving} onclick={() => void save(row)}>{t('common.save')}</Button>
                </div>
              </div>
            </li>
          {:else}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <li class="row" ondblclick={() => void edit(row)}>
              <span class="front">{row.front.text}</span>
              <span class="back">{row.back?.text ?? ''}</span>
              <span class="tags">
                {#if flagsOn}
                  {#each flags.filter((f) => hasFlag(row.flags, f.flag)) as f (f.flag)}
                    <span class="flag" style:--flag={f.color}><Chip size="sm" icon={Flag} label={flagName(f)} /></span>
                  {/each}
                {/if}
                {#each row.chips as chip (chip.attr_id + chip.label)}
                  <Chip size="sm" label={chip.label} />
                {/each}
                <Chip size="sm" tone={masteryTone[row.mastery]} label={masteryLabel(row)} />
              </span>
              <span class="actions">
                {#if flagsOn && flags.length}
                  <Menu items={flagItems(row)} label={t('deck_page.flags')} icon={Flag} align="end" triggerClass="term-menu" />
                {/if}
                <IconButton size="sm" icon={Pencil} label={t('deck_page.edit', { term: row.front.text })} onclick={() => void edit(row)} />
              </span>
            </li>
          {/if}
        {/each}
      </ul>
    </div>
    {#if hasMore}
      <div class="foot">
        <Button size="sm" variant="ghost" loading={loadingMore} onclick={onmore}>{t('deck_page.load_more')}</Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .terms {
    scroll-margin-top: var(--space-4);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-2);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .count {
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
  }
  .table {
    container-type: inline-size;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  /* No overflow clipping (the flag menu opens past the table): round the last row instead. */
  .list .row:last-child {
    border-radius: 0 0 calc(var(--radius-card) - 1px) calc(var(--radius-card) - 1px);
  }
  .table.empty {
    padding: var(--space-6) var(--space-4);
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 3fr) 216px 64px;
    align-items: center;
    gap: var(--space-4);
    min-height: var(--row-h);
    padding: 2px var(--space-2) 2px var(--space-3);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .list .row:last-child {
    border-bottom: 0;
  }
  .list .row:not(.editing):hover,
  .list .row:focus-within {
    background: var(--surface-2);
  }
  .row.header {
    min-height: 30px;
    padding-block: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .end {
    text-align: end;
  }
  .row.skeleton {
    grid-template-columns: 1fr 1fr;
  }
  .row.editing {
    display: block;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    background: var(--surface-2);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .front {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }
  .back {
    color: var(--text-1);
    overflow-wrap: anywhere;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-1);
  }
  .flag :global(svg) {
    color: var(--flag);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--motion-fast) var(--ease);
  }
  .row:hover .actions,
  .row:focus-within .actions {
    opacity: 1;
  }
  @media (hover: none) {
    .actions {
      opacity: 1;
    }
  }
  .actions :global(.term-menu) {
    width: var(--control-h-sm);
    height: var(--control-h-sm);
  }
  .edit {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
    gap: var(--space-2) var(--space-4);
  }
  .edit-actions {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .edit-hint {
    margin-inline-end: auto;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .foot {
    display: flex;
    justify-content: center;
    margin-top: var(--space-2);
  }
  .failed {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--danger-soft);
    color: var(--danger);
  }
  /* Narrow tables: the chips go on a second line under term | definition. */
  @container (max-width: 640px) {
    .row {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 64px;
      row-gap: var(--space-1);
    }
    .row.header > :nth-child(3) {
      display: none;
    }
    .tags {
      grid-column: 1 / 3;
      grid-row: 2;
      justify-content: flex-start;
    }
    .actions {
      grid-row: 1;
      grid-column: 3;
    }
  }
</style>
