<!--
  "Paste to import" on the Create screen: pasted text is split by the v0.5 import preview
  (`io.import.preview`: separator detection in Rust), shown as term/definition pairs, and
  added to the editor's rows. Separators can be overridden; nothing is saved here.
-->
<script lang="ts">
  import { formatPercent } from '$lib/format';
  import { api, type ApiError } from '$lib/api';
  import type { ImportPreview, NoteTypeId, TextOptionsDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Input, SegmentedControl, Skeleton, Textarea } from '$lib/ui';
  import { escapeSeparator, unescapeSeparator } from '../io/mapping';
  import { rowsFromPreview, type Row } from './create';

  interface Props {
    open: boolean;
    notetypeId: NoteTypeId;
    deckId: string | null;
    /** Field keys in column order (their display names label the preview). */
    columns: { key: string; label: string }[];
    onadd: (rows: Row[]) => void;
  }
  let { open = $bindable(), notetypeId, deckId, columns, onadd }: Props = $props();

  /** Rows the preview returns at most (`io.import.preview` limit). */
  const LIMIT = 200;
  /** Named field separators; `spaces` is fc-interop's ALIGNED (two or more spaces). */
  const FIELD_SEPS: Record<string, string> = { tab: '\t', comma: ',', dash: ' - ', spaces: '  ' };
  const RECORD_SEPS: Record<string, string> = { line: '\n', blank: '\n\n' };

  let text = $state('');
  let options = $state<TextOptionsDto | null>(null);
  let preview = $state<ImportPreview | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const keyOf = (map: Record<string, string>, v: string | undefined) =>
    v === undefined ? 'custom' : (Object.keys(map).find((k) => map[k] === v) ?? 'custom');
  /** The user's pick (until the next fresh detection); else what the options say. */
  let fieldPick = $state<string | null>(null);
  let recordPick = $state<string | null>(null);
  const fieldMode = $derived(fieldPick ?? keyOf(FIELD_SEPS, options?.field_separator));
  const recordMode = $derived(recordPick ?? keyOf(RECORD_SEPS, options?.record_separator));
  let customField = $state('');
  let customRecord = $state('');

  const fieldChoices = $derived([
    { value: 'tab', label: t('create.paste.sep_tab') },
    { value: 'comma', label: t('create.paste.sep_comma') },
    { value: 'dash', label: t('create.paste.sep_dash') },
    { value: 'spaces', label: t('create.paste.sep_spaces') },
    { value: 'custom', label: t('create.paste.sep_custom') },
  ]);
  const recordChoices = $derived([
    { value: 'line', label: t('create.paste.rec_line') },
    { value: 'blank', label: t('create.paste.rec_blank') },
    { value: 'custom', label: t('create.paste.sep_custom') },
  ]);

  async function run(keep: boolean) {
    error = null;
    if (!text.trim()) {
      preview = null;
      options = null;
      return;
    }
    loading = true;
    if (!keep) fieldPick = recordPick = null;
    try {
      const p = await api.call('io.import.preview', {
        source: { kind: 'text', text },
        format: 'text',
        text_options: keep ? options : null,
        notetype_id: notetypeId,
        deck_id: deckId,
        limit: LIMIT,
      });
      preview = p;
      options = p.text_options;
      if (options) {
        customField = escapeSeparator(options.field_separator);
        customRecord = escapeSeparator(options.record_separator).replaceAll('\n', '\\n');
      }
    } catch (e) {
      preview = null;
      error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    } finally {
      loading = false;
    }
  }

  // Re-detect while typing/pasting (debounced).
  let timer: ReturnType<typeof setTimeout> | undefined;
  function oninput() {
    clearTimeout(timer);
    timer = setTimeout(() => void run(false), 300);
  }

  function setOption(which: 'field_separator' | 'record_separator', value: string) {
    if (!options || value === '') return;
    options = { ...options, [which]: value };
    void run(true);
  }

  function chooseField(mode: string) {
    fieldPick = mode;
    const v = FIELD_SEPS[mode];
    if (v !== undefined) setOption('field_separator', v);
  }
  function chooseRecord(mode: string) {
    recordPick = mode;
    const v = RECORD_SEPS[mode];
    if (v !== undefined) setOption('record_separator', v);
  }

  function add() {
    if (!preview) return;
    onadd(rowsFromPreview(preview));
    text = '';
    preview = null;
    options = null;
    open = false;
  }

  const sample = $derived(preview?.rows.slice(0, 6) ?? []);
  const fieldIndex = $derived(
    columns.map((c) => preview?.suggested_columns.findIndex((s) => s.kind === 'field' && s.key === c.key) ?? -1),
  );
