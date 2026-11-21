<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { LinkedNote } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { completeLink, insertAt, linkQueryAt, mediaSnippet, wrapSelection, type Edit } from './content';
  import { uploadMedia } from './media';
  import { moduleOn } from './modules.svelte';
  import { Chip, Field, Icon, ImagePlus, Spinner } from '$lib/ui';

  interface Props {
    value: string;
    label: string;
    id: string;
    rows?: number;
    required?: boolean;
  }
  let { value = $bindable(), label, id, rows = 3, required = false }: Props = $props();

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

  /** Ctrl/⌘+B and +I format the selection (the tool strip is skipped by Tab: field to field). */
  function onkey(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    const k = e.key.toLowerCase();
    if (k !== 'b' && k !== 'i') return;
    e.preventDefault();
    e.stopPropagation();
    if (k === 'b') wrap('**', '**');
    else wrap('*', '*');
  }

  function pick(n: LinkedNote) {
    apply(completeLink(value, sel()[0], n.title));
    suggestions = [];
  }
</script>

<Field {label} {id} {required}>
  {#snippet control(f)}
    <div class="md" class:dragging>
      <textarea
        id={f.id}
        aria-describedby={f.describedBy}
        {rows}
        bind:this={area}
        bind:value
        {onpaste}
        {ondrop}
        oninput={() => void oninput()}
        onkeydown={onkey}
        ondragover={(e) => {
          e.preventDefault();
          dragging = true;
        }}
        ondragleave={() => (dragging = false)}
        placeholder={t('editor.placeholder')}
      ></textarea>
      <div class="toolbar" role="toolbar" aria-label={t('editor.toolbar')}>
        <button type="button" tabindex="-1" class="tool" aria-label={t('editor.bold')} title={t('editor.bold')} onclick={() => wrap('**', '**')}><b>{t('editor.bold_short')}</b></button>
        <button type="button" tabindex="-1" class="tool" aria-label={t('editor.italic')} title={t('editor.italic')} onclick={() => wrap('*', '*')}><i>{t('editor.italic_short')}</i></button>
        <button type="button" tabindex="-1" class="tool mono" aria-label={t('editor.code')} title={t('editor.code')} onclick={() => wrap('`', '`')}>{t('editor.code_short')}</button>
        <button type="button" tabindex="-1" class="tool" aria-label={t('editor.math')} title={t('editor.math')} onclick={() => wrap('$', '$')}>∑</button>
        {#if moduleOn('links')}
          <button type="button" tabindex="-1" class="tool mono" aria-label={t('editor.link')} title={t('editor.link')} onclick={() => wrap('[[', ']]')}>[[ ]]</button>
        {/if}
        <span class="spacer"></span>
        <button type="button" tabindex="-1" class="tool media" aria-label={t('editor.media')} title={t('editor.media')} disabled={busy} onclick={() => fileInput?.click()}>
          {#if busy}<Spinner size={14} />{:else}<Icon icon={ImagePlus} size={14} />{/if}
          <span>{t('editor.media_short')}</span>
        </button>
        <input
          bind:this={fileInput}
          type="file"
          accept="image/*,audio/*,video/*"
          multiple
          hidden
          onchange={(e) => void addFiles([...(e.currentTarget.files ?? [])])}
        />
      </div>
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
  /* Text first, the formatting tools as a quiet strip under it (Tab goes field to field;
     the tools are reachable with the mouse or Shift+Tab). */
  .md {
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    transition: border-color var(--motion-fast) var(--ease);
  }
  .md:hover {
    border-color: var(--text-3);
  }
  .md:focus-within {
    border-color: var(--accent-text);
    box-shadow: 0 0 0 1px var(--accent-text);
  }
  .md.dragging {
    border-style: dashed;
    border-color: var(--accent-text);
    background: var(--accent-soft);
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
    padding: var(--space-2) var(--space-3) var(--space-1);
    resize: vertical;
    outline: none;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px var(--space-1) var(--space-1);
  }
  .tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-width: 24px;
    height: 22px;
    padding: 0 var(--space-1);
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-xs);
    line-height: 1;
    cursor: pointer;
  }
  .tool.mono {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
  }
  .tool:hover {
    background: var(--surface-3);
    color: var(--text-1);
  }
  .tool:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .tool:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .spacer {
    flex: 1;
  }
  .suggest {
    list-style: none;
    margin: var(--space-1) 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>
