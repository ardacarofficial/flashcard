<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ReadingItemDto, TextAnalysis, WordToken } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { pct100, tzOffset } from '../stats/format';
  import WordPanel from './WordPanel.svelte';
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
  <header class="row">
    <button onclick={onclose}>{t('reading.back')}</button>
    <h2>{item.title}</h2>
    {#if analysis}
      <strong role="status">{t('reading.known_pct', { pct: pct100(analysis.summary.known_pct) })}</strong>
    {/if}
  </header>

  {#if analysis}
    <p class="legend">
      {#each STATUSES as s (s)}
        <span class="w {s}">{t(`reading.status.${s}`)}: {analysis.summary.unique[s]}</span>
      {/each}
      <button onclick={markAllKnown} disabled={newOnPage().length === 0}>{t('reading.mark_all_known')}</button>
    </p>

    <div class="body">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="text" lang={analysis.language} onmouseup={() => (selection = selectedRange())}>
        {#each rows as row, i (row.start)}
          <p>
            {#if isSubtitle}<span class="muted cue">{clock(analysis.cues[i]?.start_ms ?? 0)}</span>{/if}
            {#each segments(analysis.text, analysis.tokens, row.start, row.end) as seg (seg.start)}
              {#if seg.token}
                {@const tok = seg.token}
                <button class="w {tok.status}" class:picked={picked === tok} data-start={seg.start}
                  onclick={() => (picked = tok)}>{seg.text}</button>
              {:else}<span data-start={seg.start}>{seg.text}</span>{/if}
            {/each}
          </p>
        {/each}
      </div>
      {#if picked && analysis}
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
        />
      {/if}
    </div>

    {#if selection}
      <p class="row">
        <button onclick={extract}>{t('reading.extract.make')}</button>
        <button onclick={cloze} disabled={!ctx.selectedDeck}>{t('reading.cloze.make')}</button>
      </p>
    {/if}
    {#if notice}<p role="status">{notice}</p>{/if}

    <footer class="row">
      <button onclick={done}>{t('reading.queue.done')}</button>
      <label>{t('reading.queue.postpone_days')}
        <input type="number" min="1" max="365" bind:value={postponeDays} /></label>
      <button onclick={postpone}>{t('reading.queue.postpone')}</button>
      {#if moduleKnownOn('inline_cards')}
        <button onclick={() => (composing = true)} disabled={!ctx.selectedDeck}>{t('modes.inline.open')}</button>
      {/if}
    </footer>
    {#if composing && ctx.selectedDeck}
      <InlineComposer deckId={ctx.selectedDeck} guarded={ctx.guarded} onclose={() => (composing = false)} />
    {/if}
  {/if}
</section>

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
    align-items: center;
  }
  .body {
    display: flex;
    gap: var(--fc-space-4);
    align-items: flex-start;
  }
  .text {
    flex: 1;
    line-height: 1.8;
    max-width: 48rem;
  }
  .cue {
    margin-right: var(--fc-space-2);
    font-variant-numeric: tabular-nums;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
  }
  button.w {
    all: unset;
    cursor: pointer;
    border-radius: 3px;
  }
  .w.new {
    background: var(--fc-word-new, #cfe3ff);
  }
  .w.learning {
    background: var(--fc-word-learning, #fff1b3);
  }
  .w.ignored {
    color: var(--fc-muted, #888);
  }
  .w.picked {
    outline: 2px solid var(--fc-accent, #4b6bfb);
  }
  button.w:focus-visible {
    outline: 2px solid var(--fc-accent, #4b6bfb);
  }
</style>
