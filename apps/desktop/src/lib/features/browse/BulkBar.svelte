<!--
  Bulk actions of the Search screen: a bar over the table while cards are selected. Every
  action is one backend command (`bulk.*`, `notes.delete`); the parent refreshes afterwards.
-->
<script lang="ts">
  import { api } from '$lib/api';
  import type { AttributeDefDto, BulkResult, FlagDto, NoteTypeDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Checkbox,
    Dialog,
    Flag,
    FolderInput,
    Icon,
    Input,
    Menu,
    Select,
    Tag,
    Trash2,
    X,
    type MenuItem,
  } from '$lib/ui';

  interface Props {
    count: number;
    cards: string[];
    notes: string[];
    decks: { id: string; path: string }[];
    flags: FlagDto[];
    attrs: AttributeDefDto[];
    noteTypes: NoteTypeDto[];
    /** Runs one bulk call; resolves to whether it succeeded (the parent shows the outcome). */
    bulk: (fn: () => Promise<BulkResult>) => Promise<boolean>;
    /** Runs any other call through the shell's error handling, then refreshes. */
    run: (fn: () => Promise<unknown>) => Promise<boolean>;
    onclear: () => void;
    /** Opens the delete dialog (also from the keyboard shortcut). */
    deleting?: boolean;
  }
  let { count, cards, notes, decks, flags, attrs, noteTypes, bulk, run, onclear, deleting = $bindable(false) }: Props = $props();

  type Which = 'move' | 'tags' | 'attr' | 'find' | 'notetype' | null;
  let which = $state<Which>(null);
  let open = $state(false);
  $effect(() => {
    if (!open) which = null;
  });
  function show(w: Which) {
    which = w;
    open = true;
  }

  // Inputs of the dialogs.
  let moveTo = $state<string | null>(null);
  let tag = $state('');
  let attrId = $state<string | null>(null);
  let attrValue = $state<string>('');
  let find = $state('');
  let replace = $state('');
  let field = $state('');
  let regex = $state(false);
  let matchCase = $state(false);
  let changeTo = $state<string | null>(null);

  const FLAGS = [1, 2, 3, 4, 5, 6, 7] as const;
  const flagLabel = (n: number): string => flags.find((f) => f.flag === n)?.name ?? td(`browse.flags.${n}`);
  const enumAttrs = $derived(attrs.filter((a) => a.options.length > 0));
  const attr = $derived(enumAttrs.find((a) => a.id === attrId) ?? null);

  async function done(ok: Promise<boolean>) {
    if (await ok) open = false;
  }

  function attrPayload() {
    if (!attr || attrValue === '') return null;
    return attr.kind === 'multi_enum' ? { type: 'choices' as const, value: [attrValue] } : { type: 'choice' as const, value: attrValue };
  }

  const flagItems = $derived<MenuItem[]>([
    ...FLAGS.map((f) => ({
      id: `flag-${f}`,
      label: flagLabel(f),
      onselect: () => void bulk(() => api.call('bulk.set_flag', { card_ids: cards, flag: f, on: true })),
    })),
    { id: 'sep', separator: true },
    { id: 'flag-clear', label: t('browse.bulk.flag_clear'), onselect: () => void bulk(() => api.call('bulk.set_flag', { card_ids: cards, flag: 0, on: false })) },
  ]);

  const moreItems = $derived<MenuItem[]>([
    { id: 'suspend', label: t('browse.bulk.suspend'), onselect: () => void bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: true })) },
    { id: 'unsuspend', label: t('browse.bulk.unsuspend'), onselect: () => void bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: false })) },
    { id: 'bury', label: t('browse.bulk.bury'), onselect: () => void bulk(() => api.call('bulk.set_buried', { card_ids: cards, buried: true })) },
    { id: 'unbury', label: t('browse.bulk.unbury'), onselect: () => void bulk(() => api.call('bulk.set_buried', { card_ids: cards, buried: false })) },
    { id: 'reset', label: t('browse.bulk.reset'), onselect: () => void bulk(() => api.call('bulk.reset_progress', { card_ids: cards })) },
    { id: 'sep1', separator: true },
    ...(enumAttrs.length > 0 ? [{ id: 'attr', label: `${t('browse.bulk.classification')}…`, onselect: () => show('attr') }] : []),
    { id: 'find', label: `${t('browse.bulk.find_replace')}…`, onselect: () => show('find') },
    { id: 'notetype', label: `${t('browse.bulk.change_notetype')}…`, onselect: () => show('notetype') },
  ]);

  const title = $derived(
    which === 'move'
      ? t('browse.bulk.move')
      : which === 'tags'
        ? t('browse.bulk.tags')
        : which === 'attr'
          ? t('browse.bulk.classification')
          : which === 'find'
            ? t('browse.bulk.find_replace')
            : t('browse.bulk.change_notetype'),
  );
</script>

