<script lang="ts">
  import type { AttributeDefDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    controlFor,
    displayOptions,
    fromInput,
    selectedChoices,
    toInput,
    type FieldValue,
  } from './attribute-field';

  interface Props {
    def: AttributeDefDto;
    value: FieldValue;
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
  const raw = $derived(toInput(value));
  const chosen = $derived(selectedChoices(value));
</script>

<div class="field" data-kind={def.kind}>
  <label for={inputId}>
    {def.name}
    {#if def.required}<span class="muted" title={t('attribute.required')}>*</span>{/if}
  </label>

  {#if control === 'select'}
    <select id={inputId} value={raw} onchange={onSelect}>
      <option value="">{t('attribute.unset')}</option>
      {#each options as o (o.id)}
        <option value={o.id}>{o.label}</option>
      {/each}
    </select>
  {:else if control === 'multiselect'}
    <select id={inputId} multiple onchange={onMulti}>
      {#each options as o (o.id)}
        <option value={o.id} selected={chosen.includes(o.id)}>{o.label}</option>
      {/each}
    </select>
  {:else if control === 'number'}
    <input id={inputId} type="number" value={raw} oninput={onText} />
  {:else if control === 'date'}
    <input id={inputId} type="date" value={raw} oninput={onText} />
  {:else if control === 'tristate'}
    <select id={inputId} value={raw} onchange={onSelect}>
      <option value="">{t('attribute.unset')}</option>
      <option value="true">{t('common.yes')}</option>
      <option value="false">{t('common.no')}</option>
    </select>
  {:else}
    <input id={inputId} type="text" value={raw} oninput={onText} />
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
</style>