</script>

<Dialog bind:open title={t('create.paste.title')} description={t('create.paste.description')} size="lg">
  <div class="paste">
    <Textarea
      label={t('create.paste.text')}
      bind:value={text}
      {oninput}
      rows={7}
      placeholder={t('create.paste.placeholder')}
      spellcheck="false"
    />

    {#if options}
      <div class="seps">
        <div class="sep">
          <span class="sep-label" aria-hidden="true">{t('create.paste.between_fields')}</span>
          <SegmentedControl size="sm" label={t('create.paste.between_fields')} options={fieldChoices} value={fieldMode} onchange={chooseField} />
          {#if fieldMode === 'custom'}
            <Input
              size="sm"
              label={t('create.paste.custom_field')}
              hideLabel
              bind:value={customField}
              onchange={() => setOption('field_separator', unescapeSeparator(customField))}
            />
          {/if}
        </div>
        <div class="sep">
          <span class="sep-label" aria-hidden="true">{t('create.paste.between_cards')}</span>
          <SegmentedControl size="sm" label={t('create.paste.between_cards')} options={recordChoices} value={recordMode} onchange={chooseRecord} />
          {#if recordMode === 'custom'}
            <Input
              size="sm"
              label={t('create.paste.custom_record')}
              hideLabel
              bind:value={customRecord}
              onchange={() => setOption('record_separator', unescapeSeparator(customRecord).replaceAll('\\n', '\n'))}
            />
          {/if}
        </div>
      </div>
    {/if}

    <section class="preview" aria-live="polite" aria-busy={loading}>
      <h3>{t('create.paste.preview')}</h3>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {:else if loading && !preview}
        <Skeleton lines={3} />
      {:else if !preview || preview.total_rows === 0}
        <p class="muted">{t('create.paste.nothing')}</p>
      {:else}
        <p class="muted small">
          {t('create.paste.found', { count: preview.total_rows })}
          {#if preview.confidence !== null}· {t('create.paste.confidence', { pct: formatPercent(preview.confidence) })}{/if}
        </p>
        <ol class="pairs">
          <li class="cols" aria-hidden="true">
            {#each columns as c (c.key)}<span>{c.label}</span>{/each}
          </li>
          {#each sample as cells, i (i)}
            <li>
              {#each columns as c, ci (c.key)}
                <span class="cell" class:first={ci === 0}>{cells[fieldIndex[ci] ?? -1] ?? ''}</span>
              {/each}
            </li>
          {/each}
        </ol>
        {#if preview.total_rows > sample.length}
          <p class="muted small">{t('create.paste.more', { count: preview.total_rows - sample.length })}</p>
        {/if}
        {#if preview.total_rows > LIMIT}
          <p class="warn small" role="note">{t('create.paste.too_many', { max: LIMIT })}</p>
        {/if}
      {/if}
    </section>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" disabled={!preview || preview.rows.length === 0} onclick={add}>
      {t('create.paste.add', { count: preview?.rows.length ?? 0 })}
    </Button>
  {/snippet}
</Dialog>

<style>
  .paste {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .seps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
  }
  .sep {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .preview h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .pairs {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  .pairs li {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
    gap: var(--space-4);
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    background: var(--surface-1);
  }
  .pairs li.cols {
    background: var(--surface-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
    padding-block: var(--space-1);
  }
  .sep-label {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .pairs li + li {
    border-top: 1px solid var(--border);
  }
  .cell {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .cell.first {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .muted {
    color: var(--text-2);
    margin: 0;
  }
  .small {
    font-size: var(--text-sm);
  }
  .warn {
    color: var(--warning);
    margin: var(--space-2) 0 0;
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
</style>
