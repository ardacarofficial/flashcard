<!--
  OptionTile: a large answer option (Learn, Test, Cards' multiple choice). A marker on the left
  — a keyboard key (`kbd`) or a letter (`letter`) — the text, and after checking a result icon
  with text (colour is never the only cue). With `checked` set it is a radio (`role="radio"`)
  inside a `role="radiogroup"` the screen draws; otherwise a plain button.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import Kbd from './Kbd.svelte';
  import { CircleAlert, CircleCheck } from './icons';

  interface Props {
    /** Option text (or `children`). */
    label?: string;
    children?: Snippet;
    /** Shortcut key shown as a `Kbd` (e.g. "1"). */
    kbd?: string;
    /** Plain marker when there is no shortcut (e.g. "A"). */
    letter?: string;
    /** Radio semantics: the picked option of its group. */
    checked?: boolean;
    /** After checking: `right` = the correct option, `miss` = the wrong pick. */
    result?: 'right' | 'miss' | null;
    /** Accessible names of the result icons. */
    rightLabel?: string;
    missLabel?: string;
    disabled?: boolean;
    onclick?: () => void;
    class?: string;
    testid?: string;
  }

  let {
    label,
    children,
    kbd,
    letter,
    checked,
    result = null,
    rightLabel,
    missLabel,
    disabled = false,
    onclick,
    class: klass = '',
    testid,
  }: Props = $props();

  const radio = $derived(checked !== undefined);
</script>

<button
  type="button"
  class="fc-option {klass}"
  class:checked
  class:right={result === 'right'}
  class:miss={result === 'miss'}
  role={radio ? 'radio' : undefined}
  aria-checked={radio ? !!checked : undefined}
  {disabled}
  {onclick}
  data-fc-ui
  data-testid={testid}
>
  {#if kbd}<Kbd keys={[kbd]} />{:else if letter}<span class="letter" aria-hidden="true">{letter}</span>{/if}
  <span class="text">{#if children}{@render children()}{:else}{label}{/if}</span>
  {#if result === 'right'}<Icon icon={CircleCheck} size={20} label={rightLabel} />{/if}
  {#if result === 'miss'}<Icon icon={CircleAlert} size={20} label={missLabel} />{/if}
</button>

<style>
  .fc-option {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 64px;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    line-height: var(--leading);
    text-align: start;
    cursor: pointer;
    transition:
      border-color var(--motion-fast) var(--ease),
      background var(--motion-fast) var(--ease);
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .text :global(p) {
    margin: 0;
  }
  .letter {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  .fc-option:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .fc-option:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .fc-option.checked {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .fc-option.checked .letter {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-fg);
  }
  .fc-option:disabled {
    cursor: default;
    color: var(--text-2);
  }
  .fc-option.checked:disabled {
    color: var(--text-1);
  }
  .fc-option.right {
    border-color: var(--success);
    background: var(--success-soft);
    color: var(--text-1);
  }
  .fc-option.right :global(svg) {
    color: var(--success);
  }
  .fc-option.miss {
    border-color: var(--danger);
    background: var(--danger-soft);
    color: var(--text-1);
  }
  .fc-option.miss :global(svg) {
    color: var(--danger);
  }
</style>
