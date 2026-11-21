<script lang="ts">
  /**
   * Flashcards (spaced repetition, docs/11 §3, v1.2 flow 2): a dense start panel; then the
   * session owns the window (`SessionFrame`: progress, "12 / 40", remaining, Esc) with the
   * card centred (max 720) and answer buttons exactly as wide as it — two by default ("Still
   * learning" = again, "I know it" = good) or the four ratings with their next intervals
   * (`study.answer_buttons`, a display preference stored in settings). The backend runs the
   * session; this screen only shows and reports.
   */
  import { formatCount, formatPercent100 } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type {
    AnswerCheckDto,
    AttributeDefDto,
    FlagDto,
    Rating,
    SavedQueryDto,
    SessionDto,
    SessionSummary,
    StartModes,
    StartSession,
  } from '$lib/api/types';
  import { ApiError } from '$lib/api';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    ChevronDown,
    ChevronUp,
    Chip,
    CircleCheck,
    Flag,
    Flashcard,
    Icon,
    Input,
    Menu,
    OptionTile,
    PageHeader,
    ProgressRing,
    Repeat,
    Select,
    Skeleton,
    Switch,
    Undo2,
    type MenuItem,
  } from '$lib/ui';
  import type { DeckCounts, DeckTreeNode } from '$lib/api/types';
  import StudyAi from '../ai/StudyAi.svelte';
  import CardView from '../cards/CardView.svelte';
  import PluginToolbar from '../plugins/PluginToolbar.svelte';
  import { defaultFlagKey, flagColor, hasFlag } from '../cards/flags';
  import { moduleKnownOn, moduleOn, trackModules } from '../cards/modules.svelte';
  import type { ScreenCtx } from '../types';
  import { intervalLabel, tzOffsetMins } from './format';
  // v0.10 study modes: start options, timer, confidence scale, focus, inline cards.
  import ConfidenceButtons from '../modes/ConfidenceButtons.svelte';
  import FocusToggle from '../modes/FocusToggle.svelte';
  import InlineComposer from '../modes/InlineComposer.svelte';
  import OptRow from '../modes/OptRow.svelte';
  import SessionFrame from '../modes/SessionFrame.svelte';
  import SetupPanel from '../modes/SetupPanel.svelte';
  import StudyModeOptions from '../modes/StudyModeOptions.svelte';
  import TimerBar from '../modes/TimerBar.svelte';
  import { plainKey } from '../modes/modes';
  import { keyAction, RATINGS, studyBindings, type StudyAction, type StudyPhase } from './keys';

  let { ctx }: { ctx: ScreenCtx } = $props();

  /** v1.1 display preference (`study.answer_buttons`): two or four answer buttons. */
  const BUTTONS_KEY = 'study.answer_buttons';

  let session = $state<SessionDto | null>(null);
  let summary = $state<SessionSummary | null>(null);
  let resumable = $state<SessionDto | null>(null);
  let loading = $state(true);
  let revealed = $state(false);
  /** Which face the card shows (the learner may flip back to the question). */
  let flipped = $state(false);
  let hint = $state<string | null>(null);
  let typed = $state('');
  let check = $state<AnswerCheckDto | null>(null);
  let practice = $state(false);
  let attrs = $state<AttributeDefDto[]>([]);
  let filterAttr = $state('');
  let filterValue = $state('');
  /** Custom session from the query language (v0.4), or a saved query ("filtered deck"). */
  let customQuery = $state('');
  let savedQueries = $state<SavedQueryDto[]>([]);
  let savedId = $state('');
  let postponeDays = $state<number | string>(1);
  let modeOptions = $state<StartModes | null>(null);
  let composing = $state(false);
  let showOptions = $state(false);
  let notice = $state<string | null>(null);
  let buttons = $state<'two' | 'four'>('two');
  let busy = false;
  let shownAt = 0;
  let revealedAt: number | null = null;
  let input = $state<HTMLInputElement | null>(null);
  /** Flag names/colors (`flags.list`, Anki numbering from `card_flag_defs`). */
  let flagDefs = $state<FlagDto[]>([]);
  /** Picked option of a choice card. */
  let picked = $state<number | null>(null);
  /**
   * Study keys from `shortcuts.list`; `undefined` = built-in defaults. Loaded the same way as
   * the shell's `ShortcutHost` (not gated by the palette UI) so both agree on who handles
   * Ctrl+Z.
   */
  let bindings = $state<Map<string, string> | undefined>(undefined);

  async function loadBindings() {
    try {
      bindings = studyBindings((await api.call('shortcuts.list', {})).items);
    } catch {
      bindings = undefined;
    }
  }

  async function loadButtons() {
    try {
      const r = await api.call('settings.get', { scope: { kind: 'app' }, keys: [BUTTONS_KEY] });
      const v = r.values.find((x) => x.key === BUTTONS_KEY)?.value;
      buttons = v === 'four' ? 'four' : 'two';
    } catch {
      buttons = 'two';
    }
  }

  $effect(() => {
    void loadBindings();
    void loadButtons();
    const unsub = api.on(['setting_changed', 'module_toggled'], () => {
      void loadBindings();
      void loadButtons();
    });
    return () => void unsub.then((u) => u());
  });

  // The shell's global undo (Ctrl+Z, palette) or another screen may undo an answer: refresh.
  $effect(() => {
    const unsub = api.on(['undone', 'redone', 'data_changed'], () => {
      if (session && !busy) void ctx.guarded(reload);
    });
    return () => void unsub.then((u) => u());
  });

  const card = $derived(session?.current ?? null);
  const choice = $derived(card?.sides.interaction.kind === 'choice' ? card.sides.interaction : null);
  const cardFlags = $derived(
    card && moduleOn('flags') ? flagDefs.filter((f) => hasFlag(card.flags, f.flag)) : [],
  );
  const phase = $derived<StudyPhase>(summary ? 'done' : card ? (revealed ? 'answer' : 'question') : 'idle');
  // A running session owns the window (v1.2: no sidebar, no top bar); the summary is a
  // normal page again, so the learner can go anywhere from it.
  $effect(() => ctx.setFocus?.(phase === 'question' || phase === 'answer'));
  const filterDef = $derived(attrs.find((a) => a.id === filterAttr) ?? null);
  const twoButtons = $derived(buttons === 'two');

  onMount(() => {
    trackModules();
    void ctx
      .guarded(async () => {
        const open = (await api.call('study.active', {})).session;
        // Learn, test and match sessions resume on their own screens.
        resumable = open && !['learn', 'test', 'match'].includes(open.kind) ? open : null;
      })
      .finally(() => (loading = false));
    void loadSaved();
    const unsub = api.on(['saved_queries', 'module_toggled'], () => void loadSaved());
    return () => void unsub.then((u) => u());
  });

  async function loadSaved() {
    try {
      savedQueries = moduleOn('browse') ? (await api.call('saved_queries.list', {})).items : [];
    } catch {
      savedQueries = [];
    }
  }

  async function loadFlags() {
    try {
      flagDefs = moduleOn('flags') ? (await api.call('flags.list', {})).flags : [];
    } catch {
      flagDefs = [];
    }
  }

  $effect(() => {
    void loadFlags();
    const unsub = api.on(['flags_changed', 'module_toggled', 'undone', 'redone', 'data_changed'], () => void loadFlags());
    return () => void unsub.then((u) => u());
  });

  // Classifications of the selected deck feed the custom-session filter.
  $effect(() => {
    const deck = ctx.selectedDeck;
    filterAttr = '';
    filterValue = '';
    attrs = [];
    if (!deck || !moduleKnownOn('attributes')) return;
    void api.call('attributes.effective', { deck_id: deck, target: 'note' }).then(
      (r) => {
        if (ctx.selectedDeck === deck) {
          attrs = r.attributes
            .map((a) => a.def)
            .filter((d) => d.kind === 'enum' || d.kind === 'ordered_enum' || d.kind === 'multi_enum');
        }
      },
      () => (attrs = []),
    );
  });

  // A new classification clears its value.
  $effect(() => {
    void filterAttr;
    filterValue = '';
  });

  function show(s: SessionDto) {
    session = s;
    summary = null;
    revealed = false;
    flipped = false;
    hint = null;
    typed = '';
    check = null;
    picked = null;
    shownAt = Date.now();
    revealedAt = null;
  }

  /**
   * Timed mode (v0.10): the countdown of a card starts when the screen puts it up — the
   * backend refuses to restart one, so a resumed or undone card keeps what it has.
   */
  async function startTimer(s: SessionDto): Promise<SessionDto> {
    const timer = s.modes?.timer;
    if (!timer || !s.current || timer.deadline !== null || s.ended_at !== null) return s;
    return api.call('modes.timed.start', { session_id: s.id, card_id: s.current.card_id });
  }

  async function settle(s: SessionDto) {
    if (s.current) show(await startTimer(s));
    else await finish(s);
  }

  async function finish(s: SessionDto) {
    session = s;
    summary = await api.call('study.end', { session_id: s.id });
    resumable = null;
  }

  const queryText = $derived(customQuery.trim());
  const custom = $derived((filterAttr !== '' && filterValue !== '') || queryText !== '' || savedId !== '');

  function startRequest(): StartSession {
    const filtered = filterAttr !== '' && filterValue !== '';
    const byQuery = queryText !== '' || savedId !== '';
    return {
      kind: custom ? 'custom' : 'normal',
      mode: practice ? 'trial' : 'scheduled',
      deck_id: ctx.selectedDeck,
      filter: custom
        ? {
            attributes: filtered ? [{ attr_id: filterAttr, values: [filterValue] }] : [],
            // A query decides due-ness itself (`is:due`); the classification picker studies
            // what is due.
            due_only: byQuery ? null : true,
            query: savedId === '' && queryText !== '' ? queryText : null,
            saved_query_id: savedId === '' ? null : savedId,
          }
        : null,
      tz_offset_mins: tzOffsetMins(),
      modes: modeOptions,
    };
  }

  async function start() {
    notice = null;
    const s = await api.call('study.start', startRequest());
    if (!s.current) {
      await api.call('study.end', { session_id: s.id });
      notice = t('study.nothing');
      return;
    }
    show(await startTimer(s));
  }

  async function reload() {
    const id = session?.id ?? summary?.session_id;
    if (!id) return;
    await settle(await api.call('study.session', { session_id: id }));
  }

  /** Refreshes the shown card's data (flags) without leaving the answer side. */
  async function refresh() {
    if (!session) return;
    const s = await api.call('study.session', { session_id: session.id });
    if (s.current && s.current.card_id === card?.card_id) session = await startTimer(s);
    else await settle(s);
  }

  async function checkTyped() {
    if (!session || !card || typed.trim() === '') return;
    check = await api.call('study.check_answer', {
      session_id: session.id,
      card_id: card.card_id,
      given: typed,
      choice: null,
    });
  }

  /** Picks an option of a choice card: the card type engine judges it, then the answer shows. */
  async function choose(index: number) {
    if (!session || !card || !choice || revealed) return;
    picked = index;
    check = await api.call('study.check_answer', {
      session_id: session.id,
      card_id: card.card_id,
      given: '',
      choice: index,
    });
    await reveal();
  }

  async function reveal() {
    if (card?.type_answer && !check) await checkTyped();
    revealed = true;
    flipped = true;
    revealedAt = Date.now();
    input?.blur();
  }

  async function rate(rating: Rating) {
    if (!session || !card) return;
    const now = Date.now();
    try {
      const next = await api.call('study.answer', {
        session_id: session.id,
        card_id: card.card_id,
        rating,
        duration_ms: Math.max(0, now - shownAt),
        reveal_ms: revealedAt === null ? null : Math.max(0, revealedAt - shownAt),
      });
      await settle(next);
    } catch (e) {
      // Timed mode: the backend clock says the time ran out.
      if (e instanceof ApiError && e.code === 'CONFLICT_TIME_UP') await expire();
      else throw e;
    }
  }

  /** Confidence scale (v0.10): level 1–5, mapped onto a rating by the backend. */
  async function rateConfidence(level: number) {
    if (!session || !card) return;
    const now = Date.now();
    try {
      const next = await api.call('modes.confidence.answer', {
        session_id: session.id,
        card_id: card.card_id,
        level,
        duration_ms: Math.max(0, now - shownAt),
        reveal_ms: revealedAt === null ? null : Math.max(0, revealedAt - shownAt),
      });
      await settle(next);
    } catch (e) {
      // Timed mode: the backend clock says the time ran out (as in `rate`).
      if (e instanceof ApiError && e.code === 'CONFLICT_TIME_UP') await expire();
      else throw e;
    }
  }

  /** Timed mode (v0.10): the card's time ran out. */
  async function expire() {
    if (!session || !card) return;
    await settle(await api.call('modes.timed.expire', { session_id: session.id, card_id: card.card_id }));
  }

  async function resume(s: SessionDto) {
    // Timed: a card whose countdown never started gets it now (`settle`).
    await settle(s);
  }

  /** Two or four answer buttons: a display preference the backend validates and stores. */
  async function setButtons(v: 'two' | 'four') {
    buttons = v;
    await api.call('settings.set', { scope: { kind: 'app' }, key: BUTTONS_KEY, value: v });
  }

  async function run(a: StudyAction) {
    if (busy) return;
    busy = true;
    try {
      await ctx.guarded(async () => {
        const s = session;
        const c = card;
        switch (a.kind) {
          case 'start':
            return start();
          case 'reveal':
            return reveal();
          case 'rate':
            return rate(a.rating);
          case 'choose':
            return choose(a.index);
          case 'flag':
            if (c) {
              await api.call('cards.set_flag', { ids: [c.card_id], flag: a.flag, on: !hasFlag(c.flags, a.flag) });
              await refresh();
            }
            return;
          case 'hint':
            if (s && c) {
              const h = await api.call('study.hint', { session_id: s.id, card_id: c.card_id });
              hint = h.text ?? hint;
            }
            return;
          case 'suspend':
            if (s && c) {
              await api.call('study.suspend', { card_ids: [c.card_id], suspended: true, session_id: s.id });
              await reload();
            }
            return;
          case 'bury':
            if (s && c) {
              await api.call('study.bury', {
                card_ids: [c.card_id],
                buried: true,
                session_id: s.id,
                tz_offset_mins: tzOffsetMins(),
              });
              await reload();
            }
            return;
          case 'undo':
            await api.call('oplog.undo', {});
            return reload();
          case 'end':
            if (s) await finish(s);
            return;
          case 'repeat':
            if (summary && summary.mistakes > 0) {
              const r = await api.call('study.start', {
                kind: 'mistakes',
                mode: practice ? 'trial' : 'scheduled',
                from_session: summary.session_id,
                tz_offset_mins: tzOffsetMins(),
                modes: modeOptions,
              });
              await settle(r);
            }
            return;
          case 'back':
            summary = null;
            session = null;
            return;
        }
      });
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    // The card (or another control) handled it already (e.g. Space on the focused card).
    if (e.defaultPrevented) return;
    const el = e.target as HTMLElement | null;
    const tag = el?.tagName ?? '';
    const typing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
    // Buttons handle their own Enter/Space; keep shortcuts working everywhere else.
    const asButton = tag === 'BUTTON' || (el instanceof HTMLElement && el.getAttribute('role') === 'button');
    if (asButton && (e.key === 'Enter' || e.key === ' ')) return;
    // v0.10: inline cards (N), confidence levels (1–5) on the answer side.
    if (card && plainKey(e) && e.key.toLowerCase() === 'n' && moduleOn('inline_cards')) {
      e.preventDefault();
      composing = true;
      return;
    }
    if (phase === 'answer' && confidence && plainKey(e) && /^[1-5]$/.test(e.key)) {
      e.preventDefault();
      if (!busy) {
        busy = true;
        void ctx.guarded(() => rateConfidence(Number(e.key))).finally(() => (busy = false));
      }
      return;
    }
    const action = keyAction(phase, e, typing, {
      choices: choice?.options.length ?? 0,
      flags: moduleOn('flags'),
      bindings,
      twoButtons,
    });
    if (!action) return;
    e.preventDefault();
    if (typing) el?.blur();
    void run(action);
  }

  function onTypedKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void run({ kind: 'reveal' });
    }
  }

  /** The learner flipped the card (click / Space on it): the first flip reveals the answer. */
  function onflip(f: boolean) {
    if (f && !revealed) void run({ kind: 'reveal' });
  }

  async function postpone() {
    notice = null;
    await ctx.guarded(async () => {
      const r = await api.call('study.postpone', {
        deck_id: ctx.selectedDeck,
        days: Number(postponeDays) || 1,
        tz_offset_mins: tzOffsetMins(),
      });
      notice = t('study.postponed', { count: r.cards });
    });
  }

  function interval(ms: number): string {
    const l = intervalLabel(ms);
    return t(l.key, { n: l.n });
  }

  const confidence = $derived(session?.modes?.rating_scale === 'confidence' && (session.modes?.confidence.length ?? 0) > 0);

  const accuracy = $derived(summary && summary.answered > 0 ? Math.round((summary.correct * 100) / summary.answered) : 0);

  // "12 / 40": this card's position among answered + remaining (display only).
  const remaining = $derived(session ? session.counts.new + session.counts.learning + session.counts.review : 0);
  const position = $derived(session ? session.answered + 1 : 0);
  const total = $derived(session ? session.answered + Math.max(remaining, 1) : 0);

  const attrOptions = $derived([
    { value: '', label: t('study.filter_any') },
    ...attrs.map((a) => ({ value: a.id, label: a.name })),
  ]);
  const valueOptions = $derived([
    { value: '', label: t('study.filter_any') },
    ...(filterDef?.options ?? []).map((o) => ({ value: o.id, label: o.label })),
  ]);
  const savedOptions = $derived([
    { value: '', label: t('study.filter_any') },
    ...savedQueries.map((q) => ({ value: q.id, label: q.name })),
  ]);

  const toolItems = $derived<MenuItem[]>([
    { id: 'suspend', label: t('study.suspend'), keys: ['S'], onselect: () => void run({ kind: 'suspend' }) },
    { id: 'bury', label: t('study.bury'), keys: ['B'], onselect: () => void run({ kind: 'bury' }) },
    ...(moduleOn('inline_cards')
      ? [{ id: 'inline', label: t('modes.inline.open'), keys: ['N'], onselect: () => (composing = true) }]
      : []),
    { id: 'sep', separator: true as const },
    twoButtons
      ? { id: 'buttons', label: t('study.buttons.use_four'), onselect: () => void ctx.guarded(() => setButtons('four')) }
      : { id: 'buttons', label: t('study.buttons.use_two'), onselect: () => void ctx.guarded(() => setButtons('two')) },
  ]);

  /** Today's counts of the selected deck (the tree's own numbers), or of all decks. */
  function findNode(nodes: DeckTreeNode[], id: string): DeckTreeNode | null {
    for (const n of nodes) {
      if (n.deck.id === id) return n;
      const hit = findNode(n.children, id);
      if (hit) return hit;
    }
    return null;
  }
  const today = $derived.by((): Pick<DeckCounts, 'due' | 'learning' | 'new'> | null => {
    if (ctx.selectedDeck) return findNode(ctx.decks, ctx.selectedDeck)?.counts ?? null;
    if (ctx.decks.length === 0) return null;
    return ctx.decks.reduce(
      (a, n) => ({ due: a.due + n.counts.due, learning: a.learning + n.counts.learning, new: a.new + n.counts.new }),
      { due: 0, learning: 0, new: 0 },
    );
  });

  /** Flags as a menu in the quiet tools row (also Ctrl+1–7); a set flag is ticked. */
  const flagItems = $derived<MenuItem[]>(
    flagDefs.map((f) => ({
      id: `flag-${f.flag}`,
      label: (card && hasFlag(card.flags, f.flag) ? '✓ ' : '') + (f.name ?? td(defaultFlagKey(f.flag))),
      keys: ['Mod', String(f.flag)],
      onselect: () => void run({ kind: 'flag', flag: f.flag }),
    })),
  );

  const minutes = (ms: number) => Math.floor(ms / 60_000);
  const seconds = (ms: number) => String(Math.floor((ms % 60_000) / 1000)).padStart(2, '0');
