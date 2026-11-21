<script lang="ts">
  import { formatPercent } from '$lib/format';
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
  import {
    Button,
    Checkbox,
    CircleAlert,
    CircleCheck,
    ClipboardCheck,
    Eye,
    FileText,
    FolderOpen,
    Icon,
    Input,
    RotateCcw,
    SegmentedControl,
    Select,
    Textarea,
    Upload,
  } from '$lib/ui';
  import { FormRow, SettingsSection } from '../core/kit';

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

  const sourceKinds = $derived([
    { value: 'text' as const, label: t('io.import.source_text'), icon: ClipboardCheck },
    { value: 'file' as const, label: t('io.import.source_file'), icon: FileText },
  ]);
  const formatOptions = $derived([
    { value: 'auto' as ImportFormat | 'auto', label: t('io.import.format_auto') },
    ...FORMATS.map((f) => ({ value: f as ImportFormat | 'auto', label: t(`io.formats.${f}`) })),
  ]);
  const recordOptions = $derived([
    { value: '\n', label: t('io.import.newline') },
    { value: '\n\n', label: t('io.import.blank_line') },
    { value: ';', label: ';' },
  ]);
  const duplicateOptions = $derived(DUPLICATES.map((d) => ({ value: d, label: t(`io.duplicate.${d}`) })));
  const columnOptions = $derived([
    { value: 'ignore', label: t('io.import.ignore') },
    ...fields.map((f) => ({ value: `field:${f.key}`, label: f.name })),
    { value: 'tags', label: t('io.import.tags') },
    ...attributes.map((a) => ({ value: `attr:${a.def.id}`, label: a.def.name })),
  ]);

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

