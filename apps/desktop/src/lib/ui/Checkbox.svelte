<!--
  Checkbox (a styled native checkbox: Space toggles, label click toggles). `bind:checked`;
  `indeterminate` for "some selected". Give `label`, or `children` for rich label content,
  or (in tables) only `aria-label` via `ariaLabel`.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import { Check, Minus } from './icons';

  interface Props {
    checked?: boolean;
    indeterminate?: boolean;
    label?: string;
    description?: string;
    /** Accessible name when there is no visible label. */
    ariaLabel?: string;
    disabled?: boolean;
    name?: string;
    value?: string;
    id?: string;
    onchange?: (checked: boolean) => void;
    children?: Snippet;
    class?: string;
  }

  const uid = $props.id();
  let {
    checked = $bindable(false),
    indeterminate = false,
    label,
    description,
    ariaLabel,
    disabled = false,
    name,
    value,
    id,
    onchange,
    children,
    class: klass = '',
  }: Props = $props();

  const cid = $derived(id ?? `fc-check-${uid}`);
</script>

<label class="fc-check {klass}" class:disabled for={cid} data-fc-ui="checkbox">
  <span class="box-wrap">
    <input
      id={cid}
      type="checkbox"
      bind:checked
      {indeterminate}
      {disabled}
      {name}
      {value}
      aria-label={ariaLabel}
      aria-describedby={description ? `${cid}-desc` : undefined}
      onchange={() => onchange?.(checked)}
    />
    <span class="box" aria-hidden="true">
      {#if indeterminate}<Icon icon={Minus} size={12} strokeWidth={3} />{:else if checked}<Icon icon={Check} size={12} strokeWidth={3} />{/if}
    </span>
  </span>
  {#if label || children || description}
    <span class="text">
      {#if children}{@render children()}{:else if label}<span class="label">{label}</span>{/if}
      {#if description}<span id="{cid}-desc" class="desc">{description}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .fc-check {
    display: inline-flex;
    align-items: flex-start;
    gap: var(--space-2);
    cursor: pointer;
    color: var(--text-1);
    font-size: var(--text-sm);
    line-height: 20px;
  }
  .fc-check:hover .box {
    border-color: var(--text-2);
  }
  .disabled {
    opacity: 0.55;
    cursor: default;
  }
  .box-wrap {
    position: relative;
    display: inline-flex;
    flex: none;
    width: 16px;
    height: 16px;
    margin-top: 2px;
  }
  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .box {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: 1px solid var(--text-3);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--accent-fg);
    transition: background-color var(--motion-fast) var(--ease), border-color var(--motion-fast) var(--ease);
  }
  input:checked + .box,
  input:indeterminate + .box {
    background: var(--accent);
    border-color: var(--accent);
  }
  input:focus-visible + .box {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .text {
    display: flex;
    flex-direction: column;
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading-tight);
  }
</style>
