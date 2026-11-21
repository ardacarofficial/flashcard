<script lang="ts">
  import type { ApiError } from '$lib/api';
  import type { FieldKind } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { resolveMedia } from './media';
  import { Button, Trash2, Upload } from '$lib/ui';

  interface Props {
    value: string;
    label: string;
    id: string;
    kind: FieldKind;
    /** Uploads a file and returns its `media:<hash>` reference. */
    upload: (file: File) => Promise<string>;
    /** Tighter variant (one row: preview, buttons, hint). */
    compact?: boolean;
    required?: boolean;
  }
  let { value = $bindable(), label, id, kind, upload, compact = false, required = false }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let previewHtml = $state('');
  let dragging = $state(false);
  let input = $state<HTMLInputElement | null>(null);
  const accept = $derived(kind === 'audio' ? 'audio/*' : kind === 'video' ? 'video/*' : 'image/*');

  $effect(() => {
    const hashes = [...value.matchAll(/media:([0-9a-f]{64})/g)].map((m) => m[1]!);
    const tag = (h: string) =>
      kind === 'audio'
        ? `<audio controls src="media:${h}"></audio>`
        : kind === 'video'
          ? `<video controls src="media:${h}"></video>`
          : `<img src="media:${h}" alt="">`;
    let cancelled = false;
    void resolveMedia(hashes.map(tag).join(' ')).then((h) => {
      if (!cancelled) previewHtml = h;
    });
    return () => {
      cancelled = true;
    };
  });

  async function take(files: FileList | File[] | null | undefined) {
    const file = files?.[0];
    if (!file) return;
    busy = true;
    error = null;
    try {
      value = await upload(file);
    } catch (e) {
      error = td((e as ApiError).messageKey);
    } finally {
      busy = false;
    }
  }

</script>

<div
  class="media"
  class:compact
  class:dragging
  role="group"
  aria-labelledby={`${id}-label`}
  ondragover={(e) => {
    e.preventDefault();
    dragging = true;
  }}
  ondragleave={() => (dragging = false)}
  ondrop={(e) => {
    e.preventDefault();
    dragging = false;
    void take(e.dataTransfer?.files);
  }}
  onpaste={(e) => {
    if (e.clipboardData?.files.length) {
      e.preventDefault();
      void take(e.clipboardData.files);
    }
  }}
>
  <span class="label" id={`${id}-label`}>{label}{#if required}<span class="req" aria-hidden="true">*</span>{/if}</span>
  <div class="drop">
    {#if previewHtml}
      <div class="thumb">{@html previewHtml}</div>
    {/if}
    <div class="row">
      <Button size="sm" icon={Upload} loading={busy} onclick={() => input?.click()}>{t('editor.choose_file')}</Button>
      {#if value}
        <Button size="sm" variant="ghost" icon={Trash2} onclick={() => (value = '')}>{t('editor.remove_media')}</Button>
      {/if}
      <input bind:this={input} {id} class="file" type="file" {accept} onchange={(e) => void take(e.currentTarget.files)} disabled={busy} />
      <span class="hint">{t('editor.drop_hint')}</span>
    </div>
  </div>
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
  .media {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .label {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .req {
    color: var(--danger);
    margin-inline-start: 2px;
  }
  .drop {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    transition: border-color var(--motion-fast) var(--ease);
  }
  .compact .drop {
    flex-direction: row;
    align-items: center;
    padding: var(--space-1) var(--space-2);
  }
  .dragging .drop {
    border-color: var(--accent-text);
    background: var(--accent-soft);
  }
  .thumb :global(img) {
    display: block;
    max-width: 240px;
    max-height: 160px;
    border-radius: 4px;
  }
  .compact .thumb :global(img) {
    max-width: 64px;
    max-height: 40px;
  }
  .thumb :global(audio) {
    height: 32px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    min-width: 0;
  }
  .file {
    display: none;
  }
  .hint {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>
