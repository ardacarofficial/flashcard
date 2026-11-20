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
  }
  let { field, value = $bindable(), values, engines }: Props = $props();

  const id = $derived(`field-${field.key}`);
  const label = $derived(field.required ? `${field.name} *` : field.name);
  const isMasks = $derived(field.key === 'masks' && engines.includes('image_occlusion'));

  async function upload(file: File): Promise<string> {
    return (await uploadMedia(file)).reference;
  }
</script>

{#if isMasks}
  <MaskEditor bind:value image={values['image'] ?? ''} {label} />
{:else if field.kind === 'rich_text'}
  <MarkdownEditor bind:value {label} {id} />
{:else if field.kind === 'image' || field.kind === 'audio' || field.kind === 'video'}
  <MediaField bind:value {label} {id} kind={field.kind} {upload} />
{:else if field.kind === 'number'}
  <Input {id} {label} inputmode="decimal" bind:value />
{:else if field.kind === 'text' && field.key === 'distractors'}
  <Textarea {id} {label} rows={3} autoGrow bind:value />
{:else}
  <Input {id} {label} bind:value />
{/if}
