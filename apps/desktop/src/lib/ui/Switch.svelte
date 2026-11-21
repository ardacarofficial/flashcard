<!-- On/off switch for settings that apply at once (`role="switch"`). `bind:checked`. -->
<script lang="ts">
  interface Props {
    checked?: boolean;
    label?: string;
    description?: string;
    /** Accessible name when there is no visible label. */
    ariaLabel?: string;
    disabled?: boolean;
    id?: string;
    onchange?: (checked: boolean) => void;
    class?: string;
  }

  const uid = $props.id();
  let { checked = $bindable(false), label, description, ariaLabel, disabled = false, id, onchange, class: klass = '' }: Props = $props();

  const cid = $derived(id ?? `fc-switch-${uid}`);

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<div class="fc-switch {klass}" class:disabled>
  <button
    id={cid}
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label ? undefined : ariaLabel}
    aria-labelledby={label ? `${cid}-label` : undefined}
    aria-describedby={description ? `${cid}-desc` : undefined}
    {disabled}
    onclick={toggle}
  >
    <span class="thumb"></span>
  </button>
  {#if label || description}
    <span class="text">
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      {#if label}<span id="{cid}-label" class="label" onclick={toggle}>{label}</span>{/if}
      {#if description}<span id="{cid}-desc" class="desc">{description}</span>{/if}
    </span>
  {/if}
</div>

<style>
  .fc-switch {
    display: inline-flex;
    align-items: flex-start;
    gap: var(--space-3);
    font-size: var(--text-sm);
    line-height: 20px;
    color: var(--text-1);
  }
  .disabled {
    opacity: 0.55;
  }
  button {
    position: relative;
    flex: none;
    width: 30px;
    height: 18px;
    margin-top: 1px;
    padding: 0;
    border: none;
    border-radius: var(--radius-pill);
    background: var(--border-strong);
    cursor: pointer;
    transition: background-color var(--motion) var(--ease);
  }
  button:disabled {
    cursor: default;
  }
  button[aria-checked='true'] {
    background: var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-1);
    transition:
      transform var(--motion) var(--ease),
      background-color var(--motion) var(--ease);
  }
  button[aria-checked='true'] .thumb {
    transform: translateX(12px);
    background: var(--accent-fg);
  }
  .text {
    display: flex;
    flex-direction: column;
  }
  .label {
    cursor: pointer;
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading-tight);
  }
</style>
