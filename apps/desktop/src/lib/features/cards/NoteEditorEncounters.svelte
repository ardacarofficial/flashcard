<!--
  "Where you saw this word" (v1.4): a note's Subtitle encounters, the main one marked. Make one
  the main example, move one to another card, merge this card with another (a word that differs
  asks first, then retries with `force`), and the ranking's "better example" proposal. Every
  rule lives in fc-app (`subtitle.encounters.*`); this only shows and re-reads.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { CardRow, PrimarySuggestion, SubtitleCaptureRow } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Badge, Button, Dialog, Input, Search, Sparkles, toast } from '$lib/ui';
  import { moduleKnownOn } from './modules.svelte';

  interface Props {
    noteId: string;
    /** After a merge that kept the other note: open that one. */
    onopennote?: (id: string) => void;
    /** After a write: the note's example fields may have changed in the core. */
    onchanged?: () => void;
  }
  let { noteId, onopennote, onchanged }: Props = $props();

  let rows = $state<SubtitleCaptureRow[]>([]);
  let suggestion = $state<PrimarySuggestion | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  /** The note picker: move one encounter, or merge the whole note. */
  let picker = $state<{ kind: 'move'; captureId: string } | { kind: 'merge' } | null>(null);
  let pickerOpen = $state(false);
  let query = $state('');
  let matches = $state<CardRow[]>([]);
  /** A merge the core refused because the words differ: asks before `force`. */
  let forceWith = $state<string | null>(null);
  let forceOpen = $state(false);

  const enabled = $derived(moduleKnownOn('subtitle'));
  const better = $derived(
    suggestion?.suggested ? (rows.find((r) => r.capture_id === suggestion?.suggested) ?? null) : null,
  );

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.unknown');
  }

  async function load() {
    const id = noteId;
    if (!moduleKnownOn('subtitle')) {
      rows = [];
      suggestion = null;
      return;
    }
    try {
      const list = (await api.call('subtitle.encounters.list', { note_id: id })).encounters;
      if (id !== noteId) return;
      rows = list;
      suggestion = moduleKnownOn('examples') && list.length > 1 ? await api.call('subtitle.encounters.suggest_primary', { note_id: id }) : null;
    } catch (e) {
      fail(e);
    }
  }

  $effect(() => {
    void noteId;
    void enabled;
    void load();
  });
  $effect(() => {
    let off: (() => void) | null = null;
    let gone = false;
    void api.on(['undone', 'redone'], () => void load()).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  });

  async function run(fn: () => Promise<unknown>, done: string) {
    busy = true;
    error = null;
    try {
      await fn();
      toast.show(done, { tone: 'success' });
      onchanged?.();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
      await load();
    }
  }

  const makePrimary = (captureId: string) =>
    run(() => api.call('subtitle.encounters.set_primary', { capture_id: captureId }), t('subtitle.encounter_panel.primary_set'));

  function openPicker(p: NonNullable<typeof picker>) {
    picker = p;
    query = '';
    matches = [];
    pickerOpen = true;
  }

  // Note search for the picker: the same search language as Browse (`search.cards`).
  $effect(() => {
    const q = query.trim();
    if (!pickerOpen || !q) {
      matches = [];
      return;
    }
    const timer = setTimeout(() => {
      api.call('search.cards', { query: q, notes_only: true, limit: 8 }).then(
        (page) => (matches = page.items.filter((r) => r.note_id !== noteId)),
        () => (matches = []),
      );
    }, 200);
    return () => clearTimeout(timer);
  });

  async function merge(other: string, force: boolean) {
    busy = true;
    error = null;
    try {
      const r = await api.call('subtitle.encounters.merge_notes', { a: noteId, b: other, force });
      toast.show(t('subtitle.encounter_panel.merged'), { tone: 'success' });
      if (r.kept_note_id !== noteId) onopennote?.(r.kept_note_id);
      else onchanged?.();
    } catch (e) {
      if ((e as ApiError).code === 'CONFLICT_SUBTITLE_MERGE_WORDS' && !force) {
        forceWith = other;
        forceOpen = true;
      } else fail(e);
    } finally {
      busy = false;
      await load();
    }
  }

  function pick(row: CardRow) {
    const p = picker;
    pickerOpen = false;
    picker = null;
    if (!p) return;
    if (p.kind === 'move') {
      void run(() => api.call('subtitle.encounters.move', { capture_id: p.captureId, to_note_id: row.note_id }), t('subtitle.encounter_panel.moved'));
    } else void merge(row.note_id, false);
  }

  function clock(ms: number): string {
    const s = Math.floor(ms / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const ss = String(s % 60).padStart(2, '0');
    return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
  }

  /** The line split around the word (UTF-16 offsets, as JS strings are). */
  function parts(r: SubtitleCaptureRow): [string, string, string] {
    const s = r.facts.sentence ?? '';
    const o = r.facts.term_offset;
    if (!o || o[0] < 0 || o[1] > s.length || o[0] >= o[1]) return [s, '', ''];
    return [s.slice(0, o[0]), s.slice(o[0], o[1]), s.slice(o[1])];
  }
</script>

{#if rows.length > 0}
  <section class="encounters" aria-labelledby="enc-title" data-testid="note-encounters">
    <h4 id="enc-title">{t('subtitle.encounter_panel.title')}</h4>

    {#if better}
      <p class="better" data-testid="better-example">
        <span>{t('subtitle.encounter_panel.better', { sentence: better.facts.sentence ?? '' })}</span>
        <Button size="sm" variant="secondary" icon={Sparkles} disabled={busy} onclick={() => makePrimary(better.capture_id)}>
          {t('subtitle.encounter_panel.better_use')}
        </Button>
      </p>
    {/if}

    <ul class="list">
      {#each rows as r (r.capture_id)}
        {@const [before, word, after] = parts(r)}
        <li class="row" class:primary={r.is_primary} data-testid="encounter-row">
          <div class="meta">
            <span class="source">{r.facts.title || r.facts.site || t('subtitle.encounter_panel.unknown_source')}</span>
            {#if r.facts.t_start_ms != null}<span class="time">{clock(r.facts.t_start_ms)}</span>{/if}
            {#if r.is_primary}<Badge tone="accent" variant="soft">{t('subtitle.encounter_panel.primary')}</Badge>{/if}
            {#if r.facts.audio_origin}
              <Badge variant="soft">{t('subtitle.encounter_panel.audio', { origin: t(`subtitle.encounter_panel.origin.${r.facts.audio_origin}`) })}</Badge>
            {/if}
          </div>
          {#if before || word || after}
            <p class="sentence">{before}{#if word}<mark>{word}</mark>{/if}{after}</p>
          {/if}
          <div class="actions">
            {#if !r.is_primary}
              <Button size="sm" variant="ghost" disabled={busy} onclick={() => makePrimary(r.capture_id)}>{t('subtitle.encounter_panel.make_primary')}</Button>
            {/if}
            <Button size="sm" variant="ghost" disabled={busy} onclick={() => openPicker({ kind: 'move', captureId: r.capture_id })}>
              {t('subtitle.encounter_panel.move')}
            </Button>
          </div>
        </li>
      {/each}
    </ul>
    <div class="foot">
      <Button size="sm" variant="ghost" disabled={busy} onclick={() => openPicker({ kind: 'merge' })}>{t('subtitle.encounter_panel.merge')}</Button>
      {#if error}<span class="error" role="alert">{error}</span>{/if}
    </div>
  </section>
{/if}

{#if pickerOpen && picker}
  <Dialog
    bind:open={pickerOpen}
    size="sm"
    title={picker.kind === 'move' ? t('subtitle.encounter_panel.move_title') : t('subtitle.encounter_panel.merge_title')}
    initialFocus="input"
    onclose={() => (picker = null)}
  >
    <Input type="search" size="sm" icon={Search} label={t('subtitle.encounter_panel.search')} hint={t('subtitle.encounter_panel.search_hint')} bind:value={query} />
    {#if query.trim()}
      {#if matches.length === 0}
        <p class="muted">{t('subtitle.encounter_panel.no_match')}</p>
      {:else}
        <ul class="matches" data-testid="note-picker-results">
          {#each matches as m (m.note_id)}
            <li>
              <span class="match"><span class="front">{m.sort_field}</span> <span class="muted">{m.deck_path}</span></span>
              <Button size="sm" variant="secondary" onclick={() => pick(m)}>
                {picker.kind === 'move' ? t('subtitle.encounter_panel.pick_move') : t('subtitle.encounter_panel.pick_merge')}
              </Button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </Dialog>
{/if}

{#if forceOpen && forceWith}
  <Dialog
    bind:open={forceOpen}
    role="alertdialog"
    size="sm"
    title={t('subtitle.encounter_panel.force_title')}
    description={t('subtitle.encounter_panel.force_hint')}
    initialFocus="[data-cancel]"
    onclose={() => (forceWith = null)}
  >
    {#snippet footer()}
      <Button variant="ghost" data-cancel onclick={() => (forceOpen = false)}>{t('common.cancel')}</Button>
      <Button
        variant="danger"
        onclick={() => {
          const other = forceWith;
          forceOpen = false;
          forceWith = null;
          if (other) void merge(other, true);
        }}>{t('subtitle.encounter_panel.force_confirm')}</Button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .encounters {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h4 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .list,
  .matches {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .row {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .row.primary {
    border-color: var(--accent);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .source {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .time {
    font-variant-numeric: tabular-nums;
  }
  .sentence {
    margin: var(--space-1) 0 0;
    font-size: var(--text-sm);
  }
  mark {
    background: none;
    color: inherit;
    font-weight: var(--weight-semibold);
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-underline-offset: 2px;
  }
  .actions,
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: center;
  }
  .actions {
    margin-top: var(--space-1);
  }
  .better {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    font-size: var(--text-xs);
  }
  .matches {
    margin-top: var(--space-3);
  }
  .matches li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    align-items: center;
  }
  .match {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>
