<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, InboxItemDto, InboxSource, InboxStatus } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Card, Check, Checkbox, Chip, EmptyState, Globe, Icon, Input, PageHeader, Select, Skeleton, Switch, Tabs, X } from '$lib/ui';
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
  let allowDuplicates = $state(false);
  let notice = $state<string | null>(null);
  let loaded = $state(false);
  let pendingCount = $state<number | null>(null);

  const paths = $derived(deckPaths(ctx.decks));
  const picked = $derived(items.filter((i) => selected[i.id]));

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
    });
    loaded = true;
    void api.call('inbox.count', {}).then((c) => (pendingCount = c.pending), () => {});
  }

  function edited(i: InboxItemDto): boolean {
    const d = drafts[i.id];
    return !!d && (d.front !== (i.item.front ?? '') || d.back !== (i.item.back ?? ''));
  }

  async function accept(list: InboxItemDto[]) {
    notice = null;
    const ok = await ctx.guarded(async () => {
      const r = await api.call('inbox.accept', {
        items: list.map((i) => ({
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
    if (ok) await load();
  }

  async function reject(list: InboxItemDto[]) {
    notice = null;
    const ok = await ctx.guarded(async () => {
      const r = await api.call('inbox.reject', { ids: list.map((i) => i.id) });
      notice = t('inbox.rejected', { count: r.changed });
    });
    if (ok) await load();
  }

  function show(s: string) {
    tab = s as InboxStatus;
    loaded = false;
    void load();
  }

  const tabs = $derived([
    { id: 'pending', label: t('inbox.tabs.pending'), badge: pendingCount },
    { id: 'accepted', label: t('inbox.tabs.accepted') },
    { id: 'rejected', label: t('inbox.tabs.rejected') },
  ]);
  const deckOptions = $derived(paths.map((p) => ({ value: p.id as string | null, label: p.path })));
  const date = (ms: number) => new Date(ms).toLocaleDateString();

  onMount(() => {
    void load();
    const unsub = api.on(['inbox_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="inbox">
  <PageHeader title={t('inbox.heading')} subtitle={t('inbox.intro')} />

  <Tabs label={t('inbox.heading')} {tabs} value={tab} onchange={show} />

  {#if notice}<p class="notice" role="status"><Icon icon={Check} size={16} />{notice}</p>{/if}

  {#if !loaded}
    <div class="list" aria-busy="true">
      {#each [0, 1, 2] as i (i)}<Skeleton shape="card" height="140px" />{/each}
    </div>
  {:else if items.length === 0}
    <Card padding="lg">
      <EmptyState
        icon={tab === 'pending' ? Check : undefined}
        title={tab === 'pending' ? t('inbox.empty') : t('inbox.empty_other')}
        description={tab === 'pending' ? t('inbox.empty_hint') : undefined}
      />
    </Card>
  {:else}
    {#if tab === 'pending'}
      <div class="bulk">
        <Checkbox
          label={t('inbox.select_all')}
          checked={picked.length === items.length}
          indeterminate={picked.length > 0 && picked.length < items.length}
          onchange={(on) => (selected = Object.fromEntries(items.map((i) => [i.id, on])))}
        />
        <span class="grow"></span>
        <Switch label={t('inbox.allow_duplicates')} bind:checked={allowDuplicates} />
        <Button size="sm" disabled={picked.length === 0} icon={X} onclick={() => reject(picked)}>
          {t('inbox.reject_selected', { count: picked.length })}
        </Button>
        <Button size="sm" variant="primary" icon={Check} disabled={picked.length === 0} onclick={() => accept(picked)}>
          {t('inbox.accept_selected', { count: picked.length })}
        </Button>
      </div>
    {/if}

    <ul class="list">
      {#each items as item (item.id)}
        {@const d = drafts[item.id]}
        <li class="item" class:picked={selected[item.id]}>
          <div class="meta">
            {#if tab === 'pending'}
              <Checkbox ariaLabel={item.item.front ?? item.id} bind:checked={selected[item.id]} />
            {/if}
            <Chip size="sm" tone="accent" label={sourceLabel(item.source)} />
            {#if item.item.context?.url}
              <span class="from"><Icon icon={Globe} size={16} />{t('inbox.from_page')}: {item.item.context.title || item.item.context.url}</span>
            {/if}
            <span class="grow"></span>
            <span class="date">{date(item.created_at)}</span>
          </div>

          {#if tab === 'pending' && d}
            <div class="pair">
              <Input label={t('inbox.front')} bind:value={d.front} />
              <Input label={t('inbox.back')} bind:value={d.back} />
            </div>
          {:else}
            <div class="pair read">
              <p class="front">{item.item.front}</p>
              <p class="back">{item.item.back}</p>
            </div>
          {/if}

          {#if item.item.context?.sentence}
            <p class="sentence">{t('inbox.sentence')}: “{item.item.context.sentence}”</p>
          {/if}
          {#if Object.keys(item.item.attributes ?? {}).length > 0 || (item.item.tags ?? []).length > 0}
            <div class="chips">
              {#each Object.entries(item.item.attributes ?? {}) as [k, v] (k)}
                <Chip size="sm" label={`${k}: ${attrText(v)}`} />
              {/each}
              {#each item.item.tags ?? [] as tag (tag)}<Chip size="sm" label={`#${tag}`} />{/each}
            </div>
          {/if}

          {#if tab === 'pending' && d}
            <div class="foot">
              <Select size="sm" label={t('inbox.deck')} hideLabel options={deckOptions} bind:value={d.deck} placeholder={t('inbox.choose_deck')} class="deck" />
              <span class="grow"></span>
              <Button variant="ghost" onclick={() => reject([item])}>{t('inbox.reject')}</Button>
              <Button variant="primary" onclick={() => accept([item])}>{t('inbox.accept')}</Button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .inbox {
    max-width: 880px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .inbox :global(.fc-page-header) {
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
  .bulk {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .grow {
    flex: 1;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-1);
  }
  .item.picked {
    border-color: var(--accent);
  }
  .meta,
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .from,
  .date {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-2);
    font-size: var(--text-sm);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .date {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-4);
  }
  .pair.read p {
    margin: 0;
  }
  .front {
    font-weight: var(--weight-semibold);
    font-size: var(--text-md);
  }
  .back {
    color: var(--text-2);
  }
  .sentence {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    font-style: italic;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .foot {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .foot :global(.deck) {
    width: 260px;
  }
  @media (max-width: 720px) {
    .pair {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
