<!--
  Tooltip: a short hint shown on hover and keyboard focus of its child, after `delay` ms;
  Esc hides it. The child's first focusable element gets `aria-describedby`. Hints only —
  never put essential information or controls in a tooltip.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    text: string;
    placement?: 'top' | 'bottom' | 'start' | 'end';
    delay?: number;
    children: Snippet;
    class?: string;
  }

  const uid = $props.id();
  let { text, placement = 'top', delay = 400, children, class: klass = '' }: Props = $props();

  let shown = $state(false);
  let anchor = $state<HTMLSpanElement | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const tipId = `fc-tip-${uid}`;

  function enter() {
    clearTimeout(timer);
    timer = setTimeout(() => (shown = true), delay);
  }
  function leave() {
    clearTimeout(timer);
    shown = false;
  }

  $effect(() => {
    const target = anchor?.querySelector<HTMLElement>('a, button, input, select, textarea, [tabindex]');
    if (!target) return;
    const prev = target.getAttribute('aria-describedby');
    target.setAttribute('aria-describedby', [prev, tipId].filter(Boolean).join(' '));
    return () => {
      if (prev) target.setAttribute('aria-describedby', prev);
      else target.removeAttribute('aria-describedby');
    };
  });

  $effect(() => () => clearTimeout(timer));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class="fc-tooltip {klass}"
  bind:this={anchor}
  onpointerenter={enter}
  onpointerleave={leave}
  onfocusin={enter}
  onfocusout={leave}
  onkeydown={(e) => e.key === 'Escape' && leave()}
>
  {@render children()}
  <span id={tipId} role="tooltip" class="tip {placement}" class:shown>{text}</span>
</span>

<style>
  .fc-tooltip {
    position: relative;
    display: inline-flex;
  }
  .tip {
    position: absolute;
    z-index: 60;
    padding: var(--space-1) var(--space-2);
    border-radius: 6px;
    background: var(--text-1);
    color: var(--surface-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition: opacity var(--motion-fast) var(--ease);
  }
  .tip.shown {
    opacity: 1;
    visibility: visible;
  }
  .top {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .bottom {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .end {
    left: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
  }
  .start {
    right: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
  }
</style>
