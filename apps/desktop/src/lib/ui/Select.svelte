<!--
  Select: a custom single-choice list box (no native `<select>`). Keyboard: ↓/↑ or Alt+↓ open
  and move, Home/End, Enter/Space choose, Esc/Tab close, typing jumps to the first matching
  option. Focus stays on the trigger (`aria-activedescendant`). `bind:value`.
-->
<script lang="ts" generics="T">
  import { t } from '$lib/i18n/index.svelte';
  import Field from './Field.svelte';
  import Icon from './Icon.svelte';
  import { Check, ChevronDown } from './icons';
  import type { SelectOption } from './types';

  interface Props {
    value?: T | null;
    options: SelectOption<T>[];
    label?: string;
    hint?: string;
    error?: string | null;
    hideLabel?: boolean;
    /** Shown when no option matches `value` (default: "Choose…"). */
    placeholder?: string;
    disabled?: boolean;
    required?: boolean;
    size?: 'sm' | 'md' | 'lg';
    id?: string;
    /** Called after the user picks an option. */
    onchange?: (value: T) => void;
    class?: string;
  }

  let {
    value = $bindable(null),
    options,
    label,
    hint,
    error,
    hideLabel = false,
    placeholder,
    disabled = false,
    required = false,
    size = 'md',
    id,
    onchange,
    class: klass = '',
  }: Props = $props();

  const uid = $props.id();
  let open = $state(false);
  let active = $state(-1);
  let root = $state<HTMLDivElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let typed = '';
  let typedAt = 0;

  const listId = `fc-select-${uid}-list`;
  const optionId = (i: number) => `fc-select-${uid}-opt-${i}`;
  const selectedIndex = $derived(options.findIndex((o) => Object.is(o.value, value)));
  const current = $derived(selectedIndex >= 0 ? options[selectedIndex] : undefined);
  const enabled = (i: number) => !!options[i] && !options[i].disabled;

  function step(from: number, dir: 1 | -1): number {
    for (let i = from + dir; i >= 0 && i < options.length; i += dir) if (enabled(i)) return i;
    return from;
  }
  const first = () => step(-1, 1);
  const last = () => step(options.length, -1);

  function show(at?: number) {
    if (disabled || open) return;
    open = true;
    active = at ?? (selectedIndex >= 0 ? selectedIndex : first());
  }
  function hide(refocus = true) {
    open = false;
    if (refocus) trigger?.focus();
  }
  function choose(i: number) {
    if (!enabled(i)) return;
    const o = options[i]!;
    value = o.value;
    hide();
    onchange?.(o.value);
  }

  function typeahead(ch: string) {
    const now = Date.now();
    typed = now - typedAt > 700 ? ch : typed + ch;
    typedAt = now;
    const q = typed.toLocaleLowerCase();
    const start = open ? active : selectedIndex;
    const order = [...options.keys()].map((k) => (k + start + 1) % options.length);
    const hit = order.find((i) => enabled(i) && options[i]!.label.toLocaleLowerCase().startsWith(q));
    if (hit === undefined) return;
    if (open) active = hit;
    else choose(hit);
  }

  function onkeydown(e: KeyboardEvent) {
    if (disabled) return;
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        if (!open) show();
        else active = step(active, 1);
        return;
      case 'ArrowUp':
        e.preventDefault();
        if (!open) show();
        else active = step(active, -1);
        return;
      case 'Home':
        if (open) {
          e.preventDefault();
          active = first();
        }
        return;
      case 'End':
        if (open) {
          e.preventDefault();
          active = last();
        }
        return;
      case 'Enter':
      case ' ':
        e.preventDefault();
        if (open) choose(active);
        else show();
        return;
      case 'Escape':
        if (open) {
          e.preventDefault();
          e.stopPropagation();
          hide();
        }
        return;
      case 'Tab':
        if (open) hide(false);
        return;
      default:
        // A letter on a focused select is type-ahead: it belongs to the select, never to a
        // page/global shortcut (v1.3, journey S10: "n" opened Quick Add mid type-ahead).
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          e.preventDefault();
          typeahead(e.key);
        }
    }
  }

  // Outside clicks close the list.
  $effect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (root && !root.contains(e.target as Node)) hide(false);
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  });

  // Keep the active option in view.
  $effect(() => {
    if (!open || active < 0) return;
    document.getElementById(optionId(active))?.scrollIntoView?.({ block: 'nearest' });
  });
