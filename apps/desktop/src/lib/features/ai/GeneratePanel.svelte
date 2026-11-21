<script lang="ts">
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type {
    AiNoteDraft,
    AiPreview,
    AiSourceInput,
    AiTask,
    AttributeAssignment,
    EffectiveAttributeDto,
    JobDto,
    NoteTypeDto,
  } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Checkbox, EmptyState, FileText, Input, Library, SegmentedControl, Select, Sparkles, Textarea } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import JobProgress from '../io/JobProgress.svelte';
  import { pickFile } from '../io/files';
  import { aiResult, extractSource, previewTask, sendPreviewed, usd } from './ai';
  import PreviewPanel from './PreviewPanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let notetypes = $state<NoteTypeDto[]>([]);
  let notetypeId = $state<string>(CLASSIC_NOTETYPE_ID);
  let defs = $state<EffectiveAttributeDto[]>([]);
  let attrs = $state<Record<string, FieldValue>>({});
  let source = $state('');
  let count = $state(10);
  let instructions = $state('');
  let preview = $state.raw<AiPreview | null>(null);
  let task = $state.raw<AiTask | null>(null);
  let job = $state<JobDto | null>(null);
  let drafts = $state<{ keep: boolean; draft: AiNoteDraft }[]>([]);
  let attributes = $state.raw<AttributeAssignment[]>([]);
  let spent = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  /** Where the source text came from (progressive: shown only once used). */
  let from = $state<'text' | 'pdf' | 'url'>('text');
  let pdfPath = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let fromPage = $state(1);
  let toPage = $state(1);
  let url = $state('');
  let sourceInfo = $state<string | null>(null);

  const notetype = $derived(notetypes.find((n) => n.id === notetypeId) ?? null);
  const fieldName = (key: string) => notetype?.fields.find((f) => f.key === key)?.name ?? key;
  const kept = $derived(drafts.filter((d) => d.keep).length);
  const notetypeOptions = $derived(notetypes.map((n) => ({ value: n.id, label: n.name })));
  const sourceOptions = $derived([
    { value: 'text' as const, label: t('ai.source.text') },
    { value: 'pdf' as const, label: t('ai.source.pdf') },
    { value: 'url' as const, label: t('ai.source.url') },
  ]);
  /** Number inputs bind strings through lib/ui Input: keep the numbers. */
  const num = (v: string, fallback: number) => (Number.isFinite(Number(v)) && v.trim() !== '' ? Number(v) : fallback);

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  $effect(() => {
    void api.call('notetypes.list', {}).then((r) => (notetypes = r.notetypes), fail);
  });

  $effect(() => {
    const deck = ctx.selectedDeck;
    defs = [];
    if (!deck) return;
    void api.attribute.effective(deck).then((r) => {
      defs = r.attributes;
      attrs = Object.fromEntries(r.attributes.map((a) => [a.def.id, a.suggested_value ?? initialValue(a.def)]));
    }, fail);
  });

  async function readSource(input: AiSourceInput) {
    busy = true;
    error = null;
    notice = null;
    sourceInfo = null;
    try {
      const { job: done, text } = await extractSource(input, (j) => (job = j));
      if (!text) {
        error = td(done.error?.message_key ?? 'errors.internal.unexpected');
        return;
      }
      source = text.text;
      preview = null;
      if (text.page_count !== null) {
        pageCount = text.page_count;
        fromPage = text.from_page ?? 1;
        toPage = text.to_page ?? text.page_count;
        sourceInfo = t('ai.source.pages_read', { from: fromPage, to: toPage, count: text.page_count });
      } else {
        sourceInfo = text.title ? t('ai.source.page_read', { title: text.title }) : null;
      }
      if (text.truncated) notice = t('ai.source.truncated');
    } catch (e) {
      fail(e);
    } finally {
      job = null;
      busy = false;
    }
  }

  async function choosePdf() {
    const path = await pickFile([{ name: 'PDF', extensions: ['pdf'] }], t('ai.source.pdf_prompt'));
    if (!path) return;
    pdfPath = path;
    pageCount = null;
    await readSource({ kind: 'pdf', path, from_page: null, to_page: null });
  }

  const readPages = () =>
    pdfPath && readSource({ kind: 'pdf', path: pdfPath, from_page: fromPage, to_page: toPage });

  const readUrl = () => url.trim() && readSource({ kind: 'url', url: url.trim() });

  async function runPreview() {
    error = null;
    notice = null;
    drafts = [];
    const deck = ctx.selectedDeck;
    if (!deck) return;
    const next: AiTask = {
      feature: 'generate_cards',
      deck_id: deck,
      notetype_id: notetypeId,
      source,
      count,
      instructions: instructions.trim() || null,
      images: [],
      attributes: toAssignments($state.snapshot(attrs)),
    };
    try {
      preview = await previewTask(next);
      task = next;
    } catch (e) {
      fail(e);
    }
  }

  async function send() {
    if (!preview || !task) return;
    busy = true;
    error = null;
    try {
      const done = await sendPreviewed(task, preview, undefined, (j) => (job = j));
      preview = null;
      const r = aiResult(done);
      if (!r) {
        error = td(done.error?.message_key ?? 'errors.internal.unexpected');
      } else if (r.output.feature === 'generate_cards') {
        drafts = r.output.notes.map((draft) => ({ keep: true, draft }));
        attributes = r.output.attributes;
        spent = usd(r.cost_usd);
        if (r.truncated) notice = t('ai.generate.truncated');
      }
    } catch (e) {
      fail(e);
    } finally {
      job = null;
      busy = false;
    }
  }

  /** Field columns of the drafts sheet (the first draft's keys: every draft has the same). */
  const draftKeys = $derived(drafts[0] ? Object.keys(drafts[0].draft.fields) : []);
  const canPreview = $derived(!busy && !!source.trim());

  function onSourceKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && canPreview) {
      e.preventDefault();
      void runPreview();
    }
  }

  async function add() {
    const deck = ctx.selectedDeck;
    if (!deck) return;
    const notes = drafts.filter((d) => d.keep).map((d) => $state.snapshot(d.draft));
    const ok = await ctx.guarded(async () => {
      const r = await api.call('ai.notes.add', {
        deck_id: deck,
        notetype_id: notetypeId,
        notes,
        attributes,
        tags: [],
      });
      notice = t('ai.generate.added', { count: r.note_ids.length, skipped: r.skipped });
    });
    if (ok) drafts = [];
  }
