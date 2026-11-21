<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ReadingItemDto, TextAnalysis, WordToken } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { pct100, tzOffset } from '../stats/format';
  import WordPanel from './WordPanel.svelte';
  import { Button, Check, ChevronLeft, Chip, Input, Skeleton } from '$lib/ui';
  // v0.10 modes: "question :: answer" lines while reading (one undo step; module `inline_cards`).
  import InlineComposer from '../modes/InlineComposer.svelte';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import { clock, lines, segments, selectedRange, sentenceAround, STATUSES } from './text';

  /** Reads one item (or a pasted text): colours come from `reading.analyze`. */
  let {
    ctx,
    item,
    onclose,
  }: { ctx: ScreenCtx; item: ReadingItemDto; onclose: () => void } = $props();

  let analysis = $state<TextAnalysis | null>(null);
  let picked = $state<WordToken | null>(null);
  let selection = $state<[number, number] | null>(null);
  let notice = $state<string | null>(null);
  let postponeDays = $state(3);
  let composing = $state(false);

  async function load() {
    await ctx.guarded(async () => {
      analysis = await api.call('reading.analyze', {
        item_id: item.id,
        text: null,
        language: null,
        range: null,
        tz_offset_mins: tzOffset(),
      });
    });
  }

  onMount(() => {
    trackModules();
    void load();
    const unsub = api.on(
      ['word_marks_changed', 'study_', 'cards_changed', 'undone', 'redone', 'collection_replaced', 'data_changed'],
      () => void load(),
    );
    return () => void unsub.then((u) => u());
  });

  const isSubtitle = $derived(item.kind === 'subtitle');
  const rows = $derived(analysis ? (isSubtitle ? lines(analysis.text) : [{ start: 0, end: analysis.text.length }]) : []);
  const cueIndex = $derived(picked && isSubtitle ? rows.findIndex((r) => picked!.start >= r.start && picked!.start <= r.end) : null);
  const sentence = $derived.by(() => {
    if (!analysis || !picked) return null;
    if (isSubtitle && cueIndex !== null && cueIndex >= 0) return [rows[cueIndex]!.start, rows[cueIndex]!.end] as [number, number];
    return sentenceAround(analysis, picked.start);
  });

  function newOnPage(): string[] {
    return [...new Set((analysis?.tokens ?? []).filter((x) => x.status === 'new').map((x) => x.lemma))];
  }

  async function markAllKnown() {
    const lemmas = newOnPage();
    if (lemmas.length === 0 || !analysis) return;
    const language = analysis.language;
    if (await ctx.guarded(() => api.call('reading.words.mark', { language, lemmas, mark: 'known' }))) await load();
  }

  async function extract() {
    const r = selection;
    if (!r) return;
    const ok = await ctx.guarded(() =>
      api.call('reading.extracts.create', { item_id: item.id, start: r[0], end: r[1], title: '' }),
    );
    if (ok) {
      notice = t('reading.extract.made');
      selection = null;
    }
  }

  async function cloze() {
    const r = selection;
    if (!r || !ctx.selectedDeck) return;
    const deck_id = ctx.selectedDeck;
    const around = analysis ? sentenceAround(analysis, r[0]) : null;
    const ok = await ctx.guarded(() =>
      api.call('reading.extracts.cloze', {
        item_id: item.id,
        range: around,
        hide: [r],
        together: false,
        deck_id,
        tags: [],
      }),
    );
    if (ok) {
      notice = t('reading.cloze.made');
      selection = null;
    }
  }

  async function done() {
    const position = analysis?.text.length ?? null;
    if (await ctx.guarded(() => api.call('reading.queue.read', { id: item.id, position }))) onclose();
  }

  async function postpone() {
    const days = Number(postponeDays);
    if (await ctx.guarded(() => api.call('reading.queue.postpone', { id: item.id, days }))) onclose();
  }
