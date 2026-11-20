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
  import {
    BookOpen,
    Button,
    Card,
    Chip,
    Dialog,
    EmptyState,
    FileText,
    IconButton,
    Input,
    PageHeader,
    Plus,
    ProgressBar,
    SegmentedControl,
    Skeleton,
    Switch,
    Tabs,
    Textarea,
    Trash2,
    Upload,
  } from '$lib/ui';

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

  function switchTab(next: string) {
    tab = next as Tab;
    list = null;
    void load();
  }

  const tabs = $derived([
    { id: 'library', label: t('reading.tab.library') },
    { id: 'queue', label: t('reading.tab.queue'), badge: list?.due_count ?? null },
  ]);
  const priorityOptions = $derived(PRIORITIES.map((p) => ({ value: p, label: t(`reading.priority.${p}`) })));

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
  <div class="reading">
    <PageHeader title={t('reading.heading')} subtitle={t('reading.subtitle')}>
      {#snippet actions()}
        <Button variant="primary" icon={Plus} onclick={() => (adding = true)} data-testid="open-dialog-new-text">{t('reading.add.open')}</Button>
      {/snippet}
    </PageHeader>

    <Tabs label={t('reading.heading')} {tabs} value={tab} onchange={switchTab} />

    {#if !list}
      <div class="items" aria-busy="true">
        {#each [0, 1, 2] as i (i)}<Skeleton shape="card" height="88px" />{/each}
      </div>
    {:else if list.items.length === 0}
      <Card padding="lg">
        <EmptyState
          icon={BookOpen}
          title={tab === 'queue' ? t('reading.queue.empty') : t('reading.empty')}
          action={tab === 'queue' ? undefined : { label: t('reading.add.first'), onclick: () => (adding = true) }}
        />
      </Card>
    {:else}
      <ul class="items">
        {#each list.items as item (item.id)}
          <li class="item">
            <div class="main">
              <button class="title" onclick={() => (open = item)}>{item.title}</button>
              <div class="meta">
                <Chip size="sm" label={t(`reading.kind.${item.kind}`)} />
                {#if item.language}<Chip size="sm" label={item.language} />{/if}
                {#if item.queued}
                  <Chip size="sm" tone={item.is_due ? 'accent' : 'neutral'} label={item.is_due ? t('reading.queue.due_now') : t('reading.queue.in_days', { n: item.interval_days })} />
                {/if}
              </div>
            </div>
            <div class="progress">
              <ProgressBar label={t('reading.progress')} value={item.progress_pct} max={100} size="sm" />
              <span class="pct">{pct100(item.progress_pct)}</span>
            </div>
            <div class="actions">
              <Button size="sm" variant="ghost" onclick={() => toggleQueue(item)}>{item.queued ? t('reading.queue.remove') : t('reading.queue.add')}</Button>
              <IconButton size="sm" icon={Trash2} label={t('reading.delete_named', { name: item.title })} onclick={() => ctx.guarded(() => api.call('reading.items.delete', { id: item.id }))} />
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<Dialog bind:open={adding} title={t('reading.add.open')} description={t('reading.add.description')} size="lg">
  <form
    id="reading-add"
    class="add"
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="two">
      <Input label={t('reading.add.title')} bind:value={title} />
      <Input label={t('reading.add.language')} bind:value={language} placeholder={t('reading.add.language_placeholder')} />
    </div>
    <Textarea label={t('reading.add.body')} rows={8} bind:value={body} />
    <div class="queue">
      <Switch label={t('reading.add.queue')} bind:checked={queue} />
      {#if queue}
        <SegmentedControl size="sm" label={t('reading.priority.label')} options={priorityOptions} bind:value={priority} />
      {/if}
    </div>
    <fieldset class="sources">
      <legend>{t('reading.add.other_sources')}</legend>
      <div class="row">
        <Button size="sm" icon={FileText} onclick={openFile} disabled={importing}>{t('reading.add.file')}</Button>
        <Button size="sm" icon={Upload} onclick={pickMedia}>{mediaPath ? t('reading.add.media_set') : t('reading.add.media')}</Button>
      </div>
      <div class="row url">
        <Input size="sm" type="url" label={t('reading.add.url')} bind:value={url} class="grow" />
        <Button size="sm" disabled={importing || !url.trim()} loading={importing} onclick={() => runImport({ type: 'url', url })}>{t('reading.add.fetch')}</Button>
      </div>
    </fieldset>
    {#if importing}<p class="muted" role="status">{t('reading.add.importing')}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (adding = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="reading-add" disabled={!body.trim()}>{t('reading.add.save')}</Button>
  {/snippet}
</Dialog>

<style>
  .reading {
    max-width: 960px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .reading :global(.fc-page-header) {
    margin-bottom: 0;
  }
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 200px 150px;
    gap: var(--space-6);
    align-items: center;
    padding: var(--space-4) var(--space-4) var(--space-4) var(--space-6);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-1);
  }
  .main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
  }
  .title {
    all: unset;
    cursor: pointer;
    font-weight: var(--weight-semibold);
    font-size: var(--text-md);
    overflow-wrap: anywhere;
  }
  .title:hover {
    color: var(--accent);
  }
  .title:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .actions {
    justify-content: flex-end;
  }
  .meta,
  .actions,
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .progress :global(.fc-progress) {
    flex: 1;
  }
  .pct {
    font-size: var(--text-sm);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    min-width: 3ch;
    text-align: end;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .two {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
    gap: var(--space-4);
  }
  .queue {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    align-items: center;
  }
  .sources {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-3) var(--space-4) var(--space-4);
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  legend {
    font-size: var(--text-sm);
    color: var(--text-2);
    padding: 0 var(--space-1);
  }
  .url {
    align-items: flex-end;
  }
  .url :global(.grow) {
    flex: 1;
  }
  .muted {
    color: var(--text-2);
    margin: 0;
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
  @media (max-width: 760px) {
    .item {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-3);
    }
  }
</style>
