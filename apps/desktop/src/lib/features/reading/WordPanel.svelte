<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/api';
  import type { ReadingCardKind, ReadingClip, ReadingItemDto, WordInfo, WordMark } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { followJob, isFinal, resultOf } from '../io/job';
  import type { ScreenCtx } from '../types';
  import { tzOffset } from '../stats/format';
  import { Button, Check, Chip, IconButton, Input, SegmentedControl, X } from '$lib/ui';

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
    onclose,
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
    onclose?: () => void;
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

<aside class="panel" aria-label={t('reading.word.heading')}>
  <header>
    <div>
      <h3>{surface}</h3>
      {#if lemma !== surface}<span class="lemma">{lemma}</span>{/if}
    </div>
    {#if onclose}<IconButton size="sm" icon={X} label={t('ui.close')} onclick={onclose} />{/if}
  </header>
  {#if info}
    <div class="status">
      <Chip size="sm" tone={info.status === 'new' ? 'accent' : info.status === 'learning' ? 'warning' : 'neutral'} label={t(`reading.status.${info.status}`)} />
      <span class="sr" role="status">{t(`reading.status.${info.status}`)}</span>
    </div>
    {#each info.cards as c (c.card_id)}
      <p class="card">{c.word}{#if c.meaning}: {c.meaning}{/if}
        <span class="muted">{c.mature ? t('reading.word.mature') : t('reading.word.not_mature')}</span></p>
    {/each}
    {#if info.cards.length === 0}
      <div class="row">
        <Button size="sm" icon={Check} onclick={() => setMark('known')} disabled={info.mark === 'known'}>{t('reading.word.know')}</Button>
        <Button size="sm" variant="ghost" onclick={() => setMark('ignored')} disabled={info.mark === 'ignored'}>{t('reading.word.ignore')}</Button>
        {#if info.mark}<Button size="sm" variant="ghost" onclick={() => setMark(null)}>{t('reading.word.clear')}</Button>{/if}
      </div>
    {/if}
  {/if}

  <form onsubmit={(e) => { e.preventDefault(); void makeCard(); }}>
    <h4>{t('reading.card.heading')}</h4>
    {#if sentence}<blockquote>{sentence}</blockquote>{/if}
    <SegmentedControl
      size="sm"
      fullWidth
      label={t('reading.card.kind')}
      options={[
        { value: 'word' as ReadingCardKind, label: t('reading.card.kind_word') },
        { value: 'sentence' as ReadingCardKind, label: t('reading.card.kind_sentence') },
      ]}
      bind:value={kind}
    />
    <Input label={t('reading.card.word')} bind:value={word} />
    <Input label={t('reading.card.meaning')} bind:value={meaning} />
    {#if kind === 'sentence'}
      <Input label={t('reading.card.translation')} bind:value={translation} />
    {/if}
    {#if item?.media_path && cueIndex !== null}
      <Button size="sm" onclick={cutClip} loading={clipping} disabled={clipping}>
        {clipping ? t('reading.card.clipping') : clip ? t('reading.card.clip_ready') : t('reading.card.clip')}</Button>
    {/if}
    {#if !ctx.selectedDeck}<p class="muted">{t('reading.card.pick_deck')}</p>{/if}
    <Button variant="primary" type="submit" disabled={!ctx.selectedDeck || !word.trim()}>{t('reading.card.make')}</Button>
    {#if notice}<p class="ok" role="status">{notice}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
</aside>

<style>
  .panel {
    position: sticky;
    top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-2);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    overflow-wrap: anywhere;
  }
  .lemma {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  h4 {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  form,
  .row {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    flex-direction: row;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  form {
    border-top: 1px solid var(--border);
    padding-top: var(--space-2);
  }
  blockquote {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-left: 3px solid var(--accent);
    background: var(--surface-2);
    border-radius: 0 var(--radius-control) var(--radius-control) 0;
    font-style: italic;
    font-size: var(--text-sm);
  }
  .card {
    margin: 0;
    font-size: var(--text-sm);
  }
  .muted {
    color: var(--text-2);
    margin: 0;
    font-size: var(--text-sm);
  }
  .ok {
    color: var(--success);
    margin: 0;
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
