<!--
  Kelimelerim (v1.4, prototypes/v14/olustur-ara-istatistik.html route `words`): the word index
  as a list — a big search box (`/`), status chips with the counts of `words.stats`, language
  and sort, keyset paging (`words.list` cursor). A row opens a side panel: the status in one
  sentence, the word's cards (`reading.words.lookup`), frequency and CEFR band (`words.lookup`),
  the manual mark (`reading.words.mark`) and where it was seen (`subtitle.encounters.list`).
  Every status, count and level comes from the backend; the screen only shows them.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type {
    SubtitleCaptureRow,
    WordCard,
    WordEntryDto,
    WordFormResult,
    WordMark,
    WordSort,
    WordStats,
    WordStatus,
  } from '$lib/api/types';
  import { formatCount, formatDuration } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import { isEditableTarget } from '$lib/keyboard';
  import { moduleKnownOn, moduleOn, trackModules } from '../cards/modules.svelte';
  import type { ScreenCtx } from '../types';
  import {
    Button,
    Check,
    Chip,
    EmptyState,
    ExternalLink,
    EyeOff,
    Input,
    Kbd,
    PageHeader,
    Plus,
    RotateCcw,
    Search,
    Select,
    Skeleton,
    IconButton,
    X,
    type SelectOption,
  } from '$lib/ui';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const PAGE = 200;
  type Filter = 'all' | WordStatus;
  const STATUSES: WordStatus[] = ['new', 'learning', 'known', 'ignored'];

  let stats = $state<WordStats | null>(null);
  let words = $state<WordEntryDto[]>([]);
  let next = $state<string | null>(null);
  let loading = $state(true);
  let failed = $state(false);
  let query = $state('');
  let filter = $state<Filter>('all');
  let language = $state<string | null>(null);
  let sort = $state<WordSort>('key');
  /** Highlighted row (↑/↓) and the word whose panel is open. */
  let active = $state<string | null>(null);
  let open = $state<string | null>(null);
  let searchEl = $state<HTMLInputElement | null>(null);

  let form = $state<WordFormResult | null>(null);
  let cards = $state<WordCard[]>([]);
  let seen = $state<SubtitleCaptureRow[]>([]);
  let marking = $state(false);

  const withData = $derived(moduleKnownOn('word_data'));
  const lang = $derived(language ?? stats?.languages[0]?.language ?? null);
  const counts = $derived(stats?.languages.find((l) => l.language === lang)?.counts ?? null);
  const total = $derived(counts ? counts.new + counts.learning + counts.known + counts.ignored : 0);
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return words;
    const starts = words.filter((w) => w.key.startsWith(q));
    return [...starts, ...words.filter((w) => !w.key.startsWith(q) && w.key.includes(q))];
  });
  const selected = $derived(open ? (words.find((w) => w.key === open) ?? null) : null);

  const langOptions = $derived<SelectOption<string>[]>(
    (stats?.languages ?? []).map((l) => ({ value: l.language, label: l.language })),
  );
  const sortOptions = $derived<SelectOption<WordSort>[]>([
    { value: 'key', label: t('words.screen.sort.key') },
    ...(withData
      ? ([
          { value: 'rank', label: t('words.screen.sort.rank') },
          { value: 'band', label: t('words.screen.sort.band') },
        ] as SelectOption<WordSort>[])
      : []),
  ]);

  async function loadStats() {
    stats = await api.call('words.stats', {});
  }

  async function loadList(more = false) {
    const page = await api.call('words.list', {
      language: lang,
      status: filter === 'all' ? null : filter,
      cursor: more ? next : null,
      limit: PAGE,
      sort: withData ? sort : 'key',
      bands: [],
      max_rank: null,
    });
    words = more ? [...words, ...page.words] : page.words;
    next = page.next_cursor;
  }

  async function reload() {
    loading = true;
    try {
      await loadStats();
      await loadList();
      failed = false;
    } catch (e) {
      failed = true;
      await ctx.guarded(() => Promise.reject(e));
    } finally {
      loading = false;
    }
  }

  async function refilter() {
    await ctx.guarded(() => loadList());
  }

  function setFilter(f: Filter) {
    filter = f;
    void refilter();
  }

  async function openWord(key: string) {
    active = key;
    open = key;
    form = null;
    cards = [];
    seen = [];
    const l = lang ?? '';
    await ctx.guarded(async () => {
      const r = await api.call('words.lookup', { language: l, forms: [key] });
      if (open === key) form = r.forms[0] ?? null;
    });
    if (moduleOn('reading')) {
      try {
        const info = await api.call('reading.words.lookup', { language: l, lemma: key, tz_offset_mins: -new Date().getTimezoneOffset() });
        if (open === key) cards = info.cards;
      } catch {
        /* reading switched off: the card count of the row stays */
      }
    }
    if (moduleKnownOn('subtitle')) {
      try {
        const list = await api.call('subtitle.encounters.list', { lang: l, lemma: key });
        if (open === key) seen = list.encounters;
      } catch {
        /* no encounters to show */
      }
    }
  }

  function close() {
    open = null;
  }

  async function mark(to: WordMark | null) {
    if (!selected || !lang) return;
    const key = selected.key;
    marking = true;
    const ok = await ctx.guarded(() => api.call('reading.words.mark', { language: lang, lemmas: [key], mark: to }));
    if (ok) {
      await ctx.guarded(async () => {
        await loadStats();
        await loadList();
      });
      await openWord(key);
    }
    marking = false;
  }

  function move(step: number) {
    if (!shown.length) return;
    const i = shown.findIndex((w) => w.key === active);
    const j = i < 0 ? (step > 0 ? 0 : shown.length - 1) : Math.max(0, Math.min(shown.length - 1, i + step));
    active = shown[j]!.key;
    void tick().then(() => document.getElementById(rowId(active!))?.scrollIntoView?.({ block: 'nearest' }));
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    const inSearch = e.target === searchEl;
    if (e.key === '/' && !isEditableTarget(e.target)) {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && (inSearch || !isEditableTarget(e.target))) {
      e.preventDefault();
      move(e.key === 'ArrowDown' ? 1 : -1);
    } else if (e.key === 'Enter' && active && (inSearch || !isEditableTarget(e.target))) {
      if (e.target instanceof HTMLButtonElement) return;
      e.preventDefault();
      void openWord(active);
    } else if (e.key === 'Escape' && open) {
      e.preventDefault();
      close();
    }
  }

  const rowId = (key: string) => `word-${key.replace(/\W+/g, '-')}`;
  const statusLabel = (s: WordStatus) => t(`words.screen.status.${s}`);
  const time = (ms: number | null | undefined) => (ms == null ? '' : formatDuration(ms));

  function sentence(w: WordEntryDto): string {
    if (w.status === 'known') {
      if (w.mark === 'known') return t('words.screen.detail.say.known_manual');
      return w.max_interval_days != null
        ? t('words.screen.detail.say.known', { days: formatCount(w.max_interval_days) })
        : t('words.screen.detail.say.known_plain');
    }
    return t(`words.screen.detail.say.${w.status}`);
  }

  onMount(() => {
    trackModules();
    void reload();
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="words">
  <PageHeader title={t('words.screen.title')} subtitle={t('words.screen.lead')} />

  {#if loading && !stats}
    <Skeleton lines={6} />
  {:else if failed && !stats}
    <EmptyState title={t('words.screen.load_failed')} action={{ label: t('words.screen.retry'), onclick: () => void reload() }} />
  {:else if stats && total === 0 && stats.languages.length <= 1}
    {#if stats.unassigned_notes > 0}
      <p class="hint" data-testid="words-unassigned">{t('words.screen.unassigned', { count: stats.unassigned_notes })}</p>
    {/if}
    <EmptyState
      title={t('words.screen.empty.title')}
      description={t('words.screen.empty.body')}
      action={{ label: t('words.screen.empty.action'), icon: Plus, onclick: () => ctx.navigate?.('/notes/new') }}
    />
  {:else if stats}
    {#if stats.unassigned_notes > 0}
      <p class="hint" data-testid="words-unassigned">{t('words.screen.unassigned', { count: stats.unassigned_notes })}</p>
    {/if}

    <div class="search">
      <Input
        type="search"
        size="lg"
        icon={Search}
        label={t('words.screen.search_label')}
        hideLabel
        placeholder={t('words.screen.search_placeholder')}
        autocomplete="off"
        spellcheck={false}
        aria-keyshortcuts="/"
        aria-controls="word-list"
        bind:value={query}
        bind:element={searchEl}
      />
    </div>

    <div class="filters">
      <div class="chips" role="group" aria-label={t('words.screen.filters')}>
        <Chip selected={filter === 'all'} onclick={() => setFilter('all')}>
          {t('words.screen.all')} <span class="num" data-testid="words-count-all">{formatCount(total)}</span>
        </Chip>
        {#each STATUSES as s (s)}
          {#if s !== 'ignored' || (counts?.ignored ?? 0) > 0 || filter === 'ignored'}
            <Chip selected={filter === s} onclick={() => setFilter(s)}>
              <span class="dot {s}" aria-hidden="true"></span>{statusLabel(s)}
              <span class="num" data-testid="words-count-{s}">{formatCount(counts?.[s] ?? 0)}</span>
            </Chip>
          {/if}
        {/each}
      </div>
      <span class="sp"></span>
      {#if stats.languages.length > 1}
        <Select
          size="sm"
          label={t('words.screen.language')}
          hideLabel
          options={langOptions}
          value={lang}
          onchange={(v) => {
            language = v;
            void reload();
          }}
        />
      {/if}
      {#if withData}
        <Select
          size="sm"
          label={t('words.screen.sort.label')}
          hideLabel
          options={sortOptions}
          bind:value={sort}
          onchange={() => void refilter()}
        />
      {/if}
    </div>

    <div class="split" class:has-panel={!!selected}>
      <div class="list-wrap">
        {#if shown.length === 0}
          {#if query.trim()}
            <EmptyState size="sm" title={t('words.screen.no_match.title', { query: query.trim() })} description={t('words.screen.no_match.body')} />
          {:else}
            <EmptyState size="sm" title={t('words.screen.filter_empty.title')} description={t('words.screen.filter_empty.body')} />
          {/if}
        {:else}
          <div class="wl" id="word-list" role="listbox" aria-label={t('words.screen.list_label')} tabindex="-1">
            {#each shown as w (w.key)}
              <div
                class="row"
                role="option"
                id={rowId(w.key)}
                tabindex="-1"
                aria-selected={active === w.key}
                class:open={open === w.key}
                data-testid="word-row"
                onclick={() => void openWord(w.key)}
                onkeydown={() => {}}
              >
                <span class="term">{w.key}</span>
                <span class="st"><span class="dot {w.status}" aria-hidden="true"></span>{statusLabel(w.status)}</span>
                <span class="rank num" title={t('words.screen.rank_title')}>
                  {#if w.level?.rank != null}{t('words.screen.rank', { rank: formatCount(w.level.rank) })}{/if}
                </span>
                <span class="band">{#if w.level?.band}<Chip size="sm" label={w.level.band} />{/if}</span>
                <span class="cards small">
                  {w.cards ? t('words.screen.cards', { count: w.cards }) : t('words.screen.no_cards')}
                </span>
              </div>
            {/each}
          </div>
        {/if}
        <div class="foot">
          <span class="small muted">{t('words.screen.shown', { shown: formatCount(shown.length) })}</span>
          {#if next}
            <Button size="sm" onclick={() => void ctx.guarded(() => loadList(true))}>{t('words.screen.more')}</Button>
          {/if}
          <span class="sp"></span>
          <span class="keys small muted">
            <span><Kbd keys={['/']} size="sm" /> {t('words.screen.keys.search')}</span>
            <span><Kbd keys={['ArrowUp']} size="sm" /><Kbd keys={['ArrowDown']} size="sm" /> {t('words.screen.keys.move')}</span>
            <span><Kbd keys={['Enter']} size="sm" /> {t('words.screen.keys.open')}</span>
            <span><Kbd keys={['Esc']} size="sm" /> {t('words.screen.keys.close')}</span>
          </span>
        </div>
      </div>

      {#if selected}
        <aside class="panel" aria-label={t('words.screen.detail.label', { word: selected.key })} data-testid="word-detail">
          <div class="dh">
            <div class="word">{selected.key}</div>
            <IconButton icon={X} label={t('words.screen.detail.close')} onclick={close} />
          </div>

          <section>
            <div class="lab">{t('words.screen.detail.status')}</div>
            <div class="st"><span class="dot {selected.status}" aria-hidden="true"></span>{statusLabel(selected.status)}</div>
            <p class="say" data-testid="word-say">{sentence(selected)}</p>
            {#if moduleOn('reading')}
              <div class="actions">
                {#if selected.status !== 'known'}
                  <Button size="sm" icon={Check} loading={marking} onclick={() => void mark('known')}>{t('words.screen.detail.mark_known')}</Button>
                {/if}
                {#if selected.mark}
                  <Button size="sm" icon={RotateCcw} loading={marking} onclick={() => void mark(null)}>{t('words.screen.detail.relearn')}</Button>
                {/if}
                {#if selected.status !== 'ignored'}
                  <Button size="sm" variant="ghost" icon={EyeOff} loading={marking} onclick={() => void mark('ignored')}>{t('words.screen.detail.ignore')}</Button>
                {/if}
              </div>
            {/if}
          </section>

          <section>
            <div class="lab">{t('words.screen.detail.cards')}</div>
            {#if cards.length}
              <ul class="cardlist">
                {#each cards as c (c.card_id)}
                  <li>
                    <span class="grow"><b>{c.word}</b>{#if c.meaning} · {c.meaning}{/if}</span>
                    {#if c.interval_days != null}<span class="small muted num">{t('words.screen.detail.interval', { days: formatCount(c.interval_days) })}</span>{/if}
                  </li>
                {/each}
              </ul>
            {:else if selected.cards === 0}
              <p class="small muted">{t('words.screen.detail.no_cards')}</p>
            {:else}
              <p class="small">{t('words.screen.cards', { count: selected.cards })}</p>
            {/if}
            {#if selected.notes > 0}
              <Button size="sm" variant="ghost" icon={ExternalLink} onclick={() => ctx.navigate?.('/browse')}>{t('words.screen.detail.open_note')}</Button>
            {/if}
          </section>

          {#if withData}
            <section>
              <div class="lab">{t('words.screen.detail.level')}</div>
              {#if form?.level && (form.level.rank != null || form.level.band)}
                {#if form.level.rank != null}
                  <p class="small">{t('words.screen.detail.rank_line', { rank: formatCount(form.level.rank) })}</p>
                {/if}
                {#if form.level.band}
                  <p class="small">
                    {t('words.screen.detail.band_line', { band: form.level.band })}
                    {#if form.level.estimated}<span class="muted">({t('words.screen.detail.estimated')})</span>{/if}
                  </p>
                {/if}
              {:else}
                <p class="small muted">{t('words.screen.detail.no_level')}</p>
              {/if}
            </section>
          {/if}

          {#if moduleKnownOn('subtitle')}
            <section>
              <div class="lab">{t('words.screen.detail.seen')}</div>
              {#if seen.length}
                <ul class="seen">
                  {#each seen as s (s.capture_id)}
                    <li data-testid="word-encounter">
                      {#if s.facts.sentence}<div class="example">{s.facts.sentence}</div>{/if}
                      <div class="small muted">
                        {s.facts.title ?? s.facts.site ?? ''}{#if s.facts.t_start_ms != null} · <span class="num">{time(s.facts.t_start_ms)}</span>{/if}
                      </div>
                    </li>
                  {/each}
                </ul>
              {:else}
                <p class="small muted">{t('words.screen.detail.no_seen')}</p>
              {/if}
            </section>
          {/if}
        </aside>
      {/if}
    </div>
  {/if}
</div>

<style>
  .words {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .hint {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .filters {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .sp {
    flex: 1;
  }
  .num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .small {
    font-size: var(--text-xs);
  }
  .muted {
    color: var(--text-2);
  }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    margin-right: var(--space-1);
    background: var(--text-3);
    vertical-align: middle;
  }
  .dot.learning {
    background: var(--warning);
  }
  .dot.known {
    background: var(--success);
  }
  .dot.ignored {
    background: transparent;
    border: 1px solid var(--text-3);
  }
  .split {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
  }
  .split.has-panel {
    grid-template-columns: minmax(0, 1fr) minmax(280px, 360px);
  }
  .wl {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1.3fr) 5.5rem 3.5rem minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: 0 var(--space-3);
    border-top: 1px solid var(--border);
    cursor: pointer;
  }
  .row:first-child {
    border-top: 0;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row[aria-selected='true'],
  .row.open {
    background: var(--surface-3);
  }
  .term {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rank {
    text-align: right;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .cards {
    color: var(--text-2);
    text-align: right;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }
  .keys {
    display: flex;
    gap: var(--space-3);
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    position: sticky;
    top: var(--space-3);
  }
  .dh {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .word {
    font-size: var(--text-xl);
    font-weight: 600;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .lab {
    font-size: var(--text-2xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .say,
  p {
    margin: 0;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .cardlist li {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .example {
    font-style: italic;
  }
  @media (max-width: 900px) {
    .split.has-panel {
      grid-template-columns: minmax(0, 1fr);
    }
    .keys {
      display: none;
    }
  }
</style>
