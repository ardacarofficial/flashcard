<!--
  Flashcard: a large card that flips between `front` and `back` on click, Space or Enter.
  3D flip of `--motion-slow`; with `prefers-reduced-motion` (or `instant`) the faces swap
  without animation. Text is large (36 px) and shrinks to fit long content (down to 16 px).
  `bind:flipped`; `onflip(flipped)` after each user flip. Answer buttons live outside.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';

  interface Props {
    front: Snippet;
    back: Snippet;
    flipped?: boolean;
    /** The user cannot flip (e.g. while loading). */
    disabled?: boolean;
    /** Swap faces without animation. */
    instant?: boolean;
    /** Shrink text to fit (default true). */
    fit?: boolean;
    size?: 'md' | 'lg';
    onflip?: (flipped: boolean) => void;
    class?: string;
  }

  const uid = $props.id();
  let {
    front,
    back,
    flipped = $bindable(false),
    disabled = false,
    instant = false,
    fit = true,
    size = 'lg',
    onflip,
    class: klass = '',
  }: Props = $props();

  let frontEl = $state<HTMLDivElement | null>(null);
  let backEl = $state<HTMLDivElement | null>(null);
  const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const frontId = `fc-card-${uid}-front`;
  const backId = `fc-card-${uid}-back`;

  function flip() {
    if (disabled) return;
    flipped = !flipped;
    onflip?.(flipped);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault();
      flip();
    }
  }

  /** Steps the face's text size down until its content fits (no layout in tests: no-op). */
  function shrink(el: HTMLElement | null) {
    if (!el || !fit) return;
    const inner = el.firstElementChild as HTMLElement | null;
    if (!inner) return;
    let px = size === 'lg' ? 36 : 24;
    inner.style.fontSize = `${px}px`;
    while (px > 16 && inner.scrollHeight > el.clientHeight && el.clientHeight > 0) {
      px -= 2;
      inner.style.fontSize = `${px}px`;
    }
  }

  $effect(() => {
    void flipped;
    shrink(frontEl);
    shrink(backEl);
  });
</script>

<div
  class="fc-flashcard {size} {klass}"
  class:flipped
  class:instant={instant || reduced}
  class:disabled
  role="button"
  tabindex={disabled ? -1 : 0}
  aria-label={t('ui.flashcard.flip')}
  aria-describedby="{flipped ? backId : frontId} {uid}-hint"
  aria-disabled={disabled || undefined}
  onclick={flip}
  {onkeydown}
>
  <div class="inner">
    <div class="face front" bind:this={frontEl} aria-hidden={flipped ? 'true' : undefined}>
      <div id={frontId} class="content">{@render front()}</div>
    </div>
    <div class="face back" bind:this={backEl} aria-hidden={flipped ? undefined : 'true'}>
      <div id={backId} class="content">{@render back()}</div>
    </div>
  </div>
  <span id="{uid}-hint" class="sr-only" aria-live="polite">
    {flipped ? t('ui.flashcard.showing_back') : t('ui.flashcard.showing_front')}
  </span>
</div>

<style>
  .fc-flashcard {
    position: relative;
    width: 100%;
    max-width: 720px;
    aspect-ratio: 720 / 440;
    max-height: 440px;
    margin-inline: auto;
    perspective: 1600px;
    cursor: pointer;
    border-radius: var(--radius-flashcard);
  }
  .md {
    max-width: 560px;
    max-height: 340px;
  }
  .fc-flashcard:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .disabled {
    cursor: default;
  }
  .inner {
    position: absolute;
    inset: 0;
    transform-style: preserve-3d;
    transition: transform var(--motion-slow) var(--ease);
  }
  .flipped .inner {
    transform: rotateY(180deg);
  }
  .face {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-8);
    overflow: hidden;
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-flashcard);
    backface-visibility: hidden;
    color: var(--text-1);
  }
  .back {
    transform: rotateY(180deg);
  }
  .content {
    max-width: 100%;
    max-height: 100%;
    font-size: 36px;
    line-height: var(--leading-tight);
    text-align: center;
    overflow-wrap: anywhere;
  }
  .md .content {
    font-size: 24px;
  }
  /* Reduced motion: no 3D, the hidden face is simply not shown. */
  .instant .inner {
    transition: none;
    transform: none;
  }
  .instant .face {
    backface-visibility: visible;
    transform: none;
  }
  .instant .back,
  .instant.flipped .front {
    visibility: hidden;
  }
  .instant.flipped .back {
    visibility: visible;
  }
  @media (prefers-reduced-motion: reduce) {
    .inner {
      transition: none;
    }
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
