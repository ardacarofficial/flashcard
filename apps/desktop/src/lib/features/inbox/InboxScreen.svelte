<!--
  Inbox (v1.2): proposed cards as a dense review list. Each row shows the front / back
  preview, where it came from and the target deck; accept / edit / reject are on the row and
  on the keyboard (row focus: ↑↓ or J/K move, A accept, E or Enter edit, R or Delete reject,
  X or Space select, Ctrl+Enter accept the selection). The toolbar keeps bulk accept / reject.
-->
<script lang="ts">
  import { formatDate } from '$lib/format';
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, InboxItemDto, InboxSource, InboxStatus } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Check,
    Checkbox,
    Chip,
    EmptyState,
    Globe,
    Icon,
    IconButton,
    Input,
    Kbd,
    PageHeader,
    Pencil,
    Select,
    Skeleton,
    Switch,
    Tabs,
    X,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  interface Draft {
    front: string;
    back: string;
    deck: DeckId | null;
  }

  let tab = $state<InboxStatus>('pending');
  let items = $state<InboxItemDto[]>([]);
  let drafts = $state<Record<string, Draft>>({});
  let selected = $state<Record<string, boolean>>({});
  let editing = $state<Record<string, boolean>>({});
  let active = $state(0);
  let allowDuplicates = $state(false);
  let notice = $state<string | null>(null);
  let loaded = $state(false);
  let pendingCount = $state<number | null>(null);
  let list = $state<HTMLUListElement | null>(null);

  const paths = $derived(deckPaths(ctx.decks));
  const picked = $derived(items.filter((i) => selected[i.id]));
  const pending = $derived(tab === 'pending');

  function sourceLabel(s: InboxSource): string {
    switch (s.kind) {
      case 'local_api':
        return t('inbox.source.local_api', { name: s.client_name });
      case 'other':
        return t('inbox.source.other', { actor: s.actor });
      default:
        return t(`inbox.source.${s.kind}`);
    }
  }

  const attrText = (v: unknown) => (Array.isArray(v) ? v.join(', ') : String(v));

  async function load() {
    await ctx.guarded(async () => {
      const page = await api.call('inbox.list', { status: tab, page: { cursor: null, limit: 200 } });
      items = page.items;
      const next: Record<string, Draft> = {};
      for (const i of items) {
        next[i.id] = drafts[i.id] ?? {
          front: i.item.front ?? '',
          back: i.item.back ?? '',
          deck: i.item.deck_id ?? ctx.selectedDeck,
        };
      }
      drafts = next;
      selected = Object.fromEntries(items.map((i) => [i.id, false]));
      editing = Object.fromEntries(items.filter((i) => editing[i.id]).map((i) => [i.id, true]));
      active = Math.min(active, Math.max(0, items.length - 1));
    });
    loaded = true;
    void api.call('inbox.count', {}).then((c) => (pendingCount = c.pending), () => {});
  }

  function edited(i: InboxItemDto): boolean {
    const d = drafts[i.id];
    return !!d && (d.front !== (i.item.front ?? '') || d.back !== (i.item.back ?? ''));
  }

  async function accept(chosen: InboxItemDto[]) {
    if (chosen.length === 0) return;
    notice = null;
    const ok = await ctx.guarded(async () => {
      const r = await api.call('inbox.accept', {
        items: chosen.map((i) => ({
          id: i.id,
          deck_id: drafts[i.id]?.deck ?? null,
          notetype_id: null,
          // Edited text replaces the proposal (Classic front/back).
          fields: edited(i) ? { front: drafts[i.id]!.front, back: drafts[i.id]!.back } : null,
          tags: null,
          attributes: null,
        })),
        allow_duplicates: allowDuplicates,
      });
      notice = t('inbox.accepted', { count: r.accepted.length });
    });
    if (ok) await reloadKeepingFocus();
  }

  async function reject(chosen: InboxItemDto[]) {
    if (chosen.length === 0) return;
    notice = null;
    const ok = await ctx.guarded(async () => {
      const r = await api.call('inbox.reject', { ids: chosen.map((i) => i.id) });
      notice = t('inbox.rejected', { count: r.changed });
    });
    if (ok) await reloadKeepingFocus();
  }

  /** After a row went away, the next one takes the keyboard focus (if the list had it). */
  async function reloadKeepingFocus() {
    const hadFocus = !!list && list.contains(document.activeElement);
    await load();
    if (hadFocus) await focusRow(active);
  }

  function show(s: string) {
    tab = s as InboxStatus;
    loaded = false;
    active = 0;
    void load();
  }

  async function focusRow(i: number) {
    if (items.length === 0) return;
    active = Math.max(0, Math.min(items.length - 1, i));
    await tick();
    list?.querySelector<HTMLElement>(`[data-row="${active}"]`)?.focus();
  }

  async function startEdit(i: number) {
    const item = items[i];
    if (!item || !pending) return;
    editing[item.id] = true;
    active = i;
    await tick();
    list?.querySelector<HTMLInputElement>(`[data-row="${i}"] .pair input`)?.focus();
  }

  async function stopEdit(i: number) {
    const item = items[i];
    if (!item) return;
    editing[item.id] = false;
    await focusRow(i);
  }

  /** Keys on a focused row (not while typing in its fields or using its controls). */
  function onRowKey(e: KeyboardEvent, i: number) {
    const item = items[i]!;
    const onRow = e.target === e.currentTarget;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      void accept(picked.length > 0 && onRow ? picked : [item]);
      return;
    }
    if (!onRow) {
      if (editing[item.id] && (e.key === 'Escape' || e.key === 'Enter') && (e.target as HTMLElement).tagName === 'INPUT') {
        e.preventDefault();
        e.stopPropagation();
        void stopEdit(i);
      }
      return;
    }
    if (mod || e.altKey) return;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    let handled = true;
    if (key === 'ArrowDown' || key === 'j') void focusRow(i + 1);
    else if (key === 'ArrowUp' || key === 'k') void focusRow(i - 1);
    else if (key === 'Home') void focusRow(0);
    else if (key === 'End') void focusRow(items.length - 1);
    else if (!pending) handled = false;
    else if (key === 'a') void accept([item]);
    else if (key === 'r' || key === 'Delete') void reject([item]);
    else if (key === 'e' || key === 'Enter') void startEdit(i);
    else if (key === 'x' || key === ' ') selected[item.id] = !selected[item.id];
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  /** From nowhere in particular (page focus): J / ↓ enters the list, Ctrl+Enter accepts the selection. */
  function onWindowKey(e: KeyboardEvent) {
    if (e.defaultPrevented || !loaded || items.length === 0) return;
    const target = e.target as HTMLElement | null;
    if (target && target !== document.body && !target.classList.contains('fc-inbox')) return;
    if (pending && (e.ctrlKey || e.metaKey) && e.key === 'Enter' && picked.length > 0) {
      e.preventDefault();
      void accept(picked);
    } else if (!e.ctrlKey && !e.metaKey && !e.altKey && (e.key === 'j' || e.key === 'ArrowDown')) {
      e.preventDefault();
      void focusRow(active);
    }
  }

  const tabs = $derived([
    { id: 'pending', label: t('inbox.tabs.pending'), badge: pendingCount },
    { id: 'accepted', label: t('inbox.tabs.accepted') },
    { id: 'rejected', label: t('inbox.tabs.rejected') },
  ]);
  const deckOptions = $derived(paths.map((p) => ({ value: p.id as string | null, label: p.path })));

  onMount(() => {
    void load();
    const unsub = api.on(['inbox_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="fc-inbox">
  <PageHeader title={t('inbox.heading')} subtitle={t('inbox.intro')} />

  <Tabs label={t('inbox.heading')} {tabs} value={tab} onchange={show} />

  {#if notice}<p class="notice" role="status"><Icon icon={Check} size={14} />{notice}</p>{/if}

  {#if !loaded}
    <div class="skeleton" aria-busy="true">
      {#each [0, 1, 2, 3] as i (i)}<Skeleton shape="control" height="44px" />{/each}
    </div>
  {:else if items.length === 0}
    <div class="empty">
      <EmptyState
        size="sm"
        icon={pending ? Check : undefined}
        title={pending ? t('inbox.empty') : t('inbox.empty_other')}
        description={pending ? t('inbox.empty_hint') : undefined}
      />
    </div>
  {:else}
    <div class="panel">
      {#if pending}
        <div class="bulk">
          <Checkbox
            label={t('inbox.select_all')}
            checked={picked.length === items.length}
            indeterminate={picked.length > 0 && picked.length < items.length}
            onchange={(on) => (selected = Object.fromEntries(items.map((i) => [i.id, on])))}
          />
          <span class="grow"></span>
          <Switch label={t('inbox.allow_duplicates')} bind:checked={allowDuplicates} />
          <Button size="sm" variant="ghost" disabled={picked.length === 0} icon={X} onclick={() => reject(picked)}>
            {t('inbox.reject_selected', { count: picked.length })}
          </Button>
          <Button
            size="sm"
            variant="primary"
            icon={Check}
            kbd={picked.length > 0 ? 'Mod+Enter' : undefined}
            aria-keyshortcuts="Control+Enter"
            disabled={picked.length === 0}
            onclick={() => accept(picked)}
          >
            {t('inbox.accept_selected', { count: picked.length })}
          </Button>
        </div>
      {/if}

      <ul class="list" bind:this={list} aria-label={t('inbox.heading')}>
        {#each items as item, i (item.id)}
          {@const d = drafts[item.id]}
          {@const isEditing = pending && !!d && !!editing[item.id]}
          <!-- Roving focus: the active row is the list's one tab stop; its keys are listed in the footer. -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <li
            class="item"
            class:active={i === active}
            class:picked={selected[item.id]}
            class:read={!pending}
            data-row={i}
            tabindex={i === active ? 0 : -1}
            aria-label={(d?.front || item.item.front) ?? item.id}
            aria-keyshortcuts={pending ? 'A E R X' : undefined}
            onfocusin={() => (active = i)}
            onkeydown={(e) => onRowKey(e, i)}
          >
            {#if pending}
              <span class="check"><Checkbox ariaLabel={item.item.front ?? item.id} bind:checked={selected[item.id]} /></span>
            {/if}

            <div class="main">
              {#if isEditing && d}
                <div class="pair edit">
                  <Input size="sm" label={t('inbox.front')} bind:value={d.front} />
                  <Input size="sm" label={t('inbox.back')} bind:value={d.back} />
                </div>
              {:else}
                <div class="pair">
                  <span class="front">{pending && d ? d.front : item.item.front}</span>
                  <span class="back">{pending && d ? d.back : item.item.back}</span>
                </div>
              {/if}
              <div class="meta">
                <span class="source">{sourceLabel(item.source)}</span>
                {#if pending && edited(item)}<Chip size="sm" tone="accent" label={t('inbox.edited')} />{/if}
                {#if item.item.context?.url}
                  <span class="from"><Icon icon={Globe} size={12} />{t('inbox.from_page')}: {item.item.context.title || item.item.context.url}</span>
                {/if}
                {#each Object.entries(item.item.attributes ?? {}) as [k, v] (k)}
                  <Chip size="sm" label={`${k}: ${attrText(v)}`} />
                {/each}
                {#each item.item.tags ?? [] as tag (tag)}<Chip size="sm" label={`#${tag}`} />{/each}
                <span class="date num">{formatDate(item.created_at)}</span>
              </div>
              {#if item.item.context?.sentence}
                <p class="sentence">{t('inbox.sentence')}: “{item.item.context.sentence}”</p>
              {/if}
            </div>

            {#if pending && d}
              <div class="side">
                <Select size="sm" label={t('inbox.deck')} hideLabel options={deckOptions} bind:value={d.deck} placeholder={t('inbox.choose_deck')} class="deck" />
                <div class="acts">
                  {#if isEditing}
                    <IconButton size="sm" icon={Check} label={t('inbox.done')} onclick={() => void stopEdit(i)} />
                  {:else}
                    <IconButton size="sm" icon={Pencil} label={t('inbox.edit')} aria-keyshortcuts="E" onclick={() => void startEdit(i)} />
                  {/if}
                  <IconButton size="sm" icon={X} label={t('inbox.reject')} aria-keyshortcuts="R" onclick={() => reject([item])} />
                  <Button
                    size="sm"
                    variant={i === active ? 'primary' : 'secondary'}
                    kbd={i === active ? 'A' : undefined}
                    aria-keyshortcuts="A"
                    onclick={() => accept([item])}>{t('inbox.accept')}</Button
                  >
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>

      <p class="keys" aria-label={t('inbox.keys.label')}>
        <span><Kbd size="sm" keys={['ArrowUp']} /><Kbd size="sm" keys={['ArrowDown']} /> {t('inbox.keys.move')}</span>
        {#if pending}
          <span><Kbd size="sm" keys="A" /> {t('inbox.keys.accept')}</span>
          <span><Kbd size="sm" keys="E" /> {t('inbox.keys.edit')}</span>
          <span><Kbd size="sm" keys="R" /> {t('inbox.keys.reject')}</span>
          <span><Kbd size="sm" keys="X" /> {t('inbox.keys.select')}</span>
          <span><Kbd size="sm" keys="Mod+Enter" /> {t('inbox.keys.accept_selected')}</span>
        {/if}
      </p>
    </div>
  {/if}
</div>

<style>
  .fc-inbox {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .fc-inbox :global(.fc-page-header) {
    margin-bottom: 0;
  }
  .notice {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--success);
    font-size: var(--text-sm);
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .empty,
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .empty {
    padding: var(--space-4);
  }
  .bulk {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    min-height: 44px;
    padding: var(--space-1) var(--space-3) var(--space-1) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .grow {
    flex: 1;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .item {
    position: relative;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: start;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border);
    outline: none;
  }
  .item.read {
    grid-template-columns: minmax(0, 1fr);
  }
  .item:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  .item.active {
    background: var(--surface-2);
  }
  .item.active::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 2px;
    background: var(--accent);
  }
  .item:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .item.picked {
    background: var(--accent-soft);
  }
  .check {
    display: flex;
    align-items: center;
    height: 28px;
  }
  .main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-4);
    align-items: baseline;
    min-height: 28px;
    padding-top: 4px;
  }
  .pair.edit {
    align-items: end;
    padding-top: 0;
    gap: var(--space-2);
  }
  .front,
  .back {
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: var(--text-sm);
  }
  .front {
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .back {
    color: var(--text-2);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
    min-height: 22px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .source {
    color: var(--accent-text);
    font-weight: var(--weight-medium);
  }
  .from {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .date {
    margin-left: auto;
    color: var(--text-2);
  }
  .sentence {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-style: italic;
  }
  .side {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .side :global(.deck) {
    width: 200px;
  }
  .acts {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    width: 11rem;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: 0;
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .keys span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  @media (max-width: 1023.98px) {
    .item {
      grid-template-columns: auto minmax(0, 1fr);
    }
    .side {
      grid-column: 2;
      justify-content: space-between;
    }
  }
</style>
