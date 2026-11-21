<!-- One toast (drawn by ToastHost; use `toast.show()` rather than this directly). -->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import Icon from './Icon.svelte';
  import IconButton from './IconButton.svelte';
  import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from './icons';
  import type { ToastItem } from './toast-store.svelte';

  let { item, ondismiss }: { item: ToastItem; ondismiss: () => void } = $props();

  const ICONS = { neutral: Info, success: CircleCheck, warning: TriangleAlert, danger: CircleAlert } as const;
</script>

<div class="fc-toast {item.tone}" role={item.tone === 'danger' ? 'alert' : 'status'}>
  <span class="icon"><Icon icon={ICONS[item.tone]} size={16} /></span>
  <span class="msg">{item.message}</span>
  {#if item.action}
    <button
      type="button"
      class="action"
      onclick={() => {
        item.action?.run();
        ondismiss();
      }}>{item.action.label}</button
    >
  {/if}
  <IconButton icon={X} label={t('ui.dismiss')} size="sm" onclick={ondismiss} />
</div>

<style>
  .fc-toast {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 260px;
    max-width: 420px;
    min-height: 40px;
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-2);
    color: var(--text-1);
    box-shadow: var(--shadow-float);
    font-size: var(--text-sm);
    animation: fc-toast-in var(--motion) var(--ease);
  }
  .icon {
    display: inline-flex;
    color: var(--text-2);
  }
  .success .icon {
    color: var(--success);
  }
  .warning .icon {
    color: var(--warning);
  }
  .danger .icon {
    color: var(--danger);
  }
  .msg {
    flex: 1;
  }
  .action {
    border: none;
    background: none;
    padding: var(--space-1) var(--space-2);
    border-radius: 4px;
    color: var(--accent-text);
    font: inherit;
    font-weight: var(--weight-semibold);
    cursor: pointer;
  }
  .action:hover {
    background: var(--accent-soft);
  }
  .action:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  @keyframes fc-toast-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fc-toast {
      animation: none;
    }
  }
</style>
