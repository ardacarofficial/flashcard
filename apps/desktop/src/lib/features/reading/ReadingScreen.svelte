<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ReadingItemDto, ReadingItemList, ReadingPriority, ReadingSource } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { pct100 } from '../stats/format';
  import { pickFile } from '../io/files';
  import { followJob, isFinal, resultOf } from '../io/job';
  import Reader from './Reader.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  type Tab = 'library' | 'queue';
  const PRIORITIES: ReadingPriority[] = ['high', 'normal', 'low'];
  const TEXT_FILES = [{ name: 'Text', extensions: ['txt', 'md', 'html', 'htm', 'pdf', 'srt', 'vtt'] }];
  const MEDIA_FILES = [{ name: 'Video', extensions: ['mp4', 'mkv', 'webm', 'avi', 'mov', 'mp3', 'm4a', 'ogg', 'wav'] }];

  let tab = $state<Tab>('library');
  let list = $state<ReadingItemList | null>(null);
  let open = $state<ReadingItemDto | null>(null);
  let adding = $state(false);
  let title = $state('');
  let language = $state('');
  let body = $state('');
  let url = $state('');
  let mediaPath = $state<string | null>(null);
  let queue = $state(false);
  let priority = $state<ReadingPriority>('normal');
  let importing = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    await ctx.guarded(async () => {
      list = await api.call('reading.items.list', { queued_only: tab === 'queue' });
    });
  }

  onMount(() => {
    void load();
    const unsub = api.on(['reading_items_changed', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });

  function switchTab(next: Tab) {
    tab = next;
    void load();
  }

  async function save() {
    const ok = await ctx.guarded(async () => {
      open = await api.call('reading.items.create', {
        title,
        language,
        body,
        cues: null,
        source: null,
        media_path: null,
        queue,
        priority,
      });
    });
    if (ok) reset();
  }

  function reset() {
    adding = false;
    title = '';
    body = '';
    url = '';
    mediaPath = null;
  }

  async function runImport(source: ReadingSource) {
    importing = true;
    error = null;
    try {
      const job = await api.call('reading.items.import', {
        source,
        title,
        language,
        media_path: mediaPath,
        queue,
        priority,
      });
      const done = isFinal(job) ? job : await followJob(job, () => {});
      const item = resultOf<ReadingItemDto>(done);
      if (item) {
        reset();
        open = item;
      } else error = td(done.error?.message_key ?? 'errors.internal.unexpected');
    } catch (e) {
      error = td((e as { message_key?: string }).message_key ?? 'errors.internal.unexpected');
    } finally {
      importing = false;
    }
  }

  async function openFile() {
    const path = await pickFile(TEXT_FILES, t('reading.add.path_prompt'));
    if (path) await runImport({ type: 'file', path });
  }

  async function pickMedia() {
    mediaPath = await pickFile(MEDIA_FILES, t('reading.add.media_prompt'));
  }

  async function toggleQueue(item: ReadingItemDto) {
    await ctx.guarded(() => api.call('reading.queue.set', { id: item.id, queued: !item.queued, priority: null }));
  }
</script>

{#if open}
  <Reader {ctx} item={open} onclose={() => { open = null; void load(); }} />
{:else}
  <h2>{t('reading.heading')}</h2>
  <div class="row" role="tablist">
    <button role="tab" aria-selected={tab === 'library'} onclick={() => switchTab('library')}>{t('reading.tab.library')}</button>
    <button role="tab" aria-selected={tab === 'queue'} onclick={() => switchTab('queue')}>
      {t('reading.tab.queue')}{#if list && list.due_count > 0} ({list.due_count}){/if}</button>
    <button onclick={() => (adding = !adding)} aria-expanded={adding}>{t('reading.add.open')}</button>
  </div>

  {#if adding}
    <form class="add" onsubmit={(e) => { e.preventDefault(); void save(); }}>
      <div class="row">
        <label>{t('reading.add.title')} <input bind:value={title} /></label>
        <label>{t('reading.add.language')} <input bind:value={language} size="6" /></label>
        <label><input type="checkbox" bind:checked={queue} /> {t('reading.add.queue')}</label>
        {#if queue}
          <label>{t('reading.priority.label')}
            <select bind:value={priority}>{#each PRIORITIES as p (p)}<option value={p}>{t(`reading.priority.${p}`)}</option>{/each}</select></label>
        {/if}
      </div>
      <label>{t('reading.add.body')} <textarea rows="8" bind:value={body}></textarea></label>
      <div class="row">
        <button type="submit" disabled={!body.trim()}>{t('reading.add.save')}</button>
        <button type="button" onclick={openFile} disabled={importing}>{t('reading.add.file')}</button>
        <button type="button" onclick={pickMedia}>{mediaPath ? t('reading.add.media_set') : t('reading.add.media')}</button>
        <label>{t('reading.add.url')} <input type="url" bind:value={url} /></label>
        <button type="button" disabled={importing || !url.trim()} onclick={() => runImport({ type: 'url', url })}>{t('reading.add.fetch')}</button>
      </div>
      {#if importing}<p role="status">{t('reading.add.importing')}</p>{/if}
      {#if error}<p role="alert">{error}</p>{/if}
    </form>
  {/if}

  {#if list && list.items.length === 0}
    <p class="muted">{tab === 'queue' ? t('reading.queue.empty') : t('reading.empty')}</p>
  {/if}
  <ul>
    {#each list?.items ?? [] as item (item.id)}
      <li class="row">
        <button class="title" onclick={() => (open = item)}>{item.title}</button>
        <span class="muted">{t(`reading.kind.${item.kind}`)} · {item.language}</span>
        <span>{pct100(item.progress_pct)}</span>
        {#if item.queued}
          <span class:due={item.is_due}>{item.is_due ? t('reading.queue.due_now') : t('reading.queue.in_days', { n: item.interval_days })}</span>
        {/if}
        <button onclick={() => toggleQueue(item)}>{item.queued ? t('reading.queue.remove') : t('reading.queue.add')}</button>
        <button onclick={() => ctx.guarded(() => api.call('reading.items.delete', { id: item.id }))}>{t('reading.delete')}</button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
    align-items: center;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
    margin: var(--fc-space-4) 0;
  }
  textarea {
    width: 100%;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  .title {
    font-weight: 600;
  }
  .due {
    font-weight: 600;
  }
</style>
