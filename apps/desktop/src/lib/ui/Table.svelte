<!--
  Virtualised table (`role="grid"`): only the rows in view (+ overscan) are in the DOM, so
  10 000 rows scroll smoothly. Fixed row height. Focus the grid, then ↑/↓, PgUp/PgDn,
  Home/End move the active row; Enter (or click) calls `onrowclick`. Cells are plain text
  from `column.value`, or your `cell` snippet (`{#snippet cell(row, column)}`).
-->
<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { TableColumn } from './types';

  interface Props {
    rows: T[];
    columns: TableColumn<T>[];
    rowKey: (row: T) => string;
    /** Accessible name of the grid. */
    label: string;
    rowHeight?: number;
    /** CSS height of the scrolling area. */
    height?: string;
    overscan?: number;
    /** Key of the active (highlighted) row. */
    activeKey?: string | null;
    /** Keys of selected rows (`aria-selected`). */
    selectedKeys?: ReadonlySet<string>;
    onrowclick?: (row: T) => void;
    cell?: Snippet<[T, TableColumn<T>]>;
    empty?: Snippet;
    class?: string;
  }

  const uid = $props.id();
  let {
    rows,
    columns,
    rowKey,
    label,
    rowHeight = 34,
    height = '480px',
    overscan = 6,
    activeKey = $bindable(null),
    selectedKeys,
    onrowclick,
    cell,
    empty,
    class: klass = '',
  }: Props = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(0);

  const template = $derived(columns.map((c) => c.width ?? 'minmax(0, 1fr)').join(' '));
  /** jsdom (tests) has no layout: assume a 600 px viewport. */
  const visibleHeight = $derived(viewport || 600);
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + visibleHeight) / rowHeight) + overscan));
  const windowRows = $derived(rows.slice(start, end).map((row, k) => ({ row, index: start + k, key: rowKey(row) })));
  const activeIndex = $derived(activeKey === null ? -1 : rows.findIndex((r) => rowKey(r) === activeKey));
  const rowId = (i: number) => `fc-table-${uid}-row-${i}`;

  function setActive(i: number) {
    if (rows.length === 0) return;
    const k = Math.min(rows.length - 1, Math.max(0, i));
    activeKey = rowKey(rows[k]!);
    // Keep it in view.
    if (!scroller) return;
    const top = k * rowHeight;
    const header = rowHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + rowHeight > scroller.scrollTop + visibleHeight - header) scroller.scrollTop = top + rowHeight - visibleHeight + header;
    scrollTop = scroller.scrollTop;
  }

  function onkeydown(e: KeyboardEvent) {
    const page = Math.max(1, Math.floor(visibleHeight / rowHeight) - 1);
    const at = activeIndex;
    const moves: Record<string, () => number> = {
      ArrowDown: () => at + 1,
      ArrowUp: () => (at < 0 ? 0 : at - 1),
      PageDown: () => at + page,
      PageUp: () => at - page,
      Home: () => 0,
      End: () => rows.length - 1,
    };
    const f = moves[e.key];
    if (f) {
      e.preventDefault();
      setActive(f());
    } else if (e.key === 'Enter' && at >= 0) {
      e.preventDefault();
      onrowclick?.(rows[at]!);
    }
  }

  // Viewport height (ResizeObserver where available; jsdom has none).
  $effect(() => {
    const el = scroller;
    if (!el) return;
    viewport = el.clientHeight;
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => (viewport = el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  });

  const text = (row: T, c: TableColumn<T>) => {
    const v = c.value?.(row);
    return v === null || v === undefined ? '' : String(v);
  };
</script>

<div class="fc-table {klass}">
  <div
    bind:this={scroller}
    class="scroller"
    style:height
    role="grid"
    aria-label={label}
    aria-rowcount={rows.length + 1}
    aria-colcount={columns.length}
    aria-activedescendant={activeIndex >= 0 ? rowId(activeIndex) : undefined}
    tabindex="0"
    onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
    {onkeydown}
  >
    <div class="row head" role="row" aria-rowindex={1} style:grid-template-columns={template} style:height="{rowHeight}px">
      {#each columns as c (c.id)}
        <div class="cell {c.align ?? 'start'}" class:num={c.numeric} role="columnheader">{c.header}</div>
      {/each}
    </div>
    {#if rows.length === 0}
      <div class="empty" role="row">
        <div role="gridcell">{#if empty}{@render empty()}{:else}{t('ui.table.empty')}{/if}</div>
      </div>
    {:else}
      <div class="body" style:height="{rows.length * rowHeight}px">
        {#each windowRows as r (r.key)}
          <!-- Rows are reached with the grid's arrow keys (aria-activedescendant), not Tab. -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
          <div
            id={rowId(r.index)}
            class="row"
            class:active={r.index === activeIndex}
            class:clickable={!!onrowclick}
            role="row"
            aria-rowindex={r.index + 2}
            aria-selected={selectedKeys ? selectedKeys.has(r.key) : undefined}
            style:grid-template-columns={template}
            style:height="{rowHeight}px"
            style:transform="translateY({r.index * rowHeight}px)"
            onclick={() => {
              activeKey = r.key;
              onrowclick?.(r.row);
            }}
          >
            {#each columns as c (c.id)}
              <div class="cell {c.align ?? 'start'}" class:num={c.numeric} role="gridcell">
                {#if cell}{@render cell(r.row, c)}{:else}{text(r.row, c)}{/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .fc-table {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    overflow: hidden;
  }
  .scroller {
    position: relative;
    overflow: auto;
    outline: none;
  }
  .scroller:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .row {
    display: grid;
    align-items: center;
    column-gap: var(--space-3);
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface-1);
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .body {
    position: relative;
  }
  .body .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
  .clickable {
    cursor: pointer;
  }
  .body .row:hover {
    background: var(--surface-2);
  }
  .row[aria-selected='true'] {
    background: var(--accent-soft);
  }
  .row.active {
    box-shadow: inset 2px 0 0 var(--accent);
    background: var(--surface-3);
  }
  .num {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  .head .num {
    font-family: inherit;
  }
  .cell {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .end {
    text-align: end;
  }
  .center {
    text-align: center;
  }
  .empty {
    padding: var(--space-8) var(--space-4);
    text-align: center;
    color: var(--text-2);
  }
</style>
