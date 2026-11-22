<!--
  Keyboard shortcuts help (v1.3): every shortcut that is bound right now, grouped by where it
  works, with a search field. The table is the backend's (`shortcuts.list`, re-read on every
  open, so a key just changed in the Shortcuts screen shows here at once); nothing is listed
  from this file. Opened by `?`, the top bar's "?" and the palette (HelpHost).
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/api';
  import type { ShortcutList } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Dialog, EmptyState, Input, Kbd, Keyboard, Search } from '$lib/ui';
  import { LoadState } from '../core/kit';
  import { boundCount, helpGroups } from './help';
  import { pageKeys } from '$lib/shell/page-keys.svelte';

  interface Props {
    open: boolean;
    /** Opens the Shortcuts screen; `null` hides the link (the `palette` module is off). */
    openShortcuts: (() => void) | null;
  }
  let { open = $bindable(false), openShortcuts }: Props = $props();

  let list = $state<ShortcutList | null>(null);
  let failed = $state(false);
  let query = $state('');

  /** As the Shortcuts screen: the label's translation, else the action id. */
  const label = (key: string, action: string) => {
    const s = td(key);
    return s === key ? action : s;
  };
  /** The Shortcuts screen's context names; an `editor` or newer context gets its own or its id. */
  function contextTitle(context: string): string {
    for (const key of [`shortcuts.context.${context}`, `help.context.${context}`]) {
      const s = td(key);
      if (s !== key) return s;
    }
    return context;
  }

  const groups = $derived(list ? helpGroups(list, label, query) : []);
  /** The page's own keys (Home's Enter, Create's Ctrl+Enter …), filtered by the search too. */
  const pageRows = $derived(
    pageKeys.list
      .map((k) => ({ ...k, text: t(k.label_key) }))
      .filter((k) => !query.trim() || k.text.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())),
  );
  const anyBound = $derived(!!list && boundCount(list) > 0);

  async function load() {
    failed = false;
    try {
      list = await api.call('shortcuts.list', {});
    } catch {
      failed = true;
    }
  }

  // Fresh table and empty search on every open.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      query = '';
      void load();
    });
  });

  function edit() {
    open = false;
    openShortcuts?.();
  }
</script>

<Dialog bind:open title={t('help.title')} initialFocus="input[type='search']">
  <div class="search">
    <Input type="search" size="sm" icon={Search} bind:value={query} label={t('help.search')} hideLabel placeholder={t('help.search_placeholder')} />
  </div>

  <!-- Fixed minimum height: the dialog does not jump while the search narrows the list. -->
  <div class="results">
    {#if !list}
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={6} />
    {:else if !anyBound}
      <EmptyState size="sm" icon={Keyboard} title={t('help.none')} />
    {:else if groups.length === 0 && pageRows.length === 0}
      <EmptyState size="sm" icon={Search} title={t('help.empty')} action={{ label: t('help.clear'), onclick: () => (query = '') }} />
    {:else}
      <div class="groups" data-testid="help-groups">
        {#if pageRows.length}
          <section aria-labelledby="fc-help-page" data-testid="help-page-keys">
            <h3 id="fc-help-page">{t('help.on_this_page')}</h3>
            <ul>
              {#each pageRows as r (r.label_key)}
                <li>
                  <span class="label">{r.text}</span>
                  <Kbd keys={r.keys} size="sm" />
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        {#each groups as g (g.context)}
          <section aria-labelledby="fc-help-{g.context}">
            <h3 id="fc-help-{g.context}">{contextTitle(g.context)}</h3>
            <ul>
              {#each g.rows as r (r.action)}
                <li data-action={r.action}>
                  <span class="label">{r.label}</span>
                  <Kbd keys={r.combo} size="sm" />
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <p class="note"><Kbd keys="?" size="sm" /> <span>{t('help.note')}</span></p>
    {#if openShortcuts}
      <Button size="sm" icon={Keyboard} onclick={edit}>{t('help.edit')}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  /* Sticky over the scrolling list; the top padding keeps the focus ring inside the scroll box. */
  .search {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: var(--space-1) 0 var(--space-3);
    background: var(--surface-1);
  }
  .results {
    min-height: min(30rem, 60vh);
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h3 {
    margin: 0 0 var(--space-1);
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 32px;
    padding: 0 var(--space-2) 0 var(--space-3);
    font-size: var(--text-sm);
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    color: var(--text-1);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li :global(.fc-kbd) {
    flex: none;
  }
  .note {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0 auto 0 0;
    min-width: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
</style>
