<!--
  Segmented control: 2–5 mutually exclusive choices shown side by side (`role="radiogroup"`).
  ←/→ (and ↑/↓) move and select, Home/End jump; one tab stop. `bind:value`.
-->
<script lang="ts" generics="T">
  import Icon from './Icon.svelte';
  import type { SelectOption } from './types';

  interface Props {
    value?: T;
    options: SelectOption<T>[];
    /** Accessible name of the group. */
    label: string;
    size?: 'sm' | 'md';
    fullWidth?: boolean;
    onchange?: (value: T) => void;
    class?: string;
  }

  let { value = $bindable(), options, label, size = 'md', fullWidth = false, onchange, class: klass = '' }: Props = $props();

  let buttons = $state<HTMLButtonElement[]>([]);
  const selected = $derived(options.findIndex((o) => Object.is(o.value, value)));
  const focusable = $derived(selected >= 0 ? selected : options.findIndex((o) => !o.disabled));

  function pick(i: number, focus = false) {
    const o = options[i];
    if (!o || o.disabled) return;
    if (focus) buttons[i]?.focus();
    if (Object.is(o.value, value)) return;
    value = o.value;
    onchange?.(o.value);
  }

  function move(from: number, dir: 1 | -1) {
    const n = options.length;
    for (let k = 1; k <= n; k++) {
      const i = (((from + dir * k) % n) + n) % n;
      if (!options[i]!.disabled) return pick(i, true);
    }
  }

  function onkeydown(e: KeyboardEvent, i: number) {
    const keys: Record<string, () => void> = {
      ArrowRight: () => move(i, 1),
      ArrowDown: () => move(i, 1),
      ArrowLeft: () => move(i, -1),
      ArrowUp: () => move(i, -1),
      Home: () => move(-1, 1),
      End: () => move(options.length, -1),
    };
    const f = keys[e.key];
    if (f) {
      e.preventDefault();
      f();
    }
  }
</script>

<div class="fc-seg {size} {klass}" class:full={fullWidth} role="radiogroup" aria-label={label}>
  {#each options as o, i (i)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      aria-checked={i === selected}
      tabindex={i === focusable ? 0 : -1}
      disabled={o.disabled}
      onclick={() => pick(i)}
      onkeydown={(e) => onkeydown(e, i)}
    >
      {#if o.icon}<Icon icon={o.icon} size={16} />{/if}
      <span>{o.label}</span>
    </button>
  {/each}
</div>

<style>
  .fc-seg {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .full {
    display: flex;
  }
  .full button {
    flex: 1;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    height: calc(var(--control-h-md) - 6px);
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition: background-color var(--motion-fast) var(--ease), color var(--motion-fast) var(--ease);
  }
  .sm button {
    height: calc(var(--control-h-sm) - 6px);
    font-size: var(--text-xs);
    padding: 0 var(--space-2);
  }
  button:hover:not(:disabled) {
    color: var(--text-1);
  }
  button[aria-checked='true'] {
    background: var(--surface-3);
    border-color: var(--border-strong);
    color: var(--text-1);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