</script>

<section class="generate">
  {#if !ctx.selectedDeck}
    <EmptyState icon={Library} title={t('ai.generate.pick_deck')} />
  {:else}
    <div class="compose">
      <div class="source">
        <div class="source-head">
          <SegmentedControl label={t('ai.source.from')} options={sourceOptions} bind:value={from} size="sm" />
          {#if sourceInfo}<p class="muted" role="status">{sourceInfo}</p>{/if}
        </div>
        {#if from === 'pdf'}
          <div class="row">
            <Button size="sm" icon={FileText} disabled={busy} onclick={() => void choosePdf()}>{t('ai.source.choose_pdf')}</Button>
            {#if pageCount !== null}
              <Input
                class="page-no"
                size="sm"
                type="number"
                min="1"
                max={pageCount}
                label={t('ai.source.from_page')}
                value={String(fromPage)}
                oninput={(e) => (fromPage = num(e.currentTarget.value, fromPage))}
              />
              <Input
                class="page-no"
                size="sm"
                type="number"
                min="1"
                max={pageCount}
                label={t('ai.source.to_page')}
                value={String(toPage)}
                oninput={(e) => (toPage = num(e.currentTarget.value, toPage))}
              />
              <Button size="sm" disabled={busy || fromPage < 1 || toPage < fromPage} onclick={() => void readPages()}>
                {t('ai.source.read_pages')}
              </Button>
            {/if}
          </div>
        {:else if from === 'url'}
          <div class="row">
            <Input
              class="grow"
              size="sm"
              type="url"
              bind:value={url}
              label={t('ai.source.url_label')}
              hideLabel
              placeholder={t('ai.source.url_placeholder')}
              hint={t('ai.source.url_note')}
              onkeydown={(e) => e.key === 'Enter' && void readUrl()}
            />
            <Button size="sm" disabled={busy || !url.trim()} onclick={() => void readUrl()}>{t('ai.source.fetch')}</Button>
          </div>
        {/if}
        <Textarea
          class="source-text"
          label={t('ai.generate.source')}
          rows={12}
          bind:value={source}
          placeholder={t('ai.generate.source_hint')}
          onkeydown={onSourceKey}
        />
      </div>

      <aside class="options" aria-label={t('ai.tabs.generate')}>
        <Select size="sm" label={t('ai.generate.notetype')} options={notetypeOptions} bind:value={notetypeId} />
        {#each defs as a (a.def.id)}
          <AttributeField size="sm" def={a.def} bind:value={attrs[a.def.id]!} />
        {/each}
        <Input
          size="sm"
          type="number"
          min="1"
          max="50"
          label={t('ai.generate.count')}
          value={String(count)}
          oninput={(e) => (count = num(e.currentTarget.value, count))}
        />
        <Input size="sm" label={t('ai.generate.instructions')} bind:value={instructions} placeholder={t('ai.generate.instructions_hint')} />
        <Button variant="primary" icon={Sparkles} kbd="Mod+Enter" fullWidth disabled={!canPreview} onclick={() => void runPreview()}>
          {t('ai.generate.preview')}
        </Button>
      </aside>
    </div>
  {/if}

  {#if error}
    <p class="banner error" role="alert">{error}</p>
  {/if}
  {#if notice}
    <p class="banner" role="status">{notice}</p>
  {/if}

  {#if preview}
    <PreviewPanel {preview} {busy} onsend={() => void send()} oncancel={() => (preview = null)} />
  {/if}
  {#if job}
    <JobProgress {job} />
  {/if}

  {#if drafts.length > 0}
    <div class="drafts-head">
      <h3>{t('ai.generate.drafts', { count: drafts.length })}</h3>
      {#if spent}<span class="muted num">{t('ai.generate.spent', { cost: spent })}</span>{/if}
    </div>
    <div class="sheet" style:--cols={Math.max(draftKeys.length, 1)}>
      <div class="sheet-head" aria-hidden="true">
        <span></span>
        {#each draftKeys as key (key)}<span class="col">{fieldName(key)}</span>{/each}
      </div>
      <ol class="drafts">
        {#each drafts as d, i (i)}
          <li class="draft" class:off={!d.keep}>
            <span class="keep"><Checkbox ariaLabel={`${t('ai.generate.keep')} ${i + 1}`} bind:checked={d.keep} /></span>
            {#each Object.keys(d.draft.fields) as key (key)}
              <div class="cell">
                <Textarea label={`${fieldName(key)} ${i + 1}`} hideLabel rows={1} autoGrow maxRows={8} bind:value={d.draft.fields[key]!} />
              </div>
            {/each}
          </li>
        {/each}
      </ol>
    </div>
    <div class="actions">
      <Button variant="primary" disabled={kept === 0} onclick={() => void add()}>
        {t('ai.generate.add', { count: kept })}
      </Button>
    </div>
  {/if}
</section>

<style>
  .generate {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  /* Source on the left (what you paste), the few options and the action on the right. */
  .compose {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 232px;
    gap: var(--space-4);
    align-items: start;
  }
  .source {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .source-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .source :global(.source-text textarea) {
    min-height: 240px;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .row :global(.grow) {
    flex: 1;
    min-width: 220px;
  }
  .row :global(.page-no) {
    width: 96px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-3);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .banner {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .banner.error {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .drafts-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
  }
  .drafts-head h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  /* Drafts as a sheet: keep | field | field, like the Create editor. */
  .sheet {
    --grid: 40px repeat(var(--cols), minmax(0, 1fr));
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    overflow: hidden;
  }
  .sheet-head,
  .draft {
    display: grid;
    grid-template-columns: var(--grid);
  }
  .sheet-head {
    min-height: 30px;
    align-items: center;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .sheet-head .col {
    padding: 0 var(--space-3);
    border-left: 1px solid var(--border);
  }
  .drafts {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .draft + .draft {
    border-top: 1px solid var(--border);
  }
  .draft.off .cell {
    opacity: 0.55;
  }
  .keep {
    display: flex;
    justify-content: center;
    padding-top: 9px;
  }
  .cell {
    border-left: 1px solid var(--border);
    min-width: 0;
  }
  .cell :global(textarea) {
    display: block;
    min-height: 36px;
    padding: 8px var(--space-3);
    border: 0;
    border-radius: 0;
    background: transparent;
    resize: none;
  }
  .cell :global(textarea:focus-visible) {
    background: var(--surface-0);
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  @media (max-width: 760px) {
    .compose {
      grid-template-columns: minmax(0, 1fr);
    }
    .options {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
      align-items: end;
    }
  }
</style>
