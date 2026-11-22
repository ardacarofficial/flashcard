<!--
  One field of a note type in the everyday view (v1.3): its number, name, what it contains
  and whether it is required, plus move up / down / remove. The "what it contains" cell is
  the `contains` snippet when the caller passes one (a picker), else a plain description of
  the field's kind. Technical flags (kind of an existing field, unique, searchable) live in
  the "Advanced: template code" part of the page.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { FieldKind } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Checkbox, ChevronDown, ChevronUp, IconButton, Input, Trash2 } from '$lib/ui';

  interface Props {
    /** Field name (user text). */
    name: string;
    /** Must not be empty. */
    required: boolean;
    kind: FieldKind;
    /** 0-based position. */
    index: number;
    /** Number of fields (disables "down" on the last one). */
    count: number;
    /** Control (or text) for "what this field contains"; default: a description of its kind. */
    contains?: Snippet;
    onmove: (delta: -1 | 1) => void;
    onremove: () => void;
  }

  let { name = $bindable(), required = $bindable(), kind, index, count, contains, onmove, onremove }: Props = $props();
  const uid = $props.id();
  const label = $derived(name.trim() || t('notetype.field_n', { n: index + 1 }));
</script>

<li class="frow" data-testid="field-row">
  <span class="n num" aria-hidden="true">{index + 1}</span>
  <div class="name">
    <Input size="sm" id="fc-field-{uid}" bind:value={name} label={t('notetype.field_name_n', { n: index + 1 })} hideLabel />
  </div>
  <div class="req">
    <Checkbox bind:checked={required} label={t('notetype.required')} ariaLabel={t('notetype.required_of', { name: label })} />
  </div>
  <div class="acts">
    <IconButton size="sm" icon={ChevronUp} label={t('notetype.move_field_up', { name: label })} disabled={index === 0} onclick={() => onmove(-1)} />
    <IconButton size="sm" icon={ChevronDown} label={t('notetype.move_field_down', { name: label })} disabled={index === count - 1} onclick={() => onmove(1)} />
    <IconButton size="sm" icon={Trash2} label={t('notetype.remove_field', { name: label })} onclick={onremove} />
  </div>
  <div class="contains">
    <span class="clabel">{t('notetype.contains_label')}</span>
    {#if contains}{@render contains()}{:else}<span class="ctext">{t(`notetype.contains.${kind}`)}</span>{/if}
  </div>
</li>

<style>
  .frow {
    display: grid;
    grid-template-columns: 1.25rem minmax(0, 1fr) auto auto;
    grid-template-areas:
      'n name req acts'
      '. contains contains contains';
    gap: 6px var(--space-3);
    align-items: center;
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  .n {
    grid-area: n;
    color: var(--text-2);
    font-size: var(--text-sm);
    text-align: end;
  }
  .num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .name {
    grid-area: name;
    min-width: 0;
  }
  .req {
    grid-area: req;
    font-size: var(--text-sm);
    white-space: nowrap;
  }
  .acts {
    grid-area: acts;
    display: flex;
    gap: 2px;
  }
  .contains {
    grid-area: contains;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px var(--space-2);
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .contains :global(.fc-field) {
    /* Wide enough for the longest role ("The word (dictionary form)") in both languages. */
    flex: 0 1 17rem;
    min-width: 0;
  }
  .clabel {
    color: var(--text-2);
  }
</style>