</script>
<section class="reader">
  <header class="head">
    <div class="title-row">
      <Button variant="ghost" size="sm" icon={ChevronLeft} onclick={onclose} class="back">{t('reading.back')}</Button>
      <h1>{item.title}</h1>
      {#if analysis}
        <span class="known" role="status">{t('reading.known_pct', { pct: pct100(analysis.summary.known_pct) })}</span>
      {/if}
    </div>
    {#if analysis}
      <div class="legend">
        {#each STATUSES as s (s)}
          <span class="key"><span class="swatch {s}" aria-hidden="true"></span>{t(`reading.status.${s}`)}: {analysis.summary.unique[s]}</span>
        {/each}
        <span class="grow"></span>
        <Button size="sm" variant="ghost" icon={Check} onclick={markAllKnown} disabled={newOnPage().length === 0}>{t('reading.mark_all_known')}</Button>
      </div>
    {/if}
  </header>

  {#if !analysis}
    <Skeleton lines={8} />
  {:else}
    <div class="body" class:with-panel={!!picked}>
      <div class="col">
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <article class="text" class:subs={isSubtitle} lang={analysis.language} onmouseup={() => (selection = selectedRange())}>
          {#each rows as row, i (row.start)}
            <p>
              {#if isSubtitle}<span class="cue num">{clock(analysis.cues[i]?.start_ms ?? 0)}</span>{/if}
              {#each segments(analysis.text, analysis.tokens, row.start, row.end) as seg (seg.start)}
                {#if seg.token}
                  {@const tok = seg.token}
                  <button class="w {tok.status}" class:picked={picked === tok} data-start={seg.start}
                    onclick={() => (picked = tok)}>{seg.text}</button>
                {:else}<span data-start={seg.start}>{seg.text}</span>{/if}
              {/each}
            </p>
          {/each}
        </article>

        {#if selection}
          <div class="selection" role="toolbar" aria-label={t('reading.selection')}>
            <span class="sel-label">{t('reading.selection')}</span>
            <Button size="sm" onclick={extract}>{t('reading.extract.make')}</Button>
            <Button size="sm" onclick={cloze} disabled={!ctx.selectedDeck}>{t('reading.cloze.make')}</Button>
          </div>
        {/if}
        {#if notice}<p class="notice" role="status">{notice}</p>{/if}

        <footer class="foot">
          <Button variant="primary" icon={Check} onclick={done}>{t('reading.queue.done')}</Button>
          <div class="postpone">
            <Input size="sm" type="number" min="1" max="365" label={t('reading.queue.postpone_days')} hideLabel bind:value={postponeDays} class="days" />
            <span class="unit">{t('reading.queue.postpone_days')}</span>
            <Button size="sm" onclick={postpone}>{t('reading.queue.postpone')}</Button>
          </div>
          <span class="grow"></span>
          {#if moduleKnownOn('inline_cards')}
            <Button size="sm" variant="ghost" onclick={() => (composing = true)} disabled={!ctx.selectedDeck}>{t('modes.inline.open')}</Button>
          {/if}
        </footer>
        {#if composing && ctx.selectedDeck}
          <InlineComposer deckId={ctx.selectedDeck} guarded={ctx.guarded} onclose={() => (composing = false)} />
        {/if}
      </div>
      {#if picked}
        {@const s = sentence}
        <WordPanel
          {ctx}
          {item}
          language={analysis.language}
          lemma={picked.lemma}
          surface={analysis.text.slice(picked.start, picked.end)}
          sentence={s ? analysis.text.slice(s[0], s[1]) : ''}
          wordRange={s ? [picked.start - s[0], picked.end - s[0]] : null}
          cueIndex={cueIndex !== null && cueIndex >= 0 ? cueIndex : null}
          onchange={load}
          onclose={() => (picked = null)}
        />
      {/if}
    </div>
  {/if}
</section>

<style>
  .reader {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  .title-row :global(.back) {
    margin-inline-start: calc(-1 * var(--space-2));
  }
  h1 {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    overflow-wrap: anywhere;
  }
  .known {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--accent-text);
    white-space: nowrap;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-4);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    border: 1px solid var(--border-strong);
  }
  .grow {
    flex: 1;
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-8);
    align-items: start;
  }
  .body.with-panel {
    grid-template-columns: minmax(0, 1fr) 300px;
  }
  .col {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  /* The one place for large type: a comfortable reading column. */
  .text {
    max-width: 68ch;
    font-size: 18px;
    line-height: 1.75;
    text-align: start;
    color: var(--text-1);
  }
  .text p {
    margin: 0 0 1em;
  }
  .text p:last-child {
    margin-bottom: 0;
  }
  .text.subs p {
    margin-bottom: var(--space-2);
  }
  .cue {
    display: inline-block;
    min-width: 5ch;
    margin-right: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  button.w {
    all: unset;
    cursor: pointer;
    border-radius: 3px;
    padding: 0 1px;
  }
  .w.new,
  .swatch.new {
    background: var(--accent-soft);
  }
  .w.learning,
  .swatch.learning {
    background: var(--warning-soft);
  }
  .swatch.known {
    background: transparent;
  }
  .w.ignored {
    color: var(--text-2);
  }
  .swatch.ignored {
    background: var(--surface-2);
  }
  button.w:hover {
    box-shadow: inset 0 -1px 0 var(--text-2);
  }
  .w.picked {
    outline: 2px solid var(--accent-text);
  }
  button.w:focus-visible {
    outline: 2px solid var(--focus-ring);
  }
  .selection {
    position: sticky;
    bottom: var(--space-3);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    align-self: flex-start;
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-2);
    box-shadow: var(--shadow-float);
  }
  .sel-label {
    font-size: var(--text-xs);
    color: var(--text-2);
    margin-right: var(--space-1);
  }
  .foot,
  .postpone {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }
  .foot {
    max-width: 68ch;
    gap: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }
  .postpone :global(.days) {
    width: 64px;
  }
  .unit {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .notice {
    margin: 0;
    color: var(--success);
    font-size: var(--text-sm);
  }
  @media (max-width: 1000px) {
    .body.with-panel {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
