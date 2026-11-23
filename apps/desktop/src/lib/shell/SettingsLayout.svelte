<!--
  The Settings page: a search field and the grouped sub-navigation on the left (docs/11 §2),
  the chosen page on the right. Below 1024 px the sub-navigation collapses into one Select
  above the page (the search stays above it).

  v1.4 (prototype `ayarlar.html`): "Basic" pages first; every other group shows only with the
  ONE "Advanced" switch at the bottom of the list (remembered: `app.show_advanced`). Search
  finds advanced pages too and marks them "In Advanced"; opening one while the switch is off
  shows it once, with a band offering to turn Advanced on (A/B 4: the switch stays off).

  Search is presentation only: it filters the pages by their names and texts (the i18n
  subtrees each page shows, `settings-index.ts`) and lights up matching rows on the open
  page (`FormRow`); the first match is scrolled into view when a result is opened.
-->
<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Select from '$lib/ui/Select.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import Button from '$lib/ui/Button.svelte';
  import { Search } from '$lib/ui/icons';
  import { advanced } from './advanced.svelte';
  import { settingsSearch } from '$lib/features/core/kit/search.svelte';
  import { matchPage } from '$lib/features/core/kit/settings-index';
  import type { NavEntry, Navigation } from './navigation';

  interface Props {
    groups: Navigation['settings'];
    active: string | null;
    open: (e: NavEntry) => void;
    children: Snippet;
  }

  let { groups, active, open, children }: Props = $props();
  let content = $state<HTMLElement | null>(null);
  // Leaving Settings ends the search (rows elsewhere must not stay lit).
  onDestroy(() => (settingsSearch.query = ''));

  /** The route the page's texts are indexed under (the General page is a slot, not a route). */
  const routeOf = (e: NavEntry) => e.route ?? (e.screen.id === 'settings' ? '/settings/general' : null);

  const searching = $derived(settingsSearch.active);
  const advancedCount = $derived(groups.filter((g) => g.advanced).reduce((n, g) => n + g.entries.length, 0));
  /** The open page sits in an Advanced group while the switch is off (opened once). */
  const oneOff = $derived(!advanced.on && groups.some((g) => g.advanced && g.entries.some((e) => e.id === active)));
  const shown = $derived.by(() => {
    if (!searching)
      return groups
        .filter((g) => advanced.on || !g.advanced)
        .map((g) => ({ ...g, entries: g.entries.map((e) => ({ e, snippet: null as string | null })) }));
    const q = settingsSearch.query;
    return groups
      .map((g) => ({
        ...g,
        entries: g.entries.flatMap((e) => {
          const m = matchPage(q, td(e.label_key), routeOf(e), i18n.locale);
          return m ? [{ e, snippet: m.snippet }] : [];
        }),
      }))
      .filter((g) => g.entries.length > 0);
  });
  const count = $derived(shown.reduce((n, g) => n + g.entries.length, 0));

  const entries = $derived(shown.flatMap((g) => g.entries.map(({ e }) => ({ e, group: t(g.label_key) }))));
  // The group under each page name, unless it only repeats it ("Plugins" in "Plugins").
  const options = $derived(
    entries.map(({ e, group }) => {
      const label = td(e.label_key);
      return { value: e.id, label, description: label === group ? undefined : group };
    }),
  );
  const tagAdvanced = (g: { advanced: boolean }) => g.advanced && !advanced.on;

  /** Opens a page; while searching, brings its first matching row into view once it rendered. */
  function go(e: NavEntry) {
    open(e);
    if (!searching) return;
    let tries = 0;
    const look = () => {
      const hit = content?.querySelector('[data-search-hit]');
      if (hit) hit.scrollIntoView({ block: 'center', behavior: 'smooth' });
      else if (++tries < 10) setTimeout(look, 150);
    };
    setTimeout(look, 100);
  }

  function onkeydown(ev: KeyboardEvent) {
    if (ev.key === 'Escape' && settingsSearch.query) {
      ev.preventDefault();
      ev.stopPropagation();
      settingsSearch.query = '';
    }
  }
</script>

