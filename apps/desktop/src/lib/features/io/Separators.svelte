<!--
  Separators of a text import (v1.3): one line saying what was detected ("Tab · new line,
  93 % sure") and, on "Change", the named choices (tab, comma, dash, spaces, custom) for
  between fields and between cards. Detection itself is Rust's (`io.import.preview`).
-->
<script lang="ts">
  import { formatPercent } from '$lib/format';
  import type { TextOptionsDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Input, SegmentedControl } from '$lib/ui';
  import { escapeSeparator, unescapeSeparator } from './mapping';

  interface Props {
    options: TextOptionsDto;
    confidence: number | null;
    onchange: (options: TextOptionsDto) => void;
  }
  let { options, confidence, onchange }: Props = $props();

  /** Named field separators; `spaces` is fc-interop's ALIGNED (two or more spaces). */
  const FIELD_SEPS: Record<string, string> = { tab: '\t', comma: ',', semicolon: ';', dash: ' - ', spaces: '  ' };
  const RECORD_SEPS: Record<string, string> = { line: '\n', blank: '\n\n' };
  const keyOf = (map: Record<string, string>, v: string) => Object.keys(map).find((k) => map[k] === v) ?? 'custom';

  let open = $state(false);
  let fieldPick = $state<string | null>(null);
  let recordPick = $state<string | null>(null);
  const fieldMode = $derived(fieldPick ?? keyOf(FIELD_SEPS, options.field_separator));
  const recordMode = $derived(recordPick ?? keyOf(RECORD_SEPS, options.record_separator));
  let customField = $state('');
  let customRecord = $state('');

  $effect(() => {
    customField = escapeSeparator(options.field_separator);
    customRecord = escapeSeparator(options.record_separator).replaceAll('\n', '\\n');
  });

  const fieldChoices = $derived([
    { value: 'tab', label: t('io.sep.tab') },
    { value: 'comma', label: t('io.sep.comma') },
    { value: 'semicolon', label: t('io.sep.semicolon') },
    { value: 'dash', label: t('io.sep.dash') },
    { value: 'spaces', label: t('io.sep.spaces') },
    { value: 'custom', label: t('io.sep.custom') },
  ]);
  const recordChoices = $derived([
    { value: 'line', label: t('io.sep.line') },
    { value: 'blank', label: t('io.sep.blank') },
    { value: 'custom', label: t('io.sep.custom') },
  ]);

  const name = (map: Record<string, string>, v: string) => {
    const k = keyOf(map, v);
    return k === 'custom' ? `“${escapeSeparator(v).replaceAll('\n', '\\n')}”` : t(`io.sep.${k}` as 'io.sep.tab');
  };
  const summary = $derived(
    t('io.sep.summary', { field: name(FIELD_SEPS, options.field_separator), record: name(RECORD_SEPS, options.record_separator) }),
  );

  function set(which: 'field_separator' | 'record_separator', value: string) {
    if (value === '') return;
    onchange({ ...options, [which]: value });
  }
  function chooseField(mode: string) {
    fieldPick = mode;
    const v = FIELD_SEPS[mode];
    if (v !== undefined) set('field_separator', v);
  }
  function chooseRecord(mode: string) {
    recordPick = mode;
    const v = RECORD_SEPS[mode];
    if (v !== undefined) set('record_separator', v);
  }
</script>

<div class="seps">
  <p class="line">
    <span>{summary}</span>
    {#if confidence !== null}<span class="muted">· {t('io.sep.detected', { pct: formatPercent(confidence) })}</span>{/if}
    <Button size="sm" variant="ghost" aria-expanded={open} onclick={() => (open = !open)}>{open ? t('io.sep.done') : t('io.sep.change')}</Button>
  </p>
  {#if open}
    <div class="choices">
      <div class="sep">
        <span class="lbl" aria-hidden="true">{t('io.sep.between_fields')}</span>
        <SegmentedControl size="sm" label={t('io.sep.between_fields')} options={fieldChoices} value={fieldMode} onchange={chooseField} />
        {#if fieldMode === 'custom'}
          <Input size="sm" label={t('io.sep.custom_field')} hideLabel bind:value={customField} onchange={() => set('field_separator', unescapeSeparator(customField))} />
        {/if}
      </div>
      <div class="sep">
        <span class="lbl" aria-hidden="true">{t('io.sep.between_cards')}</span>
        <SegmentedControl size="sm" label={t('io.sep.between_cards')} options={recordChoices} value={recordMode} onchange={chooseRecord} />
        {#if recordMode === 'custom'}
          <Input
            size="sm"
            label={t('io.sep.custom_record')}
            hideLabel
            bind:value={customRecord}
            onchange={() => set('record_separator', unescapeSeparator(customRecord).replaceAll('\\n', '\n'))}
          />
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .seps {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .muted {
    color: var(--text-2);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
  }
  .sep {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
</style>