<SettingsSection title={t('io.import.source')} description={t('io.import.source_hint')} icon={Upload} body="rows">
  {#snippet actions()}
    <SegmentedControl label={t('io.import.source_kind')} options={sourceKinds} bind:value={sourceKind} />
  {/snippet}
  <div class="source">
    {#if sourceKind === 'text'}
      <Textarea
        label={t('io.import.source_text')}
        hideLabel
        rows={6}
        autoGrow
        placeholder={t('io.import.paste_placeholder')}
        bind:value={text}
      />
    {:else}
      <div class="file">
        <Button icon={FolderOpen} onclick={chooseFile}>{t('io.choose_file')}</Button>
        <span class="path" class:empty={!path}>{path ?? t('io.no_file')}</span>
      </div>
    {/if}
  </div>
  <FormRow label={t('io.import.format')} labelFor="fc-import-format">
    <Select id="fc-import-format" hideLabel label={t('io.import.format')} options={formatOptions} bind:value={format} onchange={() => void runPreview(false)} />
  </FormRow>
  <div class="foot">
    <Button variant={preview ? 'secondary' : 'primary'} icon={Eye} onclick={() => void runPreview(false)} disabled={!source}>{t('io.import.preview')}</Button>
  </div>
</SettingsSection>

{#if error}<p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}
{#if notice}<p class="note" role="status">{notice}</p>{/if}

{#if preview && !report}
  <SettingsSection title={t('io.import.preview_heading')} description={t('io.import.rows', { shown: preview.rows.length, total: preview.total_rows })} icon={Eye} body="rows">
    {#if preview.package}
      <FormRow label={t('io.import.package_label')} description={t('io.import.package', {
        notes: preview.package.notes,
        cards: preview.package.cards,
        media: preview.package.media,
        decks: preview.package.decks.length,
      })} />
      <FormRow label={t('io.import.include_history')}>
        <Checkbox ariaLabel={t('io.import.include_history')} bind:checked={includeHistory} />
      </FormRow>
    {:else}
      {#if options}
        <FormRow
          stack
          label={t('io.import.separators')}
          description={preview.confidence !== null ? t('io.import.detected', { confidence: formatPercent(preview.confidence) }) : undefined}
        >
          <div class="seps">
            <Input
              label={t('io.import.field_separator')}
              size="sm"
              value={escapeSeparator(options.field_separator)}
              onchange={(e: Event) => setSeparator('field_separator', (e.currentTarget as HTMLInputElement).value)}
            />
            <Select
              label={t('io.import.record_separator')}
              size="sm"
              options={recordOptions}
              value={options.record_separator}
              onchange={(v) => setSeparator('record_separator', v)}
            />
          </div>
        </FormRow>
      {/if}
      <div class="table-wrap">
        <table>
          {#if preview.format !== 'json'}
            <thead>
              <tr>
                {#each columns as column, i (i)}
                  <th scope="col">
                    <Select
                      size="sm"
                      hideLabel
                      label={t('io.import.column', { n: i + 1 })}
                      options={columnOptions}
                      value={encodeColumn(column)}
                      onchange={(v) => (columns[i] = decodeColumn(v))}
                    />
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
        <FormRow label={t('io.import.skip_first_row')}>
          <Checkbox ariaLabel={t('io.import.skip_first_row')} bind:checked={skipFirst} />
        </FormRow>
      {/if}
      {#if attributesOn && (hasAttributeColumn || preview.format === 'json')}
        <FormRow label={t('io.import.create_missing_options')}>
          <Checkbox ariaLabel={t('io.import.create_missing_options')} bind:checked={createMissing} />
        </FormRow>
      {/if}
    {/if}
    <FormRow label={t('io.import.duplicates')} labelFor="fc-import-dupes">
      <Select id="fc-import-dupes" hideLabel label={t('io.import.duplicates')} options={duplicateOptions} bind:value={duplicates} />
    </FormRow>
    <div class="foot">
      {#if job}
        <JobProgress {job} />
      {:else}
        <Button variant="primary" icon={Upload} disabled={!canStart} onclick={start}>
          {isPackage || !needsDeck
            ? t('io.import.start_package')
            : t('io.import.start', { deck: ctx.selectedPath ?? '' })}
        </Button>
        {#if needsDeck && !ctx.selectedDeck}<span class="hint">{t('io.pick_deck')}</span>{/if}
      {/if}
    </div>
  </SettingsSection>
{/if}

{#if report}
  <SettingsSection title={t('io.import.done_heading')} icon={CircleCheck}>
    <div class="report">
      <p class="note ok" role="status">
        <Icon icon={CircleCheck} size={16} />
        <span>
          {t('io.import.done', {
            added: report.added_notes,
            updated: report.updated_notes,
            skipped: report.skipped_duplicates,
          })}
        </span>
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
      <div><Button icon={RotateCcw} onclick={undo}>{t('io.import.undo')}</Button></div>
    </div>
  </SettingsSection>
{/if}

<style>
  .source {
    padding: var(--space-2) 0 var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    min-width: 0;
  }
  .path {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--text-1);
    overflow-wrap: anywhere;
  }
  .path.empty {
    font-family: inherit;
    color: var(--text-2);
  }
  .seps {
    flex: 1 1 100%;
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
    align-items: flex-end;
  }
  :global(.fc-form-row) .seps :global(.fc-field) {
    flex: 0 1 14rem;
    min-width: 9rem;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: var(--space-3) 0 0;
    /* The rows above end with their own border. */
  }
  .hint {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .table-wrap {
    margin: var(--space-2) 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: auto;
    max-height: 420px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: var(--space-2);
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    text-align: start;
    font-weight: var(--weight-medium);
    min-width: 10rem;
  }
  th :global(.fc-field) {
    min-width: 0;
  }
  td {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    white-space: pre-wrap;
    color: var(--text-1);
    vertical-align: top;
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  tr.header td {
    color: var(--text-2);
    font-style: italic;
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .note :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .ok {
    color: var(--success);
  }
  .bad {
    color: var(--danger);
  }
  .report {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  details {
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  summary {
    cursor: pointer;
    color: var(--text-2);
  }
  details ul {
    margin: var(--space-2) 0 0;
    padding-inline-start: var(--space-6);
  }
</style>
