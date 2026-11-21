<!--
  Settings → General → Shortcuts: every shortcut grouped by where it works. "Change" records
  the next key combination (Esc cancels); the backend stores and validates it.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ShortcutList } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, EmptyState, IconButton, Input, Kbd, Keyboard, RotateCcw, Search, TriangleAlert, X, Icon } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import { comboOf, filterItems } from './palette';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ShortcutList | null>(null);
  let failed = $state(false);
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
  /** Rows grouped by context, in first-seen order. */
  const groups = $derived.by(() => {
    const out: { context: string; rows: typeof rows }[] = [];
    for (const r of rows) {
      let g = out.find((x) => x.context === r.context);
      if (!g) out.push((g = { context: r.context, rows: [] }));
      g.rows.push(r);
    }
    return out;
  });

  async function load() {
    failed = !(await ctx.guarded(async () => (list = await api.call('shortcuts.list', {}))));
  }

  async function set(action: string, combo: string | null) {
    recording = null;
    await ctx.guarded(async () => (list = await api.call('shortcuts.set', { action, combo })));
  }

  const reset = (action: string | null) => ctx.guarded(async () => (list = await api.call('shortcuts.reset', { action })));

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
    const unsub = api.on(['setting_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<SettingsPage title={t('shortcuts.heading')} subtitle={t('shortcuts.intro')}>
  {#snippet actions()}
    <Button icon={RotateCcw} onclick={() => void reset(null)} disabled={!list}>{t('shortcuts.reset_all')}</Button>
  {/snippet}

  <div class="filter">
    <Input type="search" icon={Search} bind:value={filter} label={t('shortcuts.filter')} hideLabel placeholder={t('shortcuts.filter_placeholder')} />
  </div>

  {#if list && list.conflicts.length > 0}
    <div class="conflicts" role="alert">
      <Icon icon={TriangleAlert} size={20} />
      <div>
        <strong>{t('shortcuts.conflicts')}</strong>
        <ul>
          {#each list.conflicts as c (c.combo)}<li><Kbd keys={c.combo} /> {c.actions.join(', ')}</li>{/each}
        </ul>
      </div>
    </div>
  {/if}

  {#if !list}
    <SettingsSection>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={6} />
    </SettingsSection>
  {:else if groups.length === 0}
    <SettingsSection>
      <EmptyState size="sm" icon={Keyboard} title={t('shortcuts.empty')} action={{ label: t('shortcuts.clear_filter'), onclick: () => (filter = '') }} />
    </SettingsSection>
  {:else}
    {#each groups as g (g.context)}
      <SettingsSection title={td(`shortcuts.context.${g.context}`)} body="rows">
        {#each g.rows as r (r.action)}
          {@const name = label(r.label_key, r.action)}
          <FormRow label={name}>
            {#if recording === r.action}
              <!-- svelte-ignore a11y_autofocus -->
              <button
                type="button"
                class="rec"
                autofocus
                aria-label={t('shortcuts.record_for', { action: name })}
                onkeydown={(e) => onRecord(e, r.action)}
                onblur={() => (recording = null)}>{t('shortcuts.record')}</button
              >
            {:else}
              <button type="button" class="combo" aria-label={t('shortcuts.change_for', { action: name })} onclick={() => (recording = r.action)}>
                {#if r.combo}<Kbd keys={r.combo} />{:else}<span class="unbound">{t('shortcuts.unbound_text')}</span>{/if}
              </button>
            {/if}
            {#if r.combo !== r.default_combo}
              <Button variant="ghost" size="sm" icon={RotateCcw} onclick={() => void reset(r.action)}>{t('shortcuts.reset')}</Button>
            {/if}
            {#if r.combo}
              <IconButton size="sm" icon={X} label={t('shortcuts.unbind_for', { action: name })} onclick={() => void set(r.action, null)} />
            {/if}
          </FormRow>
        {/each}
      </SettingsSection>
    {/each}
  {/if}
</SettingsPage>

<style>
  .filter {
    max-width: 22rem;
  }
  .conflicts {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-card);
    background: var(--warning-soft);
    color: var(--warning);
    font-size: var(--text-sm);
  }
  .conflicts strong {
    color: var(--text-1);
  }
  .conflicts ul {
    margin: var(--space-1) 0 0;
    padding: 0;
    list-style: none;
    color: var(--text-1);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .combo,
  .rec {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 7rem;
    height: var(--control-h-sm, 32px);
    padding: 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .combo:hover {
    border-color: var(--border-strong);
  }
  .rec {
    border-color: var(--accent-text);
    background: var(--accent-soft);
    color: var(--text-1);
  }
  .combo:focus-visible,
  .rec:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .unbound {
    color: var(--text-2);
  }
</style>
