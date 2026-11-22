<!--
  "Advanced" disclosure of a settings page (v1.3): rarely used rows behind one button, closed
  by default; its open/closed state is remembered per `id` (this device only). It opens by
  itself while the settings search is active, so a matching row is never hidden. Put it after
  the page's everyday sections and before a `tone="danger"` section.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { ChevronRight, Icon } from '$lib/ui';
  import { loadOpen, saveOpen } from './disclosure';
  import { settingsSearch } from './search.svelte';

  interface Props {
    /** Stable id: the remembered state's key (e.g. `mcp`, `notetypes.code`). */
    id: string;
    /** Button text (default "Advanced settings"). */
    title?: string;
    /** One line saying what is inside and who needs it. */
    summary?: string;
    children: Snippet;
  }

  let { id, title, summary, children }: Props = $props();
  const uid = $props.id();
  let open = $state(false);
  $effect.pre(() => {
    open = loadOpen(id);
  });
  const shown = $derived(open || settingsSearch.active);

  function toggle() {
    open = !shown;
    if (settingsSearch.active && !open) settingsSearch.query = '';
    saveOpen(id, open);
  }
</script>

<section class="fc-settings-advanced" class:open={shown} data-testid="settings-advanced">
  <button type="button" class="toggle" aria-expanded={shown} aria-controls="fc-adv-{uid}" onclick={toggle}>
    <span class="chev"><Icon icon={ChevronRight} size={14} /></span>
    <span class="titles">
      <span class="title">{title ?? t('kit.advanced.title')}</span>
      {#if summary}<span class="summary">{summary}</span>{/if}
    </span>
  </button>
  {#if shown}
    <div class="body" id="fc-adv-{uid}">{@render children()}</div>
  {/if}
</section>

<style>
  .fc-settings-advanced {
    border-top: 1px solid var(--border);
    padding-top: var(--space-2);
  }
  .toggle {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-2);
    margin-inline-start: calc(-1 * var(--space-2));
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .toggle:hover {
    background: var(--surface-2);
  }
  .toggle:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 0;
  }
  .chev {
    display: inline-flex;
    margin-top: 2px;
    color: var(--text-3);
    transition: transform var(--duration-fast, 120ms) ease;
  }
  .open .chev {
    transform: rotate(90deg);
  }
  .titles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .title {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .summary {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    padding-top: var(--space-4);
  }
</style>
