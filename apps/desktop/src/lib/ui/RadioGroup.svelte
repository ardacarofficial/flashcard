<!--
  Radio group: a vertical list of choices with optional descriptions (styled native radios in
  a fieldset; arrow keys move). For 2–5 short choices side by side use SegmentedControl.
  `bind:value`.
-->
<script lang="ts" generics="T">
  import type { SelectOption } from './types';

  interface Props {
    value?: T;
    options: SelectOption<T>[];
    /** Group legend. */
    label: string;
    hideLabel?: boolean;
    name?: string;
    disabled?: boolean;
    onchange?: (value: T) => void;
    class?: string;
  }

  const uid = $props.id();
  let { value = $bindable(), options, label, hideLabel = false, name, disabled = false, onchange, class: klass = '' }: Props = $props();

  const group = $derived(name ?? `fc-radio-${uid}`);

  function pick(o: SelectOption<T>) {
    value = o.value;
    onchange?.(o.value);
  }
</script>

<fieldset class="fc-radio {klass}" {disabled} data-fc-ui="radio-group">
  <legend class:sr-only={hideLabel}>{label}</legend>
  {#each options as o, i (i)}
    <label class="opt" class:disabled={o.disabled}>
      <input
        type="radio"
        name={group}
        checked={Object.is(o.value, value)}
        disabled={o.disabled}
        onchange={() => pick(o)}
        aria-describedby={o.description ? `${group}-${i}-desc` : undefined}
      />
      <span class="dot" aria-hidden="true"></span>
      <span class="text">
        <span>{o.label}</span>
        {#if o.description}<span id="{group}-{i}-desc" class="desc">{o.description}</span>{/if}
      </span>
    </label>
  {/each}
</fieldset>

<style>
  .fc-radio {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: none;
    min-width: 0;
  }
  legend {
    padding: 0;
    margin-bottom: var(--space-1);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .opt {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    font-size: var(--text-sm);
    line-height: 20px;
    color: var(--text-1);
    cursor: pointer;
  }
  .opt.disabled {
    opacity: 0.55;
    cursor: default;
  }
  input {
    position: absolute;
    width: 18px;
    height: 18px;
    margin: 1px 0 0;
    opacity: 0;
    cursor: inherit;
  }
  .dot {
    flex: none;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    border: 1.5px solid var(--text-2);
    border-radius: 50%;
    background: var(--surface-1);
  }
  input:checked + .dot {
    border-color: var(--accent);
    background: radial-gradient(circle, var(--accent) 0 4px, var(--surface-1) 5px);
  }
  input:focus-visible + .dot {
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
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
