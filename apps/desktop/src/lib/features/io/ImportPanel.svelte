<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type {
    ColumnTarget,
    DuplicatePolicy,
    EffectiveAttributeDto,
    ImportFormat,
    ImportIssue,
    ImportPreview,
    ImportReport,
    ImportSource,
    JobDto,
    TextOptionsDto,
  } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import type { ScreenCtx } from '../types';
  import { pickFile } from './files';
  import { followJob, resultOf } from './job';
  import { decodeColumn, encodeColumn, escapeSeparator, unescapeSeparator } from './mapping';
  import JobProgress from './JobProgress.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const FORMATS: ImportFormat[] = ['text', 'quizlet', 'json', 'anki', 'fcpack'];
  const DUPLICATES: DuplicatePolicy[] = ['skip', 'update', 'add'];

  let sourceKind = $state<'text' | 'file'>('text');
  let text = $state('');
  let path = $state<string | null>(null);
  let format = $state<ImportFormat | 'auto'>('auto');
  let options = $state<TextOptionsDto | null>(null);
  let preview = $state<ImportPreview | null>(null);
  let columns = $state<ColumnTarget[]>([]);
  let skipFirst = $state(false);
  let duplicates = $state<DuplicatePolicy>('skip');
  let createMissing = $state(false);
  let includeHistory = $state(true);
  // Text rows go to the Classic note type (the default of `io.import.start`).
  const fields = $derived([
    { key: 'front', name: t('note.front') },
    { key: 'back', name: t('note.back') },
  ]);
  let attributes = $state<EffectiveAttributeDto[]>([]);
  let job = $state<JobDto | null>(null);
  let report = $state<ImportReport | null>(null);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);

  const isPackage = $derived(preview?.package != null);
  const source = $derived<ImportSource | null>(
    sourceKind === 'file'
      ? path
        ? { kind: 'file', path }
        : null
      : text.trim()
        ? { kind: 'text', text }
        : null,
  );
  const needsDeck = $derived(!isPackage && preview?.format !== 'json');
  const canStart = $derived(!!preview && !job && (!needsDeck || !!ctx.selectedDeck));
  const hasAttributeColumn = $derived(columns.some((c) => c.kind === 'attribute'));

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  const attributesOn = $derived(moduleKnownOn('attributes'));

  // Classification columns: only while the `attributes` module is on.
  $effect(() => {
    trackModules();
    const deck = ctx.selectedDeck;
    attributes = [];
    if (deck && attributesOn) void api.attribute.effective(deck).then((r) => (attributes = r.attributes), fail);
  });

  async function runPreview(keepOptions: boolean) {
    error = null;
    report = null;
    notice = null;
    if (!source) {
      preview = null;
      return;
    }
    try {
      preview = await api.call('io.import.preview', {
        source,
        format: format === 'auto' ? null : format,
        text_options: keepOptions ? options : null,
        notetype_id: null,
        deck_id: ctx.selectedDeck,
        limit: 10,
      });
      options = preview.text_options;
      columns = [...preview.suggested_columns];
    } catch (e) {
      preview = null;
      fail(e);
    }
  }

  async function chooseFile() {
    const picked = await pickFile(
      [{ name: '', extensions: ['txt', 'csv', 'tsv', 'json', 'apkg', 'colpkg', 'fcpack'] }],
      t('io.path_prompt'),
    );
    if (picked) {
      path = picked;
      await runPreview(false);
    }
  }

  function setSeparator(which: 'field_separator' | 'record_separator', raw: string) {
    if (!options) return;
    options = { ...options, [which]: unescapeSeparator(raw) };
    void runPreview(true);
  }

  async function start() {
    if (!source || !preview) return;
    error = null;
    report = null;
    notice = null;
    try {
      const started = await api.call('io.import.start', {
        source,
        format: preview.format,
        text_options: isPackage ? null : options,
        deck_id: isPackage ? null : ctx.selectedDeck,
        notetype_id: null,
        columns: isPackage ? null : columns,
        skip_first_row: skipFirst,
        duplicates,
        create_missing_options: createMissing,
        include_history: includeHistory,
      });
      job = started;
      const done = await followJob(started, (j) => (job = j));
      job = null;
      if (done.status === 'cancelled') notice = t('io.job.cancelled');
      else if (done.error) error = td(done.error.message_key);
      else report = resultOf<ImportReport>(done);
    } catch (e) {
      job = null;
      fail(e);
    }
  }

  function issueText(issue: ImportIssue): string {
    const text = td(issue.message.key, issue.message.params);
    return issue.row > 0 ? `${t('io.import.row', { row: issue.row })}: ${text}` : text;
  }

  async function undo() {
    try {
      await api.oplog.undo();
      report = null;
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="panel">
  <fieldset>
    <legend>{t('io.import.source')}</legend>
    <label><input type="radio" bind:group={sourceKind} value="text" /> {t('io.import.source_text')}</label>
    <label><input type="radio" bind:group={sourceKind} value="file" /> {t('io.import.source_file')}</label>
    {#if sourceKind === 'text'}
      <textarea
        rows="6"
        aria-label={t('io.import.source_text')}
        placeholder={t('io.import.paste_placeholder')}
        bind:value={text}
      ></textarea>
    {:else}
      <div class="row">
        <button onclick={chooseFile}>{t('io.choose_file')}</button>
        <span class="muted">{path ?? t('io.no_file')}</span>
      </div>
    {/if}
    <label>
      {t('io.import.format')}
      <select bind:value={format} onchange={() => void runPreview(false)}>
        <option value="auto">{t('io.import.format_auto')}</option>
        {#each FORMATS as f (f)}<option value={f}>{t(`io.formats.${f}`)}</option>{/each}
      </select>
    </label>
    <button onclick={() => void runPreview(false)} disabled={!source}>{t('io.import.preview')}</button>
  </fieldset>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}<p class="muted" role="status">{notice}</p>{/if}

  {#if preview && !report}
    {#if preview.package}
      <p>
        {t('io.import.package', {
          notes: preview.package.notes,
          cards: preview.package.cards,
          media: preview.package.media,
          decks: preview.package.decks.length,
        })}
      </p>
      <label><input type="checkbox" bind:checked={includeHistory} /> {t('io.import.include_history')}</label>
    {:else}
      {#if options}
        <div class="row">
          <label>
            {t('io.import.field_separator')}
            <input
              size="4"
              value={escapeSeparator(options.field_separator)}
              onchange={(e) => setSeparator('field_separator', e.currentTarget.value)}
            />
          </label>
          <label>
            {t('io.import.record_separator')}
            <select
              value={options.record_separator}
              onchange={(e) => setSeparator('record_separator', e.currentTarget.value)}
            >
              <option value={'\n'}>{t('io.import.newline')}</option>
              <option value={'\n\n'}>{t('io.import.blank_line')}</option>
              <option value=";">;</option>
            </select>
          </label>
        </div>
        {#if preview.confidence !== null}
          <p class="muted">{t('io.import.detected', { confidence: Math.round(preview.confidence * 100) })}</p>
        {/if}
      {/if}
      <p class="muted">{t('io.import.rows', { shown: preview.rows.length, total: preview.total_rows })}</p>
      <div class="scroll">
        <table>
          {#if preview.format !== 'json'}
            <thead>
              <tr>
                {#each columns as column, i (i)}
                  <th>
                    <select
                      aria-label={t('io.import.column', { n: i + 1 })}
                      value={encodeColumn(column)}
                      onchange={(e) => (columns[i] = decodeColumn(e.currentTarget.value))}
                    >
                      <option value="ignore">{t('io.import.ignore')}</option>
                      {#each fields as f (f.key)}<option value={`field:${f.key}`}>{f.name}</option>{/each}
                      <option value="tags">{t('io.import.tags')}</option>
                      {#each attributes as a (a.def.id)}<option value={`attr:${a.def.id}`}>{a.def.name}</option>{/each}
                    </select>
                  </th>
                {/each}
              </tr>
            </thead>
          {/if}
          <tbody>
            {#each preview.rows as row, r (r)}
              <tr class:header={skipFirst && r === 0}>
                {#each row as cell, c (c)}<td>{cell}</td>{/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if preview.format !== 'json'}
        <label><input type="checkbox" bind:checked={skipFirst} /> {t('io.import.skip_first_row')}</label>
      {/if}
      {#if attributesOn && (hasAttributeColumn || preview.format === 'json')}
        <label><input type="checkbox" bind:checked={createMissing} /> {t('io.import.create_missing_options')}</label>
      {/if}
    {/if}
    <label>
      {t('io.import.duplicates')}
      <select bind:value={duplicates}>
        {#each DUPLICATES as d (d)}<option value={d}>{t(`io.duplicate.${d}`)}</option>{/each}
      </select>
    </label>
    {#if needsDeck && !ctx.selectedDeck}
      <p class="muted">{t('io.pick_deck')}</p>
    {/if}
    {#if job}
      <JobProgress {job} />
    {:else}
      <button class="primary" disabled={!canStart} onclick={start}>
        {isPackage || !needsDeck
          ? t('io.import.start_package')
          : t('io.import.start', { deck: ctx.selectedPath ?? '' })}
      </button>
    {/if}
  {/if}

  {#if report}
    <p role="status">
      {t('io.import.done', {
        added: report.added_notes,
        updated: report.updated_notes,
        skipped: report.skipped_duplicates,
      })}
    </p>
    {#if report.issues.length > 0}
      <details>
        <summary>{t('io.import.issues')} ({report.issues.length})</summary>
        <ul>
          {#each report.issues as issue, i (i)}
            <li>{issueText(issue)}</li>
          {/each}
        </ul>
      </details>
    {/if}
    <button onclick={undo}>{t('io.import.undo')}</button>
  {/if}
</div>

<style>
  .panel,
  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    align-items: flex-start;
  }
  fieldset {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    width: 100%;
  }
  textarea {
    width: 100%;
  }
  .row {
    display: flex;
    gap: var(--fc-space-3);
    align-items: center;
  }
  .scroll {
    max-width: 100%;
    overflow: auto;
  }
  td {
    padding: var(--fc-space-1) var(--fc-space-2);
    border-bottom: var(--fc-border-width) solid var(--fc-border);
    white-space: pre-wrap;
  }
  tr.header td {
    opacity: 0.6;
  }
</style>
