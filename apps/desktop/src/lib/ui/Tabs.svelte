<!--
  Tabs (`role="tablist"`): ←/→ Home/End move and activate; one tab stop. `bind:value` holds
  the active tab id. Pass `children` to render the panel (`{#snippet children(id)}…`), or
  leave it out and render content yourself (e.g. section sub-navigation).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Badge from './Badge.svelte';
  import Icon from './Icon.svelte';
  import type { TabItem } from './types';

  interface Props {
    tabs: TabItem[];
    value?: string;
    /** Accessible name of the tab list. */
    label: string;
    variant?: 'line' | 'pill';
    onchange?: (id: string) => void;
    children?: Snippet<[string]>;
    class?: string;
  }

  const uid = $props.id();
  let { tabs, value = $bindable(tabs[0]?.id ?? ''), label, variant = 'line', onchange, children, class: klass = '' }: Props = $props();

  let refs = $state<(HTMLButtonElement | null)[]>([]);
  const tabId = (id: string) => `fc-tabs-${uid}-${id}`;
  const panelId = `fc-tabs-${uid}-panel`;
  const activeIndex = $derived(Math.max(0, tabs.findIndex((t) => t.id === value)));

  function pick(i: number, focus = false) {
    const tab = tabs[i];
    if (!tab || tab.disabled) return;
    if (focus) refs[i]?.focus();
    if (tab.id === value) return;
    value = tab.id;
    onchange?.(tab.id);
  }

  function move(from: number, dir: 1 | -1) {
    const n = tabs.length;
    for (let k = 1; k <= n; k++) {
      const i = (((from + dir * k) % n) + n) % n;
      if (!tabs[i]!.disabled) return pick(i, true);
    }
  }

  function onkeydown(e: KeyboardEvent, i: number) {
    const map: Record<string, () => void> = {
      ArrowRight: () => move(i, 1),
      ArrowLeft: () => move(i, -1),
      Home: () => move(-1, 1),
      End: () => move(tabs.length, -1),
    };
    const f = map[e.key];
    if (f) {
      e.preventDefault();
      f();
    }
  }
</script>

<div class="fc-tabs {variant} {klass}">
  <div class="list" role="tablist" aria-label={label}>
    {#each tabs as tab, i (tab.id)}
      <button
        bind:this={refs[i]}
        id={tabId(tab.id)}
        type="button"
        role="tab"
        aria-selected={i === activeIndex}
        aria-controls={children ? panelId : undefined}
        tabindex={i === activeIndex ? 0 : -1}
        disabled={tab.disabled}
        onclick={() => pick(i)}
        onkeydown={(e) => onkeydown(e, i)}
      >
        {#if tab.icon}<Icon icon={tab.icon} size={16} />{/if}
        <span>{tab.label}</span>
        <Badge count={tab.badge} tone="neutral" variant="soft" />
      </button>
    {/each}
  </div>
  {#if children && tabs[activeIndex]}
    <div id={panelId} role="tabpanel" aria-labelledby={tabId(tabs[activeIndex].id)} class="panel">
      {@render children(tabs[activeIndex].id)}
    </div>
  {/if}
</div>

<style>
  .list {
    display: flex;
    gap: var(--space-1);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .line .list {
    border-bottom: 1px solid var(--border);
    gap: var(--space-4);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: 40px;
    padding: 0 var(--space-1);
    border: none;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    cursor: pointer;
    transition: color var(--motion-fast) var(--ease);
  }
  .line button {
    margin-bottom: -1px;
    border-bottom: 2px solid transparent;
  }
  .line button[aria-selected='true'] {
    color: var(--text-1);
    border-bottom-color: var(--accent);
  }
  .pill button {
    height: 32px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-pill);
  }
  .pill button[aria-selected='true'] {
    background: var(--accent-soft);
    color: var(--text-1);
  }
  button:hover:not(:disabled) {
    color: var(--text-1);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
    border-radius: 6px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .panel {
    padding-top: var(--space-4);
  }
</style>
