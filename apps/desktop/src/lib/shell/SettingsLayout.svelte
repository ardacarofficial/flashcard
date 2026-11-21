<!--
  The Settings page: grouped sub-navigation on the left (docs/11 §2), the chosen page on the
  right. Below 1024 px the sub-navigation collapses into one Select above the page.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t, td } from '$lib/i18n/index.svelte';
  import Select from '$lib/ui/Select.svelte';
  import type { NavEntry, Navigation } from './navigation';

  interface Props {
    groups: Navigation['settings'];
    active: string | null;
    open: (e: NavEntry) => void;
    children: Snippet;
  }

  let { groups, active, open, children }: Props = $props();
  const entries = $derived(groups.flatMap((g) => g.entries.map((e) => ({ e, group: t(g.label_key) }))));
  const options = $derived(entries.map(({ e, group }) => ({ value: e.id, label: td(e.label_key), description: group })));
</script>

<div class="settings">
  <div class="picker" data-testid="settings-select">
    <Select
      label={t('shell.settings_nav')}
      hideLabel
      {options}
      value={active}
      onchange={(id) => {
        const hit = entries.find((x) => x.e.id === id);
        if (hit) open(hit.e);
      }}
    />
  </div>
  <nav class="subnav" aria-label={t('shell.settings_nav')}>
    {#each groups as g (g.id)}
      <div class="group">
        <h2 class="group-title">{t(g.label_key)}</h2>
        <ul>
          {#each g.entries as e (e.id)}
            <li>
              <button type="button" data-testid="subnav-item" class:active={e.id === active} aria-current={e.id === active ? 'page' : undefined} onclick={() => open(e)}>
                {td(e.label_key)}
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  </nav>
  <div class="content">
    <!-- The hosted v1.0 screens bring their own headings; screen lanes move them to PageHeader. -->
    {@render children()}
  </div>
</div>

<style>
  .settings {
    display: grid;
    grid-template-columns: 204px minmax(0, 1fr);
    gap: var(--space-8);
    align-items: start;
  }
  .subnav {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-height: calc(100vh - 44px - var(--space-12));
    overflow-y: auto;
    margin-inline-start: -8px;
    padding-inline-end: var(--space-2);
  }
  .group-title {
    margin: 0 0 4px;
    padding: 0 8px;
    font-size: var(--text-2xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.02em;
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
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
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
    min-width: 0;
  }
  .picker {
    display: none;
  }
  /* Narrow window: the grouped list becomes one Select on top (no second column). */
  @media (max-width: 1023.98px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-4);
    }
    .subnav {
      display: none;
    }
    .picker {
      display: block;
      max-width: 360px;
    }
  }
</style>