</script>

<svelte:window onkeydown={onKey} />

{#snippet counts(c: { due: number; learning: number; new: number })}
  <span class="cnt"><span class="k">{t('study.count.due')}</span><span class="num v">{formatCount(c.due)}</span></span>
  <span class="cnt"><span class="k">{t('study.count.learning')}</span><span class="num v">{formatCount(c.learning)}</span></span>
  <span class="cnt"><span class="k">{t('study.count.new')}</span><span class="num v">{formatCount(c.new)}</span></span>
{/snippet}

{#if phase === 'idle'}
  <section class="study">
    <PageHeader title={t('study.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    {#if loading}
      <Skeleton shape="card" height="120px" />
    {:else}
      <SetupPanel title={custom ? t('study.custom_heading') : t('study.start_heading')} {notice}>
        {#snippet meta()}
          {#if custom}
            <span>{t('study.custom_intro')}</span>
          {:else if today}
            {@render counts(today)}
          {:else}
            <span>{t('study.start_intro')}</span>
          {/if}
        {/snippet}
        {#snippet actions()}
          {#if resumable}
            <Button onclick={() => resumable && void ctx.guarded(() => resume(resumable!))}>{t('study.resume')}</Button>
          {/if}
          <Button variant="primary" kbd="Enter" aria-keyshortcuts="Enter" data-testid="study-start" onclick={() => void run({ kind: 'start' })}>
            {custom ? t('study.custom_start') : t('study.start')}
          </Button>
        {/snippet}
        <div class="opts-toggle">
          <Button
            variant="ghost"
            size="sm"
            iconEnd={showOptions ? ChevronUp : ChevronDown}
            aria-expanded={showOptions}
            aria-controls="study-options"
            onclick={() => (showOptions = !showOptions)}
          >
            {t('study.options')}
          </Button>
          {#if practice}<Chip size="sm" tone="accent" label={t('study.practice_badge')} />{/if}
        </div>
        {#if showOptions}
          <div id="study-options">
            <OptRow label={t('study.practice_label')} description={t('study.practice_hint')}>
              <Switch ariaLabel={t('study.practice_label')} bind:checked={practice} />
            </OptRow>
            <StudyModeOptions bind:value={modeOptions} />
            {#if attrs.length > 0}
              <OptRow label={t('study.filter_label')}>
                <Select class="sel" size="sm" label={t('study.filter_label')} hideLabel options={attrOptions} bind:value={filterAttr} />
                {#if filterDef}
                  <Select class="sel" size="sm" label={filterDef.name} hideLabel options={valueOptions} bind:value={filterValue} />
                {/if}
              </OptRow>
            {/if}
            {#if moduleOn('browse')}
              <OptRow label={t('study.query_label')} labelFor="study-query">
                <Input
                  id="study-query"
                  class="query"
                  size="sm"
                  label={t('study.query_label')}
                  hideLabel
                  bind:value={customQuery}
                  disabled={savedId !== ''}
                  placeholder={t('study.query_placeholder')}
                  spellcheck="false"
                />
                {#if savedQueries.length > 0}
                  <Select class="sel" size="sm" label={t('study.saved_label')} hideLabel options={savedOptions} bind:value={savedId} />
                {/if}
              </OptRow>
            {/if}
            <OptRow label={t('study.postpone_label')} labelFor="study-postpone">
              <Input
                id="study-postpone"
                class="days"
                size="sm"
                type="number"
                min="1"
                max="365"
                label={t('study.postpone_label')}
                hideLabel
                bind:value={postponeDays}
              />
              <span class="unit">{t('study.postpone_days')}</span>
              <Button size="sm" onclick={() => void postpone()}>{t('study.postpone')}</Button>
            </OptRow>
          </div>
        {/if}
      </SetupPanel>
    {/if}
  </section>
{:else if phase === 'done' && summary}
  <section class="summary" role="status">
    <header class="sum-head">
      <ProgressRing label={t('study.summary.accuracy_label')} value={accuracy} size={56} stroke={5} tone="success">
        <span class="ring-value num">{formatPercent100(accuracy)}</span>
      </ProgressRing>
      <div>
        <h1>{t('study.summary.heading')}</h1>
        <p class="muted">{t('study.summary.accuracy', { correct: summary.correct, total: summary.answered })}</p>
      </div>
    </header>
    <dl class="stats">
      <div><dt>{t('study.summary.cards')}</dt><dd class="num" data-testid="summary-answered">{formatCount(summary.answered)}</dd></div>
      <div><dt>{t('study.summary.mistakes_label')}</dt><dd class="num" data-testid="summary-mistakes">{formatCount(summary.mistakes)}</dd></div>
      <div><dt>{t('study.summary.accuracy_label')}</dt><dd class="num">{formatPercent100(accuracy)}</dd></div>
      <div>
        <dt>{t('study.summary.time_label')}</dt>
        <dd class="num">{t('study.summary.time_value', { min: minutes(summary.time_ms), sec: seconds(summary.time_ms) })}</dd>
      </div>
    </dl>
    <div class="sum-actions">
      <Button variant="primary" kbd="Enter" aria-keyshortcuts="Enter" onclick={() => void run({ kind: 'back' })}>
        {t('study.summary.back')}
      </Button>
      {#if summary.mistakes > 0}
        <Button icon={Repeat} kbd="R" aria-keyshortcuts="R" onclick={() => void run({ kind: 'repeat' })}>
          {t('study.summary.repeat')}
        </Button>
      {/if}
    </div>
  </section>
{:else if session && card}
  <SessionFrame
    title={t('study.heading')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('study.progress_label')}
    value={position - 1}
    max={total}
    count={t('study.position', { n: position, total })}
    countTestid="study-count"
    exitLabel={t('study.end_label')}
    exitText={t('study.end')}
    exitTestid="study-end"
    onexit={() => void run({ kind: 'end' })}
  >
    {#snippet meta()}
      {@render counts({ due: session!.counts.review, learning: session!.counts.learning, new: session!.counts.new })}
    {/snippet}
    {#snippet extra()}
      {#if session!.mode === 'trial'}<Chip size="sm" tone="accent" label={t('study.practice_badge')} />{/if}
      {#each cardFlags as f (f.flag)}
        <span class="flag" style:--flag={flagColor(f)} data-testid="flag">{f.name ?? td(defaultFlagKey(f.flag))}</span>
      {/each}
    {/snippet}

    {#if session.modes?.timer}
      <TimerBar timer={session.modes.timer} onexpire={() => void ctx.guarded(expire)} />
    {/if}

    <Flashcard bind:flipped {onflip} fit={false} class="study-card">
      {#snippet front()}
        <div class="face-body">
          <CardView sides={card.sides} showAnswer={false} interactive={false} testid="question" />
        </div>
      {/snippet}
      {#snippet back()}
        <div class="face-body">
          {#if revealed}
            <CardView sides={card.sides} showAnswer={true} interactive={false} testid="answer" />
          {/if}
        </div>
      {/snippet}
    </Flashcard>
    <PluginToolbar cardId={card.card_id} />
    {#if hint && !revealed}<p class="hint" data-testid="hint">{hint}</p>{/if}
    {#if choice}
      <div class="choices" role="group" aria-label={t('study.choices')}>
        {#each choice.options as option, i (i)}
          <OptionTile
            kbd={String(i + 1)}
            result={revealed && i === choice.correct ? 'right' : revealed && picked === i ? 'miss' : null}
            rightLabel={t('study.choice_right')}
            missLabel={t('study.choice_wrong')}
            disabled={revealed}
            onclick={() => void run({ kind: 'choose', index: i })}
          >{@html option}</OptionTile>
        {/each}
      </div>
    {/if}
    {#if card.type_answer}
      <Input
        bind:element={input}
        bind:value={typed}
        size="lg"
        disabled={revealed}
        label={t('study.type_placeholder')}
        hideLabel
        placeholder={t('study.type_placeholder')}
        onkeydown={onTypedKey}
        class="typed"
      />
    {/if}
    {#if revealed && check}
      <div class="check {check.grade}" role="status">
        <Icon icon={check.grade === 'correct' ? CircleCheck : Repeat} size={16} />
        <strong>{t(`study.grade.${check.grade}`)}</strong>
        <span class="diffs">{#each check.diff as d, i (i)}<span class="diff {d.kind}">{d.text}</span>{/each}</span>
        {#if check.grade !== 'correct'}<span class="expected">{t('study.expected', { text: check.expected })}</span>{/if}
      </div>
    {/if}

    <div class="answer-bar">
      {#if revealed && confidence && session.modes}
        <ConfidenceButtons levels={session.modes.confidence} onrate={(l) => void ctx.guarded(() => rateConfidence(l))} />
      {:else if revealed && twoButtons}
        <div class="answers two" role="group" aria-label={t('study.rate_label')}>
          <button type="button" class="ans again" aria-keyshortcuts="1" onclick={() => void run({ kind: 'rate', rating: 'again' })}>
            <Icon icon={Repeat} size={16} />
            <span class="lbl">{t('study.buttons.still_learning')}</span>
            <kbd class="key" aria-hidden="true">1</kbd>
          </button>
          <button type="button" class="ans good" aria-keyshortcuts="2" onclick={() => void run({ kind: 'rate', rating: 'good' })}>
            <Icon icon={CircleCheck} size={16} />
            <span class="lbl">{t('study.buttons.know_it')}</span>
            <kbd class="key" aria-hidden="true">2</kbd>
          </button>
        </div>
      {:else if revealed}
        <div class="answers four" role="group" aria-label={t('study.rate_label')}>
          {#each RATINGS as r, i (r)}
            <button
              type="button"
              class="ans rating {r}"
              class:suggested={check?.suggested_rating === r}
              aria-keyshortcuts={String(i + 1)}
              onclick={() => void run({ kind: 'rate', rating: r })}
            >
              <span class="lbl">{t(`study.rating.${r}`)}</span>
              <span class="sub"><span class="num ivl">{interval(card.intervals[i]?.interval_ms ?? 0)}</span><kbd class="key" aria-hidden="true">{i + 1}</kbd></span>
            </button>
          {/each}
        </div>
      {:else}
        <button type="button" class="ans reveal" data-testid="flip" aria-keyshortcuts="Space" onclick={() => void run({ kind: 'reveal' })}>
          <span class="lbl">{t('study.show_answer')}</span>
          <kbd class="key" aria-hidden="true">Space</kbd>
        </button>
      {/if}
    </div>

    <div class="tools">
      <div class="tool-btns">
        <Button variant="ghost" size="sm" kbd="H" aria-keyshortcuts="H" disabled={revealed} onclick={() => void run({ kind: 'hint' })}>
          {t('study.hint')}
        </Button>
        <Button variant="ghost" size="sm" icon={Undo2} kbd="Z" aria-keyshortcuts="Z" onclick={() => void run({ kind: 'undo' })}>
          {t('study.undo')}
        </Button>
        {#if moduleOn('flags') && flagItems.length > 0}
          <Menu items={flagItems} icon={Flag} label={t('study.flag_menu')} placement="top" align="start" />
        {/if}
        <FocusToggle />
        <Menu items={toolItems} label={t('study.more')} placement="top" align="start" />
      </div>
      <!-- Only keys no button on screen shows already. -->
      {#if revealed}
        <p class="legend" data-focus-hide><kbd>Space</kbd> {t('study.keyhint.flip')}</p>
      {/if}
    </div>

    <StudyAi cardId={card.card_id} {revealed} typed={card.type_answer ? typed : null} />
    {#if composing}
      <InlineComposer deckId={card.deck_id} guarded={ctx.guarded} onclose={() => (composing = false)} />
    {/if}
  </SessionFrame>
{/if}

<style>
  .study {
    display: flex;
    flex-direction: column;
  }
  .cnt {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
  }
  .cnt .k {
    color: var(--text-2);
  }
  .cnt .v {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .opts-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-top: 1px solid var(--border);
  }
  #study-options :global(.sel) {
    min-width: 160px;
  }
  #study-options :global(.query) {
    width: 240px;
  }
  #study-options :global(.days) {
    width: 72px;
  }
  .unit {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  /* Session. */
  .flag {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .flag::before {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--flag);
  }
  :global(.study-card.fc-flashcard) {
    max-width: none;
    min-height: 240px;
    max-height: min(400px, 50vh);
  }
  .face-body {
    width: 100%;
    max-height: 100%;
    overflow: auto;
  }
  .face-body :global(.card) {
    background: transparent;
    border: none;
    padding: 0;
    min-height: 0;
    font-size: var(--text-card);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .hint {
    margin: 0;
    text-align: center;
    font-family: var(--font-mono);
    font-size: var(--text-lg);
    color: var(--text-2);
    letter-spacing: 0.08em;
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .check {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    font-size: var(--text-sm);
  }
  .check.correct {
    color: var(--success);
  }
  .check.almost {
    color: var(--warning);
  }
  .check.wrong {
    color: var(--danger);
  }
  .diffs {
    color: var(--text-1);
  }
  .expected {
    color: var(--text-2);
  }
  .diff.missing {
    text-decoration: underline;
    color: var(--danger);
  }
  .diff.extra {
    text-decoration: line-through;
    color: var(--danger);
  }
  /* Answer buttons: exactly as wide as the card. */
  .answer-bar {
    min-height: 52px;
    display: flex;
  }
  .answers {
    display: grid;
    gap: var(--space-2);
    width: 100%;
  }
  .two {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .four {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
  .ans {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 52px;
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition:
      border-color var(--motion-fast) var(--ease),
      background var(--motion-fast) var(--ease);
  }
  .ans:hover {
    background: var(--surface-2);
  }
  .ans:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .key {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    font-weight: var(--weight-regular);
    color: var(--text-2);
  }
  .two .key,
  .reveal .key {
    position: absolute;
    right: var(--space-4);
  }
  .again :global(svg) {
    color: var(--warning);
  }
  .again:hover {
    border-color: color-mix(in srgb, var(--warning) 55%, var(--border));
  }
  .good :global(svg) {
    color: var(--success);
  }
  .good:hover {
    border-color: color-mix(in srgb, var(--success) 55%, var(--border));
  }
  .rating {
    flex-direction: column;
    gap: 2px;
  }
  .rating.again .lbl {
    color: var(--danger);
  }
  .rating.hard .lbl {
    color: var(--warning);
  }
  .rating.good .lbl {
    color: var(--success);
  }
  .rating.easy .lbl {
    color: var(--accent-text);
  }
  .rating.again:hover {
    border-color: color-mix(in srgb, var(--danger) 55%, var(--border));
  }
  .rating.hard:hover {
    border-color: color-mix(in srgb, var(--warning) 55%, var(--border));
  }
  .rating.good:hover {
    border-color: color-mix(in srgb, var(--success) 55%, var(--border));
  }
  .rating.easy:hover,
  .rating.suggested {
    border-color: var(--accent-text);
  }
  .rating.suggested {
    box-shadow: inset 0 0 0 1px var(--accent-text);
  }
  .sub {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
  }
  .ivl {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .reveal {
    background: var(--surface-2);
    font-weight: var(--weight-semibold);
  }
  .reveal:hover {
    background: var(--surface-3);
  }
  .tools {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-inline: calc(-1 * var(--space-2));
  }
  .tool-btns {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-wrap: wrap;
  }
  .legend {
    margin: 0 var(--space-2) 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .legend kbd {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  /* Summary: a normal page again (the shell is back). */
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 760px;
  }
  .sum-head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .sum-head h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .ring-value {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  .muted {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .stats div {
    padding: var(--space-3) var(--space-4);
  }
  .stats div + div {
    border-left: 1px solid var(--border);
  }
  .stats dt {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .stats dd {
    margin: var(--space-1) 0 0;
    font-size: var(--text-2xl);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .sum-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  @media (max-width: 720px) {
    .choices,
    .four {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
