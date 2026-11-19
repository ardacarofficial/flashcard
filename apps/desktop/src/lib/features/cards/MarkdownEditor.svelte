<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { LinkedNote } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { completeLink, insertAt, linkQueryAt, mediaSnippet, wrapSelection, type Edit } from './content';
  import { uploadMedia } from './media';
  import { moduleOn } from './modules.svelte';

  interface Props {
    value: string;
    label: string;
    id: string;
    rows?: number;
  }
  let { value = $bindable(), label, id, rows = 3 }: Props = $props();

  let area = $state<HTMLTextAreaElement | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let suggestions = $state<LinkedNote[]>([]);
  let dragging = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);

  function apply(edit: Edit) {
    value = edit.text;
    queueMicrotask(() => {
      area?.focus();
      area?.setSelectionRange(edit.start, edit.end);
    });
  }

  function sel(): [number, number] {
    return [area?.selectionStart ?? value.length, area?.selectionEnd ?? value.length];
  }

  function wrap(before: string, after: string) {
    const [s, e] = sel();
    apply(wrapSelection(value, s, e, before, after));
  }

  async function addFiles(files: Iterable<File>) {
    busy = true;
    error = null;
    try {
      for (const file of files) {
        const media = await uploadMedia(file);
        const [s, e] = sel();
        apply(insertAt(value, s, e, mediaSnippet(media)));
      }
    } catch (e) {
      error = td((e as ApiError).messageKey);
    } finally {
      busy = false;
    }
  }

  function onpaste(e: ClipboardEvent) {
    const files = [...(e.clipboardData?.files ?? [])];
    if (files.length) {
      e.preventDefault();
      void addFiles(files);
    }
  }

  function ondrop(e: DragEvent) {
    dragging = false;
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length) {
      e.preventDefault();
      void addFiles(files);
    }
  }

  let seq = 0;
  async function oninput() {
    if (!moduleOn('links')) return;
    const q = linkQueryAt(value, sel()[0]);
    const mine = ++seq;
    if (q === null || q.trim() === '') {
      suggestions = [];
      return;
    }
    try {
      const r = await api.call('links.suggest', { prefix: q, limit: 8 });
      if (mine === seq) suggestions = r.items;
    } catch {
      suggestions = [];
    }
  }

  function pick(n: LinkedNote) {
    apply(completeLink(value, sel()[0], n.title));
    suggestions = [];
  }
</script>

<div class="md">
  <label for={id}>{label}</label>
  <div class="toolbar" role="toolbar" aria-label={t('editor.toolbar')}>
    <button type="button" class="small" title={t('editor.bold')} onclick={() => wrap('**', '**')}><b>{t('editor.bold_short')}</b></button>
    <button type="button" class="small" title={t('editor.italic')} onclick={() => wrap('*', '*')}><i>{t('editor.italic_short')}</i></button>
    <button type="button" class="small" title={t('editor.code')} onclick={() => wrap('`', '`')}>{t('editor.code_short')}</button>
    <button type="button" class="small" title={t('editor.math')} onclick={() => wrap('$', '$')}>∑</button>
    {#if moduleOn('links')}
      <button type="button" class="small" title={t('editor.link')} onclick={() => wrap('[[', ']]')}>[[ ]]</button>
    {/if}
    <button type="button" class="small" title={t('editor.media')} disabled={busy} onclick={() => fileInput?.click()}>＋</button>
    <input
      bind:this={fileInput}
      type="file"
      accept="image/*,audio/*,video/*"
      multiple
      hidden
      onchange={(e) => void addFiles([...(e.currentTarget.files ?? [])])}
    />
  </div>
  <textarea
    {id}
    {rows}
    bind:this={area}
    bind:value
    class:dragging
    {onpaste}
    {ondrop}
    oninput={() => void oninput()}
    ondragover={(e) => {
      e.preventDefault();
      dragging = true;
    }}
    ondragleave={() => (dragging = false)}
    placeholder={t('editor.placeholder')}
  ></textarea>
  {#if suggestions.length > 0}
    <ul class="suggest" role="listbox" aria-label={t('editor.link_suggestions')}>
      {#each suggestions as s (s.id)}
        <li role="option" aria-selected="false">
          <button type="button" class="small" onclick={() => pick(s)}>{s.title}</button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if busy}<span class="muted">{t('editor.uploading')}</span>{/if}
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
  .md {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .toolbar {
    display: flex;
    gap: var(--fc-space-1);
  }
  textarea.dragging {
    outline: 2px dashed var(--fc-accent);
  }
  .suggest {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-1);
  }
</style>