</script>

<Field {label} {hint} {error} {hideLabel} {required} {id} class={klass}>
  {#snippet control({ id: cid, describedBy, invalid })}
    <div class="fc-select {size}" class:invalid bind:this={root}>
      <button
        bind:this={trigger}
        id={cid}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={open && active >= 0 ? optionId(active) : undefined}
        aria-describedby={describedBy}
        aria-invalid={invalid || undefined}
        aria-required={required || undefined}
        {disabled}
        class="trigger"
        onclick={() => (open ? hide() : show())}
        {onkeydown}
      >
        <span class="value" class:placeholder={!current}>{current?.label ?? placeholder ?? t('ui.select.placeholder')}</span>
        <span class="chev"><Icon icon={ChevronDown} size={14} /></span>
      </button>
      {#if open}
        <ul id={listId} role="listbox" class="list" tabindex="-1" aria-labelledby={cid}>
          {#each options as o, i (i)}
            <!-- Keyboard handling lives on the combobox (aria-activedescendant). -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <li
              id={optionId(i)}
              role="option"
              aria-selected={i === selectedIndex}
              aria-disabled={o.disabled || undefined}
              class:active={i === active}
              onpointerenter={() => o.disabled || (active = i)}
              onpointerdown={(e) => e.preventDefault()}
              onclick={() => choose(i)}
            >
              <span class="opt">
                <span>{o.label}</span>
                {#if o.description}<span class="desc">{o.description}</span>{/if}
              </span>
              {#if i === selectedIndex}<span class="tick"><Icon icon={Check} size={14} /></span>{/if}
            </li>
          {:else}
            <li class="empty" role="presentation">{t('ui.select.empty')}</li>
          {/each}
        </ul>
      {/if}
    </div>
  {/snippet}
</Field>

<style>
  .fc-select {
    position: relative;
  }
  .trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    height: var(--control-h-md);
    padding: 0 var(--space-2) 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease);
  }
  .trigger:hover:not(:disabled),
  .trigger[aria-expanded='true'] {
    border-color: var(--border-strong);
  }
  .trigger:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .trigger:disabled {
    background: var(--disabled-bg);
    color: var(--disabled-fg);
    cursor: not-allowed;
  }
  .chev {
    display: inline-flex;
    color: var(--text-3);
  }
  .sm .trigger {
    height: var(--control-h-sm);
    font-size: var(--text-xs);
    padding-inline-start: var(--space-2);
  }
  .lg .trigger {
    height: var(--control-h-lg);
    font-size: var(--text-md);
  }
  .invalid .trigger {
    border-color: var(--danger);
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .placeholder {
    color: var(--text-2);
  }
  .list {
    position: absolute;
    z-index: 40;
    inset-inline: 0;
    top: calc(100% + var(--space-1));
    min-width: 180px;
    max-height: 296px;
    overflow: auto;
    margin: 0;
    padding: var(--space-1);
    list-style: none;
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-float);
    animation: fc-select-in var(--motion-fast) var(--ease);
  }
  li[role='option'] {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: 28px;
    padding: 4px var(--space-2);
    border-radius: 4px;
    cursor: pointer;
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  li.active {
    background: var(--surface-3);
  }
  li[aria-selected='true'] {
    font-weight: var(--weight-medium);
  }
  .tick {
    display: inline-flex;
    flex: none;
    color: var(--accent-text);
  }
  li[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }
  .opt {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-2);
    font-weight: var(--weight-regular);
  }
  .empty {
    padding: var(--space-2);
    color: var(--text-2);
  }
  @keyframes fc-select-in {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
  }
</style>
