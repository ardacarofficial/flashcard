<!--
  One classification value (v1.1: built from `$lib/ui`). Enum kinds use the custom Select,
  multi-choice kinds a row of toggle chips, the rest an Input. Same props as before.
-->
<script lang="ts">
  import type { AttributeDefDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Chip, Field, Input, Select } from '$lib/ui';
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
  const label = $derived(def.required ? `${def.name} *` : def.name);
  const raw = $derived(toInput(value));
  const chosen = $derived(selectedChoices(value));

  const selectOptions = $derived(
    control === 'tristate'
      ? [
          { value: '', label: t('attribute.unset') },
          { value: 'true', label: t('common.yes') },
          { value: 'false', label: t('common.no') },
        ]
      : [{ value: '', label: t('attribute.unset') }, ...options.map((o) => ({ value: o.id, label: o.label }))],
  );

  function toggle(id: string) {
    const next = chosen.includes(id) ? chosen.filter((c) => c !== id) : [...chosen, id];
    value = fromInput(def.kind, next);
  }
</script>

<div class="attr" data-kind={def.kind}>
  {#if control === 'select' || control === 'tristate'}
    <Select id={inputId} {label} options={selectOptions} value={raw} onchange={(v) => (value = fromInput(def.kind, v))} />
  {:else if control === 'multiselect'}
    <Field {label} id={inputId}>
      {#snippet control(f)}
        <div class="chips" id={f.id} role="group" aria-label={def.name}>
          {#each options as o (o.id)}
            <Chip size="sm" label={o.label} selected={chosen.includes(o.id)} onclick={() => toggle(o.id)} />
          {/each}
        </div>
      {/snippet}
    </Field>
  {:else}
    <Input
      id={inputId}
      {label}
      type={control === 'number' ? 'number' : control === 'date' ? 'date' : 'text'}
      value={raw}
      oninput={(e) => (value = fromInput(def.kind, e.currentTarget.value))}
    />
  {/if}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    min-height: var(--control-h-md);
    align-items: center;
  }
</style>
