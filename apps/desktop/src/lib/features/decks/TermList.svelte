<!--
  The deck page's term list (v1.1, docs/11 §3): term | definition in two columns, mastery and
  level chips, flags (⚑ menu), inline edit (Enter on the pencil; Ctrl+Enter saves, Esc
  cancels) and "Add card". Rows come from `notes.terms`; an edit is a `notes.update`.
-->
<script lang="ts">
  import { api } from '$lib/api';
  import type { FlagDto, NoteTerm } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Card, Check, Chip, EmptyState, Flag, IconButton, List, Menu, Pencil, Plus, Skeleton, Textarea, type MenuItem } from '$lib/ui';
  import { defaultFlagKey, flagBit, hasFlag } from '../cards/flags';
  import { masteryTone } from './library';

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

  const flagName = (f: FlagDto) => f.name ?? td(defaultFlagKey(f.flag));
  const MASTERY = { new: 'deck_page.mastery_new', learning: 'deck_page.mastery_learning', mastered: 'deck_page.mastery_mastered' } as const;
  const masteryLabel = (row: NoteTerm) => t(MASTERY[row.mastery]);

  function edit(row: NoteTerm) {
    editing = row.note_id;
    front = row.front.value;
    back = row.back?.value ?? '';
  }

  async function save(row: NoteTerm) {
    if (saving) return;
    saving = true;
    const ok = await guarded(async () => {
      const note = await api.call('notes.get', { id: row.note_id });
      const fields = { ...note.fields, [row.front.key]: front };
      if (row.back) fields[row.back.key] = back;
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

  function onEditKey(e: KeyboardEvent, row: NoteTerm) {
    if (e.key === 'Escape') {
      e.preventDefault();
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

<section aria-labelledby="deck-terms" data-testid="term-list">
  <div class="head">
    <h2 id="deck-terms" class="section-title">
      {t('deck_page.terms')}
      {#if status === 'ready'}<span class="count">{terms.length}{hasMore ? '+' : ''}</span>{/if}
    </h2>
    {#if onadd && status === 'ready' && terms.length > 0}
      <Button icon={Plus} onclick={onadd}>{t('deck_page.add_card')}</Button>
    {/if}
  </div>

  {#if status === 'loading'}
    <Card padding="none">
      {#each [0, 1, 2, 3] as i (i)}
        <div class="row skeleton"><Skeleton shape="text" /><Skeleton shape="text" /></div>
      {/each}
    </Card>
  {:else if status === 'failed'}
    <Card padding="md" class="failed">
      <p role="alert">{t('deck_page.terms_failed')}</p>
      <Button size="sm" onclick={onretry}>{t('deck_page.retry')}</Button>
    </Card>
  {:else if terms.length === 0}
    <Card padding="lg">
      <EmptyState
        size="sm"
        icon={List}
        title={t('deck_page.terms_empty')}
        action={onadd ? { label: t('deck_page.terms_empty_action'), icon: Plus, onclick: onadd } : undefined}
      />
    </Card>
  {:else}
    <Card padding="none">
      <div class="row header" aria-hidden="true">
        <span>{t('deck_page.term')}</span>
        <span class="back">{t('deck_page.definition')}</span>
      </div>
      <ul class="list">
        {#each terms as row (row.note_id)}
          <li class="row" class:editing={editing === row.note_id}>
            {#if editing === row.note_id}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="edit" onkeydown={(e) => onEditKey(e, row)}>
                <Textarea label={t('deck_page.edit_front')} bind:value={front} autoGrow rows={1} maxRows={6} />
                {#if row.back}
                  <Textarea label={t('deck_page.edit_back')} bind:value={back} autoGrow rows={1} maxRows={6} />
                {/if}
                <div class="edit-actions">
                  <Button variant="ghost" size="sm" onclick={() => (editing = null)}>{t('common.cancel')}</Button>
                  <Button variant="primary" size="sm" loading={saving} onclick={() => void save(row)}>{t('common.save')}</Button>
                </div>
              </div>
            {:else}
              <span class="front">{row.front.text}</span>
              <span class="back">{row.back?.text ?? ''}</span>
              <span class="tags">
                {#each row.chips as chip (chip.attr_id + chip.label)}
                  <Chip size="sm" label={chip.label} />
                {/each}
                <Chip size="sm" tone={masteryTone[row.mastery]} label={masteryLabel(row)} />
                {#if flagsOn}
                  {#each flags.filter((f) => hasFlag(row.flags, f.flag)) as f (f.flag)}
                    <span class="flag" style:--flag={f.color}><Chip size="sm" icon={Flag} label={flagName(f)} /></span>
                  {/each}
                {/if}
              </span>
              <span class="actions">
                {#if flagsOn && flags.length}
                  <Menu items={flagItems(row)} label={t('deck_page.flags')} icon={Flag} align="end" />
                {/if}
                <IconButton icon={Pencil} label={t('deck_page.edit', { term: row.front.text })} onclick={() => edit(row)} />
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    </Card>
    {#if hasMore}
      <div class="foot">
        <Button variant="secondary" loading={loadingMore} onclick={onmore}>{t('deck_page.load_more')}</Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }
  .section-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .count {
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
    color: var(--text-2);
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 3fr) 208px 72px;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .list .row:last-child {
    border-bottom: 0;
  }
  .row.header {
    padding-block: var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
    background: var(--surface-2);
  }
  .row.skeleton {
    grid-template-columns: 1fr 1fr;
  }
  .row.editing {
    display: block;
    background: var(--surface-2);
  }
  .front {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }
  .back {
    color: var(--text-1);
    overflow-wrap: anywhere;
    padding-inline-start: var(--space-4);
    border-inline-start: 1px solid var(--border);
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
    gap: var(--space-1);
  }
  .row.header .back {
    border-color: transparent;
  }
  .edit {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  .edit-actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .foot {
    display: flex;
    justify-content: center;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }
  :global(.failed.fc-card) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  :global(.failed.fc-card) p {
    margin: 0;
    color: var(--danger);
  }
  @media (max-width: 1023.98px) {
    .row {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 72px;
    }
    .tags {
      grid-column: 1 / 3;
      justify-content: flex-start;
    }
    .actions {
      grid-row: 1;
      grid-column: 3;
    }
  }
</style>
