<!--
  One note field as its input: rich text (MarkdownEditor), media, the image-occlusion mask
  editor, a number or plain text. Label above, required marked by the field frame.
  `compact` (v1.2, optional): small inputs and 2-row text areas for tight dialogs (Quick Add).
-->
<script lang="ts">
  import type { FieldDef } from '$lib/api/types';
  import MarkdownEditor from './MarkdownEditor.svelte';
  import MediaField from './MediaField.svelte';
  import MaskEditor from './MaskEditor.svelte';
  import { uploadMedia } from './media';
  import { Input, Textarea } from '$lib/ui';

  interface Props {
    field: FieldDef;
    value: string;
    /** Whole note (the mask editor reads the image field). */
    values: Record<string, string>;
    /** Engine of the note type's templates (`image_occlusion` gets a mask editor). */
    engines: string[];
    /** Tighter variant (sm inputs, fewer text rows). */
    compact?: boolean;
  }
  let { field, value = $bindable(), values, engines, compact = false }: Props = $props();

  const id = $derived(`field-${field.key}`);
  const label = $derived(field.name);
  const isMasks = $derived(field.key === 'masks' && engines.includes('image_occlusion'));
  const size = $derived(compact ? ('sm' as const) : ('md' as const));

  async function upload(file: File): Promise<string> {
    return (await uploadMedia(file)).reference;
  }
</script>

{#if isMasks}
  <MaskEditor bind:value image={values['image'] ?? ''} {label} required={field.required} />
{:else if field.kind === 'rich_text'}
  <MarkdownEditor bind:value {label} {id} rows={compact ? 2 : 3} required={field.required} />
{:else if field.kind === 'image' || field.kind === 'audio' || field.kind === 'video'}
  <MediaField bind:value {label} {id} kind={field.kind} {upload} {compact} required={field.required} />
{:else if field.kind === 'number'}
  <Input {id} {label} {size} required={field.required} inputmode="decimal" bind:value />
{:else if field.kind === 'text' && field.key === 'distractors'}
  <Textarea {id} {label} required={field.required} rows={compact ? 2 : 3} autoGrow bind:value />
{:else}
  <Input {id} {label} {size} required={field.required} bind:value />
{/if}
