<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, InboxItemDto, InboxSource, InboxStatus } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { t } from '$lib/i18n/index.svelte';
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
      selected = {};
    });
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

  function show(s: InboxStatus) {
    tab = s;
    void load();
  }

  onMount(() => {
    void load();
    const unsub = api.on(['inbox_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<section>
  <h1>{t('inbox.heading')}</h1>
  <p class="muted">{t('inbox.intro')}</p>

  <div class="tabs" role="tablist">
    {#each ['pending', 'accepted', 'rejected'] as const as s (s)}
      <button role="tab" aria-selected={tab === s} class:active={tab === s} onclick={() => show(s)}>
        {t(`inbox.tabs.${s}`)}
      </button>
    {/each}
  </div>

  {#if notice}<p role="status">{notice}</p>{/if}

  {#if items.length === 0}
    <p class="muted">{t('inbox.empty')}</p>
  {:else}
    {#if tab === 'pending'}
      <div class="bulk">
        <label>
          <input
            type="checkbox"
            checked={picked.length === items.length}
            onchange={(e) => {
              const on = e.currentTarget.checked;
              selected = Object.fromEntries(items.map((i) => [i.id, on]));
            }}
          />
          {t('inbox.select_all')}
        </label>
        <button disabled={picked.length === 0} onclick={() => accept(picked)}>
          {t('inbox.accept_selected', { count: picked.length })}
        </button>
        <button disabled={picked.length === 0} onclick={() => reject(picked)}>
          {t('inbox.reject_selected', { count: picked.length })}
        </button>
        <label>
          <input type="checkbox" bind:checked={allowDuplicates} />
          {t('inbox.allow_duplicates')}
        </label>
      </div>
    {/if}

    <ul class="items">
      {#each items as item (item.id)}
        {@const d = drafts[item.id]}
        <li>
          <div class="meta">
            {#if tab === 'pending'}
              <input type="checkbox" aria-label={item.item.front ?? item.id} bind:checked={selected[item.id]} />
            {/if}
            <span class="source">{sourceLabel(item.source)}</span>
            {#if item.item.context?.url}
              <span class="muted">{t('inbox.from_page')}: {item.item.context.title || item.item.context.url}</span>
            {/if}
          </div>
          {#if tab === 'pending' && d}
            <div class="fields">
              <label>{t('inbox.front')} <input bind:value={d.front} /></label>
              <label>{t('inbox.back')} <input bind:value={d.back} /></label>
              <label>
                {t('inbox.deck')}
                <select bind:value={d.deck}>
                  <option value={null}>{t('inbox.choose_deck')}</option>
                  {#each paths as p (p.id)}<option value={p.id}>{p.path}</option>{/each}
                </select>
              </label>
            </div>
          {:else}
            <p><strong>{item.item.front}</strong> — {item.item.back}</p>
          {/if}
          {#if item.item.context?.sentence}
            <p class="muted">{t('inbox.sentence')}: “{item.item.context.sentence}”</p>
          {/if}
          {#if Object.keys(item.item.attributes ?? {}).length > 0 || (item.item.tags ?? []).length > 0}
            <p class="chips">
              {#each Object.entries(item.item.attributes ?? {}) as [k, v] (k)}
                <span class="chip">{k}: {attrText(v)}</span>
              {/each}
              {#each item.item.tags ?? [] as tag (tag)}<span class="chip">#{tag}</span>{/each}
            </p>
          {/if}
          {#if tab === 'pending'}
            <div class="actions">
              <button onclick={() => accept([item])}>{t('inbox.accept')}</button>
              <button onclick={() => reject([item])}>{t('inbox.reject')}</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .tabs,
  .bulk,
  .actions,
  .meta,
  .fields {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .tabs {
    margin-bottom: var(--fc-space-4);
  }
  .tabs button.active {
    background: var(--fc-selected);
  }
  .items {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  .items li {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius, 6px);
    padding: var(--fc-space-3);
  }
  .source {
    font-weight: 600;
  }
  .chip {
    display: inline-block;
    padding: 0 var(--fc-space-2);
    margin-right: var(--fc-space-1);
    border-radius: 999px;
    background: var(--fc-bg-subtle);
  }
</style>