<div class="bar" role="toolbar" aria-label={t('browse.bulk.heading')}>
  <span class="count" aria-live="polite">{t('browse.selected', { count })}</span>
  <Button variant="ghost" size="sm" icon={X} onclick={onclear}>{t('browse.clear_selection')}</Button>
  <span class="sep" aria-hidden="true"></span>
  <Button size="sm" icon={FolderInput} onclick={() => show('move')} data-testid="open-dialog-move">{t('browse.bulk.move_short')}</Button>
  <Button size="sm" icon={Tag} onclick={() => show('tags')}>{t('browse.bulk.tags')}</Button>
  <Menu items={flagItems} label={t('browse.bulk.flag')} triggerClass="bar-trigger">
    {#snippet trigger()}<span class="trig"><Icon icon={Flag} size={16} />{t('browse.bulk.flag')}</span>{/snippet}
  </Menu>
  <Menu items={moreItems} label={t('browse.bulk.more')} />
  <span class="grow"></span>
  <Button size="sm" variant="danger" icon={Trash2} onclick={() => (deleting = true)}>{t('browse.bulk.delete')}</Button>
</div>

<Dialog bind:open {title} size="sm">
  {#if which === 'move'}
    <Select label={t('browse.col.deck')} options={decks.map((d) => ({ value: d.id, label: d.path }))} bind:value={moveTo} placeholder={t('browse.bulk.choose_deck')} />
  {:else if which === 'tags'}
    <Input label={t('browse.bulk.tag')} bind:value={tag} placeholder={t('browse.bulk.tag_placeholder')} />
  {:else if which === 'attr'}
    <div class="stack">
      <Select label={t('browse.bulk.classification')} options={enumAttrs.map((a) => ({ value: a.id, label: a.name }))} bind:value={attrId} />
      {#if attr}
        <Select
          label={t('browse.bulk.value')}
          options={[{ value: '', label: t('browse.bulk.clear_value') }, ...attr.options.map((o) => ({ value: o.id, label: o.label }))]}
          bind:value={attrValue}
        />
      {/if}
    </div>
  {:else if which === 'find'}
    <div class="stack">
      <Input label={t('browse.bulk.find')} bind:value={find} />
      <Input label={t('browse.bulk.replace')} bind:value={replace} />
      <Input label={t('browse.bulk.field')} bind:value={field} placeholder={t('browse.bulk.all_fields')} />
      <Checkbox label={t('browse.bulk.regex')} bind:checked={regex} />
      <Checkbox label={t('browse.bulk.match_case')} bind:checked={matchCase} />
    </div>
  {:else if which === 'notetype'}
    <div class="stack">
      <Select label={t('browse.bulk.notetype')} options={noteTypes.map((n) => ({ value: n.id, label: n.name }))} bind:value={changeTo} />
      <p class="help">{t('browse.bulk.notetype_help')}</p>
    </div>
  {/if}

  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    {#if which === 'move'}
      <Button variant="primary" disabled={!moveTo} onclick={() => done(bulk(() => api.call('bulk.move_cards', { card_ids: cards, deck_id: moveTo! })))}>
        {t('browse.bulk.apply')}
      </Button>
    {:else if which === 'tags'}
      <Button disabled={!tag.trim()} onclick={() => done(bulk(() => api.call('bulk.remove_tags', { note_ids: notes, tags: [tag] })))}>
        {t('browse.bulk.remove_tag')}
      </Button>
      <Button variant="primary" disabled={!tag.trim()} onclick={() => done(bulk(() => api.call('bulk.add_tags', { note_ids: notes, tags: [tag] })))}>
        {t('browse.bulk.add_tag')}
      </Button>
    {:else if which === 'attr'}
      <Button
        variant="primary"
        disabled={!attr}
        onclick={() => done(bulk(() => api.call('bulk.set_attribute', { card_ids: cards, attr_id: attrId!, value: attrPayload() })))}
      >
        {t('browse.bulk.apply')}
      </Button>
    {:else if which === 'find'}
      <Button
        variant="primary"
        disabled={!find}
        onclick={() =>
          done(
            bulk(() =>
              api.call('bulk.find_replace', { note_ids: notes, find, replace, field: field.trim() || null, regex, match_case: matchCase }),
            ),
          )}
      >
        {t('browse.bulk.apply')}
      </Button>
    {:else if which === 'notetype'}
      <Button
        variant="primary"
        disabled={!changeTo}
        onclick={() =>
          done(
            run(async () => {
              const r = await api.call('bulk.change_notetype', { note_ids: notes, notetype_id: changeTo!, field_map: null, template_map: null });
              return t('browse.bulk.notetype_done', { notes: r.notes, kept: r.cards_kept, trashed: r.cards_trashed, created: r.cards_created });
            }),
          )}
      >
        {t('browse.bulk.apply')}
      </Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    border-radius: var(--radius-card);
  }
  .count {
    font-weight: var(--weight-semibold);
    font-size: var(--text-sm);
    padding-inline: var(--space-1);
  }
  .sep {
    width: 1px;
    align-self: stretch;
    background: var(--border-strong);
    margin-inline: var(--space-1);
  }
  .bar :global(button.bar-trigger) {
    height: var(--control-h-sm);
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
    font-size: var(--text-xs);
  }
  .trig {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-weight: var(--weight-medium);
  }
  .grow {
    flex: 1;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .help {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
