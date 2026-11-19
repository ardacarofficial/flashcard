<script lang="ts">
  import type { AttributeDef, AttributeValue } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { controlFor, displayOptions, fromInput } from './attribute-field';

  interface Props {
    def: AttributeDef;
    value: AttributeValue;
  }
  let { def, value = $bindable() }: Props = $props();

  const control = $derived(controlFor(def.kind));
  const options = $derived(displayOptions(def));
  const inputId = $derived(`attr-${def.id}`);

  function onSelect(e: Event) {
    value = fromInput(def.kind, (e.currentTarget as HTMLSelectElement).value);
  }
  function onMulti(e: Event) {
    const el = e.currentTarget as HTMLSelectElement;
    value = fromInput(
      def.kind,
      Array.from(el.selectedOptions, (o) => o.value),
    );
  }
  function onText(e: Event) {
    value = fromInput(def.kind, (e.currentTarget as HTMLInputElement).value);
  }
  function onCheck(e: Event) {
    value = fromInput(def.kind, (e.currentTarget as HTMLInputElement).checked);
  }
</script>

<div class="field" data-kind={def.kind}>
  <label for={inputId}>
    {def.name}
    {#if def.required}<span class="muted" title={t('attribute.required')}>*</span>{/if}
  </label>

  {#if control === 'select'}
    <select id={inputId} required={def.required} value={typeof value === 'string' ? value : ''} onchange={onSelect}>
      <option value="">{t('attribute.unset')}</option>
      {#each options as o (o.id)}
        <option value={o.id}>{o.label}</option>
      {/each}
    </select>
  {:else if control === 'multiselect'}
    <select id={inputId} multiple required={def.required} onchange={onMulti}>
      {#each options as o (o.id)}
        <option value={o.id} selected={Array.isArray(value) && value.includes(o.id)}>{o.label}</option>
      {/each}
    </select>
  {:else if control === 'number'}
    <input id={inputId} type="number" required={def.required} value={typeof value === 'number' ? value : ''} oninput={onText} />
  {:else if control === 'date'}
    <input id={inputId} type="date" required={def.required} value={typeof value === 'string' ? value : ''} oninput={onText} />
  {:else if control === 'checkbox'}
    <input id={inputId} type="checkbox" checked={value === true} onchange={onCheck} />
  {:else}
    <input id={inputId} type="text" required={def.required} value={typeof value === 'string' ? value : ''} oninput={onText} />
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .field[data-kind='bool'] {
    flex-direction: row-reverse;
    justify-content: flex-end;
    align-items: center;
    gap: var(--fc-space-2);
  }
</style>
