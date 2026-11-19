<script lang="ts">
  import type { ApiError } from '$lib/api';
  import type { FieldKind } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { resolveMedia } from './media';

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
  role="group"
  aria-labelledby={`${id}-label`}
  ondragover={(e) => e.preventDefault()}
  ondrop={(e) => {
    e.preventDefault();
    void take(e.dataTransfer?.files);
  }}
  onpaste={(e) => {
    if (e.clipboardData?.files.length) {
      e.preventDefault();
      void take(e.clipboardData.files);
    }
  }}
>
  <span id={`${id}-label`}>{label}</span>
  {#if previewHtml}
    <div class="thumb">{@html previewHtml}</div>
  {/if}
  <div class="row">
    <input {id} type="file" {accept} onchange={(e) => void take(e.currentTarget.files)} disabled={busy} />
    {#if value}
      <button type="button" class="small" onclick={() => (value = '')}>{t('editor.remove_media')}</button>
    {/if}
  </div>
  <span class="muted hint">{t('editor.drop_hint')}</span>
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
  .media {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .thumb :global(img) {
    max-width: 240px;
    max-height: 160px;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .hint {
    font-size: 0.85em;
  }
</style>
