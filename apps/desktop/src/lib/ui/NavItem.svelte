<!--
  One sidebar entry (an `<li>` with a button). `active` sets `aria-current="page"`; `accent`
  draws it as the prominent action (Create); `badge` shows a count when > 0.
  Below 1024 px (or `collapsed`) only the icon shows; the label stays the accessible name.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import Badge from './Badge.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    label: string;
    icon: IconNode;
    active?: boolean;
    accent?: boolean;
    badge?: number | null;
    /** Accessible text for the badge ("Inbox, 3 waiting"); default: label + count. */
    badgeLabel?: string;
    collapsed?: boolean;
    onclick: () => void;
    /** Replaces the icon (e.g. an Avatar). */
    leading?: Snippet;
  }

  let { label, icon, active = false, accent = false, badge = null, badgeLabel, collapsed = false, onclick, leading }: Props = $props();

  const hasBadge = $derived(badge !== null && badge > 0);
</script>

<li class="fc-nav-item" class:collapsed>
  <button
    type="button"
    class:active
    class:accent
    aria-current={active ? 'page' : undefined}
    aria-label={hasBadge ? (badgeLabel ?? `${label} (${badge})`) : undefined}
    title={label}
    {onclick}
  >
    <span class="icon">
      {#if leading}{@render leading()}{:else}<Icon {icon} size={20} />{/if}
      {#if hasBadge}<span class="dot" aria-hidden="true"></span>{/if}
    </span>
    <span class="label">{label}</span>
    {#if hasBadge}<span class="badge" aria-hidden="true"><Badge count={badge} /></span>{/if}
  </button>
</li>

<style>
  button {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 40px;
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    text-align: start;
    cursor: pointer;
    transition: background-color var(--motion-fast) var(--ease), color var(--motion-fast) var(--ease);
  }
  button:hover {
    background: color-mix(in srgb, var(--text-1) 6%, transparent);
    color: var(--text-1);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  button.active {
    background: var(--surface-1);
    color: var(--text-1);
    box-shadow: var(--shadow-1);
  }
  button.active .icon {
    color: var(--accent);
  }
  button.accent {
    margin: var(--space-1) 0;
    background: var(--accent);
    color: var(--accent-fg);
  }
  button.accent .icon {
    color: inherit;
  }
  button.accent:hover {
    background: var(--accent-hover);
    color: var(--accent-fg);
  }
  button.accent.active {
    box-shadow: 0 0 0 2px var(--surface-2), 0 0 0 4px var(--accent);
  }
  .icon {
    position: relative;
    display: inline-flex;
    flex: none;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    display: none;
    position: absolute;
    top: -2px;
    right: -3px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--surface-2);
  }
  .collapsed button {
    justify-content: center;
    width: 44px;
    padding: 0;
  }
  .collapsed .label,
  .collapsed .badge {
    display: none;
  }
  .collapsed .dot {
    display: block;
  }
  @media (max-width: 1023.98px) {
    button {
      justify-content: center;
      width: 44px;
      padding: 0;
    }
    .label,
    .badge {
      display: none;
    }
    .dot {
      display: block;
    }
  }
</style>