<div class="settings" class:searching>
  <div class="search" data-testid="settings-search">
    <Input
      type="search"
      size="sm"
      icon={Search}
      label={t('kit.search.label')}
      hideLabel
      placeholder={t('kit.search.placeholder')}
      bind:value={settingsSearch.query}
      {onkeydown}
    />
    {#if searching}
      <p class="count" role="status">
        {count === 0 ? t('kit.search.none', { query: settingsSearch.query.trim() }) : t('kit.search.results', { count })}
      </p>
    {/if}
  </div>
  <div class="picker" data-testid="settings-select">
    {#if options.length > 0}
      <Select
        label={t('shell.settings_nav')}
        hideLabel
        {options}
        value={active}
        onchange={(id) => {
          const hit = entries.find((x) => x.e.id === id);
          if (hit) go(hit.e);
        }}
      />
    {/if}
    <div class="adv-inline"><Switch label={t('shell.advanced.label')} checked={advanced.on} onchange={(v) => void advanced.set(v)} /></div>
  </div>
  <nav class="subnav" aria-label={t('shell.settings_nav')}>
    {#each shown as g (g.id)}
      <div class="group" class:apart={g.advanced && !searching && g === shown.find((x) => x.advanced)} data-testid="settings-group-{g.id}">
        <h2 class="group-title">{t(g.label_key)}</h2>
        <ul>
          {#each g.entries as { e, snippet } (e.id)}
            <li>
              <button
                type="button"
                data-testid="subnav-item"
                class:active={e.id === active}
                class:with-snippet={!!snippet}
                aria-current={e.id === active ? 'page' : undefined}
                onclick={() => go(e)}
              >
                <span class="name"
                  >{td(e.label_key)}{#if tagAdvanced(g)}<span class="adv-tag" data-testid="advanced-tag">{t('shell.advanced.tag')}</span>{/if}</span
                >
                {#if snippet}<span class="snippet">{snippet}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
    {#if !searching}
      <div class="adv-switch" data-testid="advanced-switch">
        <Switch label={t('shell.advanced.label')} checked={advanced.on} onchange={(v) => void advanced.set(v)} />
        <p class="adv-hint">{advanced.on ? t('shell.advanced.on_hint') : t('shell.advanced.off_hint', { count: advancedCount })}</p>
      </div>
    {/if}
  </nav>
  <div class="content" bind:this={content}>
    {#if oneOff}
      <div class="one-off" role="note" data-testid="advanced-one-off">
        <span>{t('shell.advanced.one_off')}</span>
        <Button size="sm" variant="secondary" onclick={() => void advanced.set(true)}>{t('shell.advanced.turn_on')}</Button>
      </div>
    {/if}
    {@render children()}
  </div>
</div>

<style>
  .settings {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    grid-template-rows: auto 1fr;
    gap: var(--space-3) var(--space-8);
    align-items: start;
  }
  .search {
    grid-column: 1;
    grid-row: 1;
    margin-inline-start: -8px;
  }
  .count {
    margin: var(--space-1) 0 0;
    padding: 0 8px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .subnav {
    grid-column: 1;
    grid-row: 2;
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-height: calc(100vh - 44px - var(--space-12) - 40px);
    overflow-y: auto;
    margin-inline-start: -8px;
    padding-inline-end: var(--space-2);
  }
  .group.apart {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .group-title {
    margin: 0 0 4px;
    padding: 0 8px;
    font-size: var(--text-2xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.02em;
    color: var(--text-2);
  }
  .adv-tag {
    margin-inline-start: var(--space-2);
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    vertical-align: 1px;
  }
  .adv-switch {
    margin-top: var(--space-2);
    padding: var(--space-3) 8px 0;
    border-top: 1px solid var(--border);
  }
  .adv-hint {
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .one-off {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  button {
    display: flex;
    flex-direction: column;
    justify-content: center;
    width: 100%;
    min-height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  button.with-snippet {
    padding: 4px 8px;
  }
  .name,
  .snippet {
    display: block;
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .snippet {
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  button:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  button.active {
    background: var(--surface-3);
    color: var(--text-1);
    font-weight: var(--weight-medium);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .content {
    grid-column: 2;
    grid-row: 1 / span 2;
    min-width: 0;
  }
  .picker {
    display: none;
  }
  /* Narrow window: the grouped list becomes one Select on top (no second column). */
  @media (max-width: 1023.98px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto;
      gap: var(--space-3);
    }
    .search {
      max-width: 360px;
      margin-inline-start: 0;
    }
    .subnav {
      display: none;
    }
    .picker {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: var(--space-2) var(--space-4);
    }
    .picker > :global(:first-child) {
      flex: 0 1 360px;
    }
    .content {
      grid-column: 1;
      grid-row: auto;
      margin-top: var(--space-2);
    }
    /* While searching, the results are a list (a Select would hide them). */
    .searching .subnav {
      display: flex;
      position: static;
      max-height: none;
      max-width: 480px;
      margin-inline-start: 0;
    }
    .searching .picker {
      display: none;
    }
  }
</style>
