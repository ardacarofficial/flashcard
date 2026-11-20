<!--
  Chip: a small pill for classifications, filters and tags. With `onclick` it is a toggle
  button (`selected` → `aria-pressed`); with `onremove` it gets a remove (×) button.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import { t } from '$lib/i18n/index.svelte';
  import Icon from './Icon.svelte';
  import { X } from './icons';
  import type { Tone } from './types';

  interface Props {
    label?: string;
    tone?: Tone;
    icon?: IconNode;
    size?: 'sm' | 'md';
    selected?: boolean;
    onclick?: () => void;
    onremove?: () => void;
    disabled?: boolean;
    children?: Snippet;
    class?: string;
  }

  let { label, tone = 'neutral', icon, size = 'md', selected, onclick, onremove, disabled = false, children, class: klass = '' }: Props = $props();
</script>

{#snippet content()}
  {#if icon}<Icon {icon} size={14} />{/if}
  {#if children}{@render children()}{:else}{label}{/if}
{/snippet}

<span class="fc-chip {tone} {size} {klass}" class:selected class:interactive={!!onclick}>
  {#if onclick}
    <button type="button" class="main" aria-pressed={selected} {disabled} {onclick}>{@render content()}</button>
  {:else}
    <span class="main">{@render content()}</span>
  {/if}
  {#if onremove}
    <button type="button" class="remove" aria-label={t('ui.remove', { label: label ?? '' })} {disabled} onclick={onremove}>
      <Icon icon={X} size={12} strokeWidth={2.5} />
    </button>
  {/if}
</span>

<style>
  .fc-chip {
    display: inline-flex;
    align-items: center;
    height: 24px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: 1;
    white-space: nowrap;
    border: 1px solid transparent;
    max-width: 100%;
  }
  .md {
    height: 28px;
  }
  .main {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 100%;
    padding: 0 var(--space-3);
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    border-radius: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sm .main {
    padding: 0 var(--space-2);
  }
  button.main {
    cursor: pointer;
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    margin-inline: -6px 4px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .remove:hover {
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .neutral {
    background: var(--surface-2);
    color: var(--text-1);
    border-color: var(--border);
  }
  .accent {
    background: var(--accent-soft);
    color: var(--text-1);
  }
  .success {
    background: var(--success-soft);
    color: var(--success);
  }
  .warning {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .interactive:hover {
    border-color: var(--border-strong);
  }
  .selected {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
</style>
