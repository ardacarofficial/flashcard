<!--
  Modal dialog, or a side sheet (`variant="sheet"`). Traps Tab focus, closes on Esc and on a
  backdrop click (unless `dismissible={false}`), returns focus to where it was on close.
  `bind:open`; actions go in the `footer` snippet.

  Focus on open: `initialFocus` (a selector inside the dialog), else the first focusable
  control that is not the close button, else the dialog itself.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import IconButton from './IconButton.svelte';
  import { X } from './icons';
  import { focusables, portal, trapTab } from './focus';

  interface Props {
    open?: boolean;
    title: string;
    description?: string;
    variant?: 'dialog' | 'sheet';
    size?: 'sm' | 'md' | 'lg';
    /** Esc, backdrop click and the × button close it (default true). */
    dismissible?: boolean;
    /** `alertdialog` for confirmations that interrupt. */
    role?: 'dialog' | 'alertdialog';
    initialFocus?: string;
    onclose?: () => void;
    children?: Snippet;
    footer?: Snippet;
  }

  const uid = $props.id();
  let {
    open = $bindable(false),
    title,
    description,
    variant = 'dialog',
    size = 'md',
    dismissible = true,
    role = 'dialog',
    initialFocus,
    onclose,
    children,
    footer,
  }: Props = $props();

  let panel = $state<HTMLDivElement | null>(null);
  const titleId = `fc-dialog-${uid}-title`;
  const descId = `fc-dialog-${uid}-desc`;

  function close() {
    if (!open) return;
    open = false;
    onclose?.();
  }

  $effect(() => {
    if (!open || !panel) return;
    const before = document.activeElement as HTMLElement | null;
    const el = panel;
    const target =
      (initialFocus ? el.querySelector<HTMLElement>(initialFocus) : null) ??
      focusables(el).find((f) => !f.dataset.dialogClose) ??
      el;
    target.focus();
    return () => {
      if (before && document.contains(before)) before.focus();
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && dismissible) {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (panel) {
      trapTab(panel, e);
    }
  }
</script>

{#if open}
  <div class="fc-overlay {variant}" use:portal>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => dismissible && close()}></div>
    <div
      bind:this={panel}
      class="panel {variant} {size}"
      {role}
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={description ? descId : undefined}
      tabindex="-1"
      {onkeydown}
    >
      <header>
        <div class="heading">
          <h2 id={titleId}>{title}</h2>
          {#if description}<p id={descId} class="desc">{description}</p>{/if}
        </div>
        {#if dismissible}
          <IconButton icon={X} label={t('ui.close')} size="sm" data-dialog-close="true" onclick={close} />
        {/if}
      </header>
      {#if children}<div class="body">{@render children()}</div>{/if}
      {#if footer}<footer>{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .fc-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
  }
  .fc-overlay.sheet {
    justify-content: flex-end;
    align-items: stretch;
    padding: 0;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgb(0 0 0 / 0.4);
    animation: fc-fade var(--motion) var(--ease);
  }
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
    max-height: 100%;
    background: var(--surface-1);
    color: var(--text-1);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    outline: none;
  }
  .panel.dialog {
    border-radius: var(--radius-lg);
    animation: fc-pop var(--motion) var(--ease);
  }
  .panel.sheet {
    border-radius: var(--radius-lg) 0 0 var(--radius-lg);
    animation: fc-slide var(--motion-slow) var(--ease);
  }
  .dialog.sm {
    max-width: 400px;
  }
  .dialog.md {
    max-width: 560px;
  }
  .dialog.lg {
    max-width: 800px;
  }
  .sheet.sm {
    max-width: 360px;
  }
  .sheet.md {
    max-width: 480px;
  }
  .sheet.lg {
    max-width: 720px;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-6) var(--space-6) var(--space-4);
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }
  .desc {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .body {
    padding: 0 var(--space-6) var(--space-6);
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-6);
    border-top: 1px solid var(--border);
  }
  .sheet .body {
    flex: 1;
  }
  @keyframes fc-fade {
    from {
      opacity: 0;
    }
  }
  @keyframes fc-pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  @keyframes fc-slide {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .backdrop,
    .panel {
      animation: none !important;
    }
  }
</style>
