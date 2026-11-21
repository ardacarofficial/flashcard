<!--
  One sidebar entry (an `<li>` with a compact 30 px button). `active` sets
  `aria-current="page"` (selected surface + accent icon); `accent` keeps the icon in the
  accent colour; `badge` shows a count (mono) when > 0.
  Below 1024 px (or `collapsed`) only the icon shows; the label stays the accessible name.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
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
      {#if leading}{@render leading()}{:else}<Icon {icon} size={16} />{/if}
      {#if hasBadge}<span class="dot" aria-hidden="true"></span>{/if}
    </span>
    <span class="label">{label}</span>
    {#if hasBadge}<span class="badge num" aria-hidden="true">{(badge ?? 0) > 99 ? '99+' : badge}</span>{/if}
  </button>
</li>

<style>
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: var(--nav-row-h);
    padding: 0 10px;
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
    background: var(--surface-2);
    color: var(--text-1);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .icon {
    position: relative;
    display: inline-flex;
    flex: none;
    color: var(--text-3);
    transition: color var(--motion-fast) var(--ease);
  }
  button:hover .icon {
    color: var(--text-2);
  }
  button.active {
    background: var(--surface-3);
    color: var(--text-1);
  }
  button.active .icon,
  button.accent .icon {
    color: var(--accent-text);
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    color: var(--text-2);
    font-size: var(--text-2xs);
  }
  .dot {
    display: none;
    position: absolute;
    top: -2px;
    right: -3px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--surface-1);
  }
  .collapsed button {
    justify-content: center;
    width: 36px;
    height: 32px;
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
      width: 36px;
      height: 32px;
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
