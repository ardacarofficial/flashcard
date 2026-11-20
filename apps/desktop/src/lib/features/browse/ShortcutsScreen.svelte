<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ShortcutList } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { comboOf, filterItems } from './palette';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ShortcutList | null>(null);
  let filter = $state('');
  let recording = $state<string | null>(null);

  const label = (key: string, action: string) => {
    const s = td(key);
    return s === key ? action : s;
  };
  const rows = $derived.by(() => {
    if (!list) return [];
    const byId = new Map(list.items.map((i) => [i.action, i]));
    const items = list.items.map((i) => ({
      kind: 'screen' as const,
      id: i.action,
      label: label(i.label_key, i.action),
      screenId: i.action,
    }));
    return filterItems(items, filter).map((x) => byId.get(x.id)!);
  });

  async function load() {
    await ctx.guarded(async () => (list = await api.call('shortcuts.list', {})));
  }

  async function set(action: string, combo: string | null) {
    recording = null;
    await ctx.guarded(async () => (list = await api.call('shortcuts.set', { action, combo })));
  }

  function onRecord(e: KeyboardEvent, action: string) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      recording = null;
      return;
    }
    const combo = comboOf(e);
    if (combo) void set(action, combo);
  }

  onMount(() => {
    void load();
    const unsub = api.on(['setting_changed', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('shortcuts.heading')}</h2>

<div class="bar">
  <input bind:value={filter} placeholder={t('shortcuts.filter')} aria-label={t('shortcuts.filter')} />
  <button onclick={() => ctx.guarded(async () => (list = await api.call('shortcuts.reset', { action: null })))}>{t('shortcuts.reset_all')}</button>
</div>

{#if list && list.conflicts.length > 0}
  <div class="error" role="alert">
    <strong>{t('shortcuts.conflicts')}</strong>
    <ul>
      {#each list.conflicts as c (c.combo)}<li><kbd>{c.combo}</kbd>: {c.actions.join(', ')}</li>{/each}
    </ul>
  </div>
{/if}

<table>
  <thead>
    <tr><th>{t('shortcuts.action')}</th><th>{t('shortcuts.combo')}</th><th></th></tr>
  </thead>
  <tbody>
    {#each rows as r (r.action)}
      <tr>
        <td>{label(r.label_key, r.action)} <small class="muted">{td(`shortcuts.context.${r.context}`)}</small></td>
        <td>
          {#if recording === r.action}
            <!-- svelte-ignore a11y_autofocus -->
            <input class="rec" readonly autofocus value={t('shortcuts.record')} aria-label={t('shortcuts.record')} onkeydown={(e) => onRecord(e, r.action)} onblur={() => (recording = null)} />
          {:else}
            <kbd>{r.combo ?? t('shortcuts.unbound')}</kbd>
          {/if}
        </td>
        <td class="actions">
          <button onclick={() => (recording = r.action)}>{t('shortcuts.change')}</button>
          {#if r.combo}<button onclick={() => void set(r.action, null)}>{t('shortcuts.unbind')}</button>{/if}
          {#if r.combo !== r.default_combo}
            <button onclick={() => ctx.guarded(async () => (list = await api.call('shortcuts.reset', { action: r.action })))}>{t('shortcuts.reset')}</button>
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .bar {
    display: flex;
    gap: var(--fc-space-2);
    margin-bottom: var(--fc-space-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td,
  th {
    text-align: left;
    padding: var(--fc-space-1) var(--fc-space-2);
    border-bottom: var(--fc-border-width) solid var(--fc-border);
  }
  .actions {
    display: flex;
    gap: var(--fc-space-1);
  }
  .rec {
    width: 10em;
  }
</style>
