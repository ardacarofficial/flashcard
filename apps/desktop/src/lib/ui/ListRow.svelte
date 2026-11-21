<!--
  ListRow: one dense 34 px row of a list or tree-table (Library decks, Home's due decks, term
  lists). Put rows in a `<ul class="…">` (or `role="list"`); the row is an `<li>`.

  Layout: [indent + tree chevron] [leading] [label / sub] [cells] [actions].
  - `onclick` makes the label area a button (Enter/Space; the row's main action).
  - `cells` are the right-hand columns (counts, bars); `cellColumns` is their CSS grid track
    list so every row of a list lines up (`'56px 56px 120px'`). Numbers inside: `.num`.
  - `actions` show on hover and keyboard focus (row actions: start, menu).
  - `depth` indents a tree level; `expanded` (true/false) draws a chevron toggled by `ontoggle`.
  - `header` draws the column header row instead (no hover, small secondary text).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import Icon from './Icon.svelte';
  import { ChevronDown, ChevronRight } from './icons';

  interface Props {
    label?: string;
    /** Secondary text after the label (path, note type). */
    sub?: string;
    onclick?: () => void;
    /** Accessible name of the row button (default: its text). */
    buttonLabel?: string;
    depth?: number;
    /** Tree rows: `true`/`false` draws the expand chevron; `undefined` = leaf. */
    expanded?: boolean;
    ontoggle?: () => void;
    active?: boolean;
    selected?: boolean;
    header?: boolean;
    cellColumns?: string;
    leading?: Snippet;
    /** Replaces `label`/`sub` (rich label content). */
    children?: Snippet;
    cells?: Snippet;
    actions?: Snippet;
    testid?: string;
    class?: string;
  }

  let {
    label,
    sub,
    onclick,
    buttonLabel,
    depth = 0,
    expanded,
    ontoggle,
    active = false,
    selected = false,
    header = false,
    cellColumns,
    leading,
    children,
    cells,
    actions,
    testid,
    class: klass = '',
  }: Props = $props();
</script>

{#snippet text()}
  {#if leading}<span class="leading">{@render leading()}</span>{/if}
  {#if children}{@render children()}{:else}
    <span class="label">{label}</span>
    {#if sub}<span class="sub">{sub}</span>{/if}
  {/if}
{/snippet}

<li
  class="fc-list-row {klass}"
  class:header
  class:active
  class:selected
  class:clickable={!!onclick}
  role={header ? 'presentation' : undefined}
  data-testid={testid}
  style:--depth={depth}
>
  <span class="indent" aria-hidden={expanded === undefined ? 'true' : undefined}>
    {#if expanded !== undefined}
      <button type="button" class="toggle" aria-expanded={expanded} aria-label={expanded ? t('ui.collapse') : t('ui.expand')} onclick={ontoggle}>
        <Icon icon={expanded ? ChevronDown : ChevronRight} size={14} />
      </button>
    {/if}
  </span>
  {#if onclick}
    <button type="button" class="main" aria-label={buttonLabel} aria-current={active ? 'true' : undefined} {onclick}>{@render text()}</button>
  {:else}
    <span class="main">{@render text()}</span>
  {/if}
  {#if cells}<span class="cells" style:grid-template-columns={cellColumns}>{@render cells()}</span>{/if}
  {#if actions}<span class="actions">{@render actions()}</span>{/if}
</li>

<style>
  .fc-list-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--row-h);
    padding: 0 var(--space-2) 0 calc(var(--space-1) + var(--depth, 0) * 16px);
    border-bottom: 1px solid var(--border);
    color: var(--text-1);
    font-size: var(--text-sm);
    list-style: none;
  }
  .fc-list-row:last-child {
    border-bottom: none;
  }
  .fc-list-row:not(.header):hover,
  .fc-list-row:focus-within {
    background: var(--surface-2);
  }
  .selected {
    background: var(--accent-soft);
  }
  .active {
    background: var(--surface-3);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .header {
    min-height: 30px;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .indent {
    display: inline-flex;
    flex: none;
    width: 20px;
    justify-content: center;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .toggle:hover {
    background: var(--surface-3);
    color: var(--text-1);
  }
  .main {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 1;
    min-width: 0;
    height: var(--row-h);
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
  }
  .header .main {
    height: 30px;
  }
  button.main {
    cursor: pointer;
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
    border-radius: 4px;
  }
  .leading {
    display: inline-flex;
    flex: none;
    color: var(--text-3);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header .label {
    font-weight: inherit;
  }
  .sub {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cells {
    display: grid;
    grid-auto-flow: column;
    align-items: center;
    gap: var(--space-3);
    flex: none;
    color: var(--text-2);
    font-size: var(--text-xs);
    text-align: end;
  }
  .actions {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: none;
    opacity: 0;
    transition: opacity var(--motion-fast) var(--ease);
  }
  .fc-list-row:hover .actions,
  .fc-list-row:focus-within .actions,
  .active .actions {
    opacity: 1;
  }
  @media (hover: none) {
    .actions {
      opacity: 1;
    }
  }
</style>
