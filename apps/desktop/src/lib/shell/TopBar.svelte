<!--
  The 44 px top bar (v1.2 brief "Shell"), always present except in a running study session:
  where you are (section › page › what the screen appended, `PageChrome.setTrail`) on the
  left; the search-or-command field (opens the command palette) and "Add card" (Quick Add,
  `N`) on the right. It only draws: App.svelte decides what the parts do.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import Kbd from '$lib/ui/Kbd.svelte';
  import { ChevronRight, Plus, Search } from '$lib/ui/icons';
  import type { Crumb } from '$lib/ui/types';

  interface Props {
    trail: Crumb[];
    /** Opens the command palette (or the search page when the palette is off). */
    onsearch: (() => void) | null;
    /** Key hint of the search field (`Ctrl+K`). */
    searchKeys: string;
    /** Opens Quick Add; `null` hides the button (nothing can add cards). */
    onadd: (() => void) | null;
  }

  let { trail, onsearch, searchKeys, onadd }: Props = $props();
</script>

<header class="fc-topbar" data-testid="top-bar">
  <nav class="trail" aria-label={t('shell.topbar.trail')}>
    <ol>
      {#each trail as c, i (i)}
        {@const last = i === trail.length - 1}
        <li class:last>
          {#if i > 0}<span class="sep" aria-hidden="true"><Icon icon={ChevronRight} size={14} /></span>{/if}
          {#if c.onclick && !last}
            <button type="button" class="crumb link" onclick={c.onclick}>{c.label}</button>
          {:else}
            <span class="crumb" aria-current={last ? 'page' : undefined}>{c.label}</span>
          {/if}
        </li>
      {/each}
    </ol>
  </nav>
  <div class="tools" role="group" aria-label={t('shell.topbar.label')}>
    {#if onsearch}
      <button type="button" class="search" onclick={onsearch} aria-label={t('shell.topbar.search')} aria-keyshortcuts={searchKeys.replace('Mod', 'Control')} data-testid="top-search">
        <Icon icon={Search} size={14} />
        <span class="placeholder" aria-hidden="true">{t('shell.topbar.search')}</span>
        <Kbd keys={searchKeys} size="sm" />
      </button>
    {/if}
    {#if onadd}
      <Button variant="primary" size="sm" icon={Plus} kbd="N" aria-keyshortcuts="N" title={t('shell.topbar.add_card_hint')} onclick={onadd} data-testid="quick-add-open">
        {t('shell.topbar.add_card')}
      </Button>
    {/if}
  </div>
</header>

<style>
  .fc-topbar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex: none;
    height: var(--topbar-h);
    padding: 0 var(--space-3) 0 var(--content-pad-x);
    border-bottom: 1px solid var(--border);
    background: var(--surface-0);
  }
  .trail {
    flex: 1;
    min-width: 0;
  }
  ol {
    display: flex;
    align-items: center;
    min-width: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    min-width: 0;
    flex: none;
  }
  li.last {
    flex: 0 1 auto;
  }
  .sep {
    display: inline-flex;
    margin: 0 2px;
    color: var(--text-3);
  }
  .crumb {
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .last .crumb {
    color: var(--text-1);
  }
  button.crumb {
    height: 24px;
    padding: 0 4px;
    border: none;
    border-radius: 4px;
    background: transparent;
    cursor: pointer;
  }
  button.crumb:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  li:first-child button.crumb {
    margin-inline-start: -4px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 280px;
    height: var(--control-h-sm);
    padding: 0 4px 0 var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-3);
    font-size: var(--text-sm);
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease);
  }
  .search:hover {
    border-color: var(--border-strong);
  }
  .search:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .placeholder {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text-2);
    text-align: start;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Narrow window: the field shrinks to its icon and key hint (the name stays its label). */
  @media (max-width: 1023.98px) {
    .search {
      width: auto;
    }
    .search .placeholder {
      display: none;
    }
  }
</style>
