<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/api';
  import type { ReadingCardKind, ReadingClip, ReadingItemDto, WordInfo, WordMark } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { followJob, isFinal, resultOf } from '../io/job';
  import type { ScreenCtx } from '../types';
  import { tzOffset } from '../stats/format';

  /** The clicked word: everything here comes from the backend (lookup, card creation). */
  let {
    ctx,
    item,
    language,
    lemma,
    surface,
    sentence,
    wordRange,
    cueIndex,
    onchange,
  }: {
    ctx: ScreenCtx;
    item: ReadingItemDto | null;
    language: string;
    lemma: string;
    surface: string;
    sentence: string;
    wordRange: [number, number] | null;
    cueIndex: number | null;
    onchange: () => void;
  } = $props();

  let info = $state<WordInfo | null>(null);
  let kind = $state<ReadingCardKind>('word');
  let word = $state('');
  let meaning = $state('');
  let translation = $state('');
  let clip = $state<ReadingClip | null>(null);
  let clipping = $state(false);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    await ctx.guarded(async () => {
      info = await api.call('reading.words.lookup', { language, lemma, tz_offset_mins: tzOffset() });
    });
  }

  // A new word resets the form (only the lemma is tracked).
  $effect(() => {
    const next = lemma;
    untrack(() => {
      word = next;
      meaning = '';
      translation = '';
      clip = null;
      notice = null;
      error = null;
      kind = item?.kind === 'subtitle' ? 'sentence' : 'word';
      void load();
    });
  });

  async function setMark(mark: WordMark | null) {
    if (await ctx.guarded(() => api.call('reading.words.mark', { language, lemmas: [lemma], mark }))) {
      await load();
      onchange();
    }
  }

  async function cutClip() {
    if (!item || cueIndex === null) return;
    clipping = true;
    error = null;
    try {
      const job = await api.call('reading.clips.extract', { item_id: item.id, cue_index: cueIndex, image: true });
      const done = isFinal(job) ? job : await followJob(job, () => {});
      clip = resultOf<ReadingClip>(done);
      if (!clip) error = td(done.error?.message_key ?? 'errors.internal.unexpected');
    } catch (e) {
      error = td((e as { message_key?: string }).message_key ?? 'errors.internal.unexpected');
    } finally {
      clipping = false;
    }
  }

  async function makeCard() {
    if (!ctx.selectedDeck) return;
    const deck_id = ctx.selectedDeck;
    const ok = await ctx.guarded(() =>
      api.call('reading.cards.make', {
        kind,
        deck_id,
        word,
        meaning,
        sentence,
        word_range: wordRange,
        translation,
        item_id: item?.id ?? null,
        cue_index: cueIndex,
        audio: clip?.audio.reference ?? null,
        image: clip?.image?.reference ?? null,
        tags: [],
      }),
    );
    if (ok) {
      notice = t('reading.card.made');
      await load();
      onchange();
    }
  }
</script>

<aside aria-label={t('reading.word.heading')}>
  <h3>{surface} <span class="muted">({lemma})</span></h3>
  {#if info}
    <p role="status">{t(`reading.status.${info.status}`)}</p>
    {#each info.cards as c (c.card_id)}
      <p class="card">{c.word}{#if c.meaning}: {c.meaning}{/if}
        <span class="muted">{c.mature ? t('reading.word.mature') : t('reading.word.not_mature')}</span></p>
    {/each}
    {#if info.cards.length === 0}
      <div class="row">
        <button onclick={() => setMark('known')} disabled={info.mark === 'known'}>{t('reading.word.know')}</button>
        <button onclick={() => setMark('ignored')} disabled={info.mark === 'ignored'}>{t('reading.word.ignore')}</button>
        {#if info.mark}<button onclick={() => setMark(null)}>{t('reading.word.clear')}</button>{/if}
      </div>
    {/if}
  {/if}

  <form onsubmit={(e) => { e.preventDefault(); void makeCard(); }}>
    <h4>{t('reading.card.heading')}</h4>
    {#if sentence}<blockquote>{sentence}</blockquote>{/if}
    <label>{t('reading.card.kind')}
      <select bind:value={kind}>
        <option value="word">{t('reading.card.kind_word')}</option>
        <option value="sentence">{t('reading.card.kind_sentence')}</option>
      </select></label>
    <label>{t('reading.card.word')} <input bind:value={word} /></label>
    <label>{t('reading.card.meaning')} <input bind:value={meaning} /></label>
    {#if kind === 'sentence'}
      <label>{t('reading.card.translation')} <input bind:value={translation} /></label>
    {/if}
    {#if item?.media_path && cueIndex !== null}
      <button type="button" onclick={cutClip} disabled={clipping}>
        {clipping ? t('reading.card.clipping') : clip ? t('reading.card.clip_ready') : t('reading.card.clip')}</button>
    {/if}
    {#if !ctx.selectedDeck}<p class="muted">{t('reading.card.pick_deck')}</p>{/if}
    <button type="submit" disabled={!ctx.selectedDeck || !word.trim()}>{t('reading.card.make')}</button>
    {#if notice}<p role="status">{notice}</p>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  </form>
</aside>

<style>
  aside {
    border-left: 1px solid var(--fc-border, #ccc);
    padding-left: var(--fc-space-4);
    min-width: 16rem;
  }
  form,
  .row {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .row {
    flex-direction: row;
    flex-wrap: wrap;
  }
  blockquote {
    margin: 0;
    font-style: italic;
  }
</style>
