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
  }
  let { value = $bindable(), label, id, kind, upload }: Props = $props();

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
  <span class="label" id={`${id}-label`}>{label}</span>
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
    </div>
    <span class="hint">{t('editor.drop_hint')}</span>
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
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .drop {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-1);
  }
  .dragging .drop {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .thumb :global(img) {
    max-width: 240px;
    max-height: 160px;
    border-radius: var(--radius-control);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .file {
    display: none;
  }
  .hint {
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
