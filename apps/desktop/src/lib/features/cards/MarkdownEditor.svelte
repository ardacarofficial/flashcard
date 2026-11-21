<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { LinkedNote } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { completeLink, insertAt, linkQueryAt, mediaSnippet, wrapSelection, type Edit } from './content';
  import { uploadMedia } from './media';
  import { moduleOn } from './modules.svelte';
  import { Button, Chip, Field, ImagePlus } from '$lib/ui';

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

<Field {label} {id}>
  {#snippet control(f)}
    <div class="md" class:dragging>
      <div class="toolbar" role="toolbar" aria-label={t('editor.toolbar')}>
        <Button variant="ghost" size="sm" aria-label={t('editor.bold')} title={t('editor.bold')} onclick={() => wrap('**', '**')}><b>{t('editor.bold_short')}</b></Button>
        <Button variant="ghost" size="sm" aria-label={t('editor.italic')} title={t('editor.italic')} onclick={() => wrap('*', '*')}><i>{t('editor.italic_short')}</i></Button>
        <Button variant="ghost" size="sm" aria-label={t('editor.code')} title={t('editor.code')} onclick={() => wrap('`', '`')}><code>{t('editor.code_short')}</code></Button>
        <Button variant="ghost" size="sm" aria-label={t('editor.math')} title={t('editor.math')} onclick={() => wrap('$', '$')}>∑</Button>
        {#if moduleOn('links')}
          <Button variant="ghost" size="sm" aria-label={t('editor.link')} title={t('editor.link')} onclick={() => wrap('[[', ']]')}>[[ ]]</Button>
        {/if}
        <span class="spacer"></span>
        <Button variant="ghost" size="sm" icon={ImagePlus} loading={busy} aria-label={t('editor.media')} title={t('editor.media')} onclick={() => fileInput?.click()}>{t('editor.media_short')}</Button>
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
        id={f.id}
        aria-describedby={f.describedBy}
        {rows}
        bind:this={area}
        bind:value
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
    </div>
    {#if suggestions.length > 0}
      <ul class="suggest" role="listbox" aria-label={t('editor.link_suggestions')}>
        {#each suggestions as s (s.id)}
          <li role="option" aria-selected="false">
            <Chip size="sm" label={s.title} onclick={() => pick(s)} />
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}<span class="error" role="alert">{error}</span>{/if}
  {/snippet}
</Field>

<style>
  .md {
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    transition: border-color var(--motion-fast) var(--ease);
  }
  .md:focus-within {
    border-color: var(--accent-text);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .md.dragging {
    border-style: dashed;
    border-color: var(--accent-text);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  textarea {
    display: block;
    width: 100%;
    box-sizing: border-box;
    border: none;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    line-height: var(--leading);
    padding: var(--space-3);
    resize: vertical;
    outline: none;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .suggest {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
