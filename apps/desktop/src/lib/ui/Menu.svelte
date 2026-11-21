<!--
  Menu: a "⋯" (or custom) trigger that opens a list of actions (`role="menu"`). Opening moves
  focus to the first item; ↑/↓ Home/End move, Enter/Space run, Esc closes and returns focus,
  Tab closes, typing jumps to a matching item. Items: see `MenuItem` in types.ts.
-->
<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import { t } from '$lib/i18n/index.svelte';
  import Icon from './Icon.svelte';
  import Kbd from './Kbd.svelte';
  import { Ellipsis } from './icons';
  import type { MenuItem } from './types';

  interface Props {
    items: MenuItem[];
    /** Accessible name of the trigger (default: "More actions"). */
    label?: string;
    icon?: IconNode;
    /** Custom trigger content (e.g. an Avatar); `label` stays its accessible name. */
    trigger?: Snippet;
    align?: 'start' | 'end';
    /** Open upwards (menus at the bottom of the window). */
    placement?: 'bottom' | 'top';
    triggerClass?: string;
    class?: string;
  }

  const uid = $props.id();
  let { items, label, icon = Ellipsis, trigger, align = 'end', placement = 'bottom', triggerClass = '', class: klass = '' }: Props = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  let button = $state<HTMLButtonElement | null>(null);
  let refs = $state<(HTMLButtonElement | null)[]>([]);
  const menuId = `fc-menu-${uid}`;

  type Action = Extract<MenuItem, { onselect: () => void }>;
  const isAction = (i: MenuItem): i is Action => !('separator' in i);
  const actionable = $derived(items.map((it, i) => (isAction(it) && !it.disabled ? i : -1)).filter((i) => i >= 0));

  function focusAt(i: number | undefined) {
    if (i !== undefined) refs[i]?.focus();
  }
  function current(): number {
    return refs.findIndex((r) => r === document.activeElement);
  }

  async function show(at: 'first' | 'last' = 'first') {
    open = true;
    await tick();
    focusAt(at === 'first' ? actionable[0] : actionable[actionable.length - 1]);
  }
  function hide(refocus = true) {
    open = false;
    if (refocus) button?.focus();
  }
  function run(item: Action) {
    if (item.disabled) return;
    hide();
    item.onselect();
  }

  function move(dir: 1 | -1) {
    const pos = actionable.indexOf(current());
    const n = actionable.length;
    if (n === 0) return;
    focusAt(actionable[pos < 0 ? (dir === 1 ? 0 : n - 1) : (pos + dir + n) % n]);
  }

  function onmenukey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        return move(1);
      case 'ArrowUp':
        e.preventDefault();
        return move(-1);
      case 'Home':
        e.preventDefault();
        return focusAt(actionable[0]);
      case 'End':
        e.preventDefault();
        return focusAt(actionable[actionable.length - 1]);
      case 'Escape':
        e.preventDefault();
        e.stopPropagation();
        return hide();
      case 'Tab':
        return hide(false);
      default:
        if (e.key.length === 1 && /\S/.test(e.key)) {
          const q = e.key.toLocaleLowerCase();
          const from = actionable.indexOf(current());
          const order = actionable.map((_, k) => actionable[(from + 1 + k) % actionable.length]!);
          const hit = order.find((i) => (items[i] as Action).label.toLocaleLowerCase().startsWith(q));
          focusAt(hit);
        }
    }
  }

  function ontriggerkey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      void show('first');
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      void show('last');
    }
  }

  $effect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (root && !root.contains(e.target as Node)) hide(false);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  });
</script>

<div class="fc-menu {klass}" bind:this={root}>
  <button
    bind:this={button}
    type="button"
    class="trigger {triggerClass}"
    class:custom={!!trigger}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={open ? menuId : undefined}
    aria-label={label ?? t('ui.more')}
    title={trigger ? undefined : (label ?? t('ui.more'))}
    onclick={() => (open ? hide() : void show())}
    onkeydown={ontriggerkey}
  >
    {#if trigger}{@render trigger()}{:else}<Icon {icon} size={16} />{/if}
  </button>
  {#if open}
    <div id={menuId} role="menu" tabindex="-1" class="list {align} {placement}" aria-label={label ?? t('ui.more')} onkeydown={onmenukey}>
      {#each items as item, i (item.id)}
        {#if isAction(item)}
          <button
            bind:this={refs[i]}
            type="button"
            role="menuitem"
            tabindex="-1"
            class="item"
            class:danger={item.danger}
            aria-disabled={item.disabled || undefined}
            onclick={() => run(item)}
          >
            {#if item.icon}<span class="ico"><Icon icon={item.icon} size={14} /></span>{/if}
            <span class="label">{item.label}</span>
            {#if item.keys}<Kbd keys={item.keys} size="sm" />{/if}
          </button>
        {:else}
          <div role="separator" class="sep"></div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .fc-menu {
    position: relative;
    display: inline-flex;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-h-md);
    height: var(--control-h-md);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .trigger.custom {
    width: auto;
    height: auto;
    padding: 2px;
    border-radius: var(--radius-pill);
  }
  .trigger:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .trigger:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .list {
    position: absolute;
    z-index: 50;
    min-width: 200px;
    padding: var(--space-1);
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-float);
    animation: fc-menu-in var(--motion-fast) var(--ease);
  }
  @keyframes fc-menu-in {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
  }
  .bottom {
    top: calc(100% + var(--space-1));
  }
  .top {
    bottom: calc(100% + var(--space-1));
  }
  .end {
    inset-inline-end: 0;
  }
  .start {
    inset-inline-start: 0;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 28px;
    padding: 4px var(--space-2);
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    white-space: nowrap;
    cursor: pointer;
  }
  .item:hover,
  .item:focus {
    background: var(--surface-3);
    outline: none;
  }
  .ico {
    display: inline-flex;
    color: var(--text-2);
  }
  .danger .ico {
    color: inherit;
  }
  .item:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .item[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }
  .danger {
    color: var(--danger);
  }
  .label {
    flex: 1;
  }
  .sep {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--border);
  }
</style>
