<script lang="ts">
  /**
   * Flashcards (spaced repetition, docs/11 §3): a big centred card that flips with Space,
   * answer buttons under it — two by default ("Still learning" = again, "I know it" = good) or
   * the four ratings with their next intervals (`study.answer_buttons`, a display preference
   * stored in settings). The backend runs the session; this screen only shows and reports.
   */
  import { formatPercent100 } from '$lib/format';
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
    Card,
    ChevronDown,
    ChevronUp,
    Chip,
    CircleCheck,
    Flashcard,
    Icon,
    IconButton,
    Input,
    Kbd,
    Menu,
    OptionTile,
    PageHeader,
    ProgressBar,
    ProgressRing,
    Repeat,
    Select,
    Skeleton,
    Switch,
    Undo2,
    X,
    type MenuItem,
  } from '$lib/ui';
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
  // A running session switches the shell to the focus layout (no picker, no tabs).
  // The session's end screen (summary) stays in the same centred column.
  $effect(() => ctx.setFocus?.(phase !== 'idle'));
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

  const minutes = (ms: number) => Math.floor(ms / 60_000);
  const seconds = (ms: number) => String(Math.floor((ms % 60_000) / 1000)).padStart(2, '0');
</script>

<svelte:window onkeydown={onKey} />

<section class="study" class:playing={phase === 'question' || phase === 'answer'}>
  {#if phase === 'idle'}
    <PageHeader title={t('study.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
    {#if loading}
      <div class="start-card"><Skeleton shape="card" height="200px" /></div>
    {:else}
      <Card padding="lg" class="start-card">
        <div class="start">
          <div class="start-text">
            <h2>{custom ? t('study.custom_heading') : t('study.start_heading')}</h2>
            <p>{custom ? t('study.custom_intro') : t('study.start_intro')}</p>
          </div>
          <div class="start-actions">
            {#if resumable}
              <Button size="lg" onclick={() => resumable && void ctx.guarded(() => resume(resumable!))}>
                {t('study.resume')}
              </Button>
            {/if}
            <Button variant="primary" size="lg" data-testid="study-start" onclick={() => void run({ kind: 'start' })}>
              {custom ? t('study.custom_start') : t('study.start')}
              <Kbd keys={['Enter']} class="on-accent" />
            </Button>
          </div>
        </div>
        {#if notice}
          <p class="notice" role="status"><span class="dot" aria-hidden="true"></span>{notice}</p>
        {/if}
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
          <div id="study-options" class="options">
            <Switch label={t('study.practice_label')} description={t('study.practice_hint')} bind:checked={practice} />
            <StudyModeOptions bind:value={modeOptions} />
            {#if attrs.length > 0}
              <div class="row">
                <Select label={t('study.filter_label')} options={attrOptions} bind:value={filterAttr} />
                {#if filterDef}
                  <Select label={filterDef.name} options={valueOptions} bind:value={filterValue} />
                {/if}
              </div>
            {/if}
            {#if moduleOn('browse')}
              <div class="row">
                <Input
                  class="grow"
                  label={t('study.query_label')}
                  bind:value={customQuery}
                  disabled={savedId !== ''}
                  placeholder={t('study.query_placeholder')}
                  spellcheck="false"
                />
                {#if savedQueries.length > 0}
                  <Select label={t('study.saved_label')} options={savedOptions} bind:value={savedId} />
                {/if}
              </div>
            {/if}
            <div class="row postpone">
              <Input
                class="days"
                type="number"
                min="1"
                max="365"
                label={t('study.postpone_label')}
                hint={t('study.postpone_days')}
                bind:value={postponeDays}
              />
              <Button onclick={() => void postpone()}>{t('study.postpone')}</Button>
            </div>
          </div>
        {/if}
      </Card>
    {/if}
  {:else if phase === 'done' && summary}
    <div class="summary" role="status">
      <Card padding="lg" elevated>
        <div class="summary-head">
          <ProgressRing label={t('study.summary.accuracy_label')} value={accuracy} size={112} tone="success">
            <span class="ring-value">{formatPercent100(accuracy)}</span>
          </ProgressRing>
          <div>
            <h2>{t('study.summary.heading')}</h2>
            <p class="muted">{t('study.summary.accuracy', { n: formatPercent100(accuracy) })}</p>
          </div>
        </div>
        <dl class="stats">
          <div><dt>{t('study.summary.cards')}</dt><dd data-testid="summary-answered">{summary.answered}</dd></div>
          <div><dt>{t('study.summary.mistakes_label')}</dt><dd data-testid="summary-mistakes">{summary.mistakes}</dd></div>
          <div>
            <dt>{t('study.summary.time_label')}</dt>
            <dd>{t('study.summary.time_value', { min: minutes(summary.time_ms), sec: seconds(summary.time_ms) })}</dd>
          </div>
        </dl>
        <div class="summary-actions">
          {#if summary.mistakes > 0}
            <Button icon={Repeat} onclick={() => void run({ kind: 'repeat' })}>
              {t('study.summary.repeat')} <Kbd keys={['R']} />
            </Button>
          {/if}
          <Button variant="primary" onclick={() => void run({ kind: 'back' })}>
            {t('study.summary.back')} <Kbd keys={['Enter']} class="on-accent" />
          </Button>
        </div>
      </Card>
    </div>
  {:else if session && card}
    <div class="topbar">
      <div class="progress" data-focus-hide>
        <ProgressBar label={t('study.progress_label')} value={position - 1} max={total} size="sm" />
        <span class="count" data-testid="study-count">{t('study.position', { n: position, total })}</span>
      </div>
      <div class="badges">
        {#if session.mode === 'trial'}<Chip size="sm" tone="accent" label={t('study.practice_badge')} />{/if}
        {#each cardFlags as f (f.flag)}
          <span class="flag" style:--flag={flagColor(f)} data-testid="flag">{f.name ?? td(defaultFlagKey(f.flag))}</span>
        {/each}
      </div>
      <IconButton icon={X} label={t('study.end_label')} onclick={() => void run({ kind: 'end' })} data-testid="study-end" />
    </div>
    <p class="remaining" data-focus-hide>{t('study.remaining', session.counts)}</p>
    {#if session.modes?.timer}
      <TimerBar timer={session.modes.timer} onexpire={() => void ctx.guarded(expire)} />
    {/if}

    <div class="stage">
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
        <div class="typed">
          <Input
            bind:element={input}
            bind:value={typed}
            size="lg"
            disabled={revealed}
            label={t('study.type_placeholder')}
            hideLabel
            placeholder={t('study.type_placeholder')}
            onkeydown={onTypedKey}
          />
        </div>
      {/if}
      {#if revealed && check}
        <div class="check {check.grade}" role="status">
          <strong>{t(`study.grade.${check.grade}`)}</strong>
          <span class="diffs">{#each check.diff as d, i (i)}<span class="diff {d.kind}">{d.text}</span>{/each}</span>
          {#if check.grade !== 'correct'}<span class="expected">{t('study.expected', { text: check.expected })}</span>{/if}
        </div>
      {/if}
    </div>

    <StudyAi cardId={card.card_id} {revealed} typed={card.type_answer ? typed : null} />

    <div class="answer-bar">
      {#if revealed && confidence && session.modes}
        <ConfidenceButtons levels={session.modes.confidence} onrate={(l) => void ctx.guarded(() => rateConfidence(l))} />
      {:else if revealed && twoButtons}
        <div class="two" role="group" aria-label={t('study.rate_label')}>
          <button type="button" class="big again" onclick={() => void run({ kind: 'rate', rating: 'again' })}>
            <Icon icon={Repeat} size={20} />
            <span class="big-label">{t('study.buttons.still_learning')}</span>
            <Kbd keys={['1']} />
          </button>
          <button type="button" class="big good" onclick={() => void run({ kind: 'rate', rating: 'good' })}>
            <Icon icon={CircleCheck} size={20} />
            <span class="big-label">{t('study.buttons.know_it')}</span>
            <Kbd keys={['2']} />
          </button>
        </div>
      {:else if revealed}
        <div class="ratings" role="group" aria-label={t('study.rate_label')}>
          {#each RATINGS as r, i (r)}
            <button
              type="button"
              class="rating {r}"
              class:suggested={check?.suggested_rating === r}
              onclick={() => void run({ kind: 'rate', rating: r })}
            >
              <span class="r-name">{t(`study.rating.${r}`)}</span>
              <span class="r-meta"><span>{interval(card.intervals[i]?.interval_ms ?? 0)}</span><Kbd keys={[String(i + 1)]} /></span>
            </button>
          {/each}
        </div>
      {:else}
        <div class="reveal-row">
          <Button variant="primary" size="lg" data-testid="flip" onclick={() => void run({ kind: 'reveal' })}>
            {t('study.show_answer')} <Kbd keys={['Space']} class="on-accent" />
          </Button>
        </div>
      {/if}
    </div>

    <div class="tools">
      <Button variant="ghost" size="sm" disabled={revealed} onclick={() => void run({ kind: 'hint' })}>
        {t('study.hint')} <Kbd keys={['H']} />
      </Button>
      <Button variant="ghost" size="sm" icon={Undo2} onclick={() => void run({ kind: 'undo' })}>
        {t('study.undo')} <Kbd keys={['Z']} />
      </Button>
      <FocusToggle />
      <Menu items={toolItems} label={t('study.more')} placement="top" />
    </div>
    {#if composing}
      <InlineComposer deckId={card.deck_id} guarded={ctx.guarded} onclose={() => (composing = false)} />
    {/if}
    <!-- Only keys no button on screen shows already (the reveal and rating buttons carry theirs). -->
    <p class="keys" data-focus-hide>
      {#if revealed}<span><Kbd keys={['Space']} /> {t('study.keyhint.flip')}</span>{/if}
      <span><Kbd keys={['Esc']} /> {t('study.end')}</span>
    </p>
  {/if}
</section>

<style>
  .study {
    max-width: 880px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  /* Running session: one centred column as wide as the card; the answer bar sits under it. */
  .study.playing {
    max-width: 720px;
    margin-inline: auto;
    gap: var(--space-3);
  }
  :global(.start-card) {
    max-width: 720px;
  }
  .start {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    flex-wrap: wrap;
  }
  .start-text h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .start-text p {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    max-width: 44ch;
  }
  .start-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  :global(.on-accent kbd) {
    background: transparent;
    border-color: color-mix(in srgb, var(--accent-fg) 45%, transparent);
    color: var(--accent-fg);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-4) 0 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .notice .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .opts-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-6);
    margin-left: calc(-1 * var(--space-2));
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }
  .row {
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .row :global(.grow) {
    flex: 1;
    min-width: 220px;
  }
  .row :global(.days) {
    width: 200px;
  }
  /* Session. */
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .progress {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .progress :global(.fc-progress) {
    flex: 1;
  }
  .count {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .badges {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .flag {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .flag::before {
    content: '';
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--flag);
  }
  .remaining {
    margin: calc(-1 * var(--space-2)) 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .stage {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .stage :global(.study-card) {
    min-height: 260px;
    max-height: 380px;
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
    font-size: var(--text-2xl);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .hint {
    margin: 0;
    text-align: center;
    font-family: var(--font-mono);
    color: var(--text-2);
    letter-spacing: 0.08em;
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .typed {
    max-width: 560px;
    width: 100%;
    margin: 0 auto;
  }
  .check {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: baseline;
    justify-content: center;
    font-size: var(--text-sm);
  }
  .check.correct strong {
    color: var(--success);
  }
  .check.almost strong {
    color: var(--warning);
  }
  .check.wrong strong {
    color: var(--danger);
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
  .answer-bar {
    min-height: 72px;
    display: flex;
    justify-content: center;
    align-items: flex-start;
  }
  .reveal-row {
    display: flex;
    justify-content: center;
  }
  .two {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
    width: 100%;
    max-width: 560px;
  }
  .big,
  .rating {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: 56px;
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-weight: var(--weight-semibold);
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background var(--motion-fast) var(--ease);
  }
  .big:focus-visible,
  .rating:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .big.again {
    color: var(--warning);
    border-color: color-mix(in srgb, var(--warning) 45%, var(--border));
  }
  .big.again:hover {
    background: var(--warning-soft);
  }
  .big.good {
    color: var(--success);
    border-color: color-mix(in srgb, var(--success) 45%, var(--border));
  }
  .big.good:hover {
    background: var(--success-soft);
  }
  .big-label {
    color: var(--text-1);
  }
  .ratings {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-2);
    width: 100%;
  }
  .rating {
    flex-direction: column;
    gap: 2px;
  }
  .rating:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .rating.suggested {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .r-meta {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
    color: var(--text-2);
  }
  .tools {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }
  .keys {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: var(--space-4);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .keys span {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  /* Summary. */
  .summary {
    max-width: 560px;
    width: 100%;
    /* The end screen sits centred in the focus column. */
    margin: var(--space-12) auto 0;
  }
  .summary-head {
    display: flex;
    align-items: center;
    gap: var(--space-6);
  }
  .summary-head h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
  }
  .ring-value {
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-3);
    margin: var(--space-6) 0;
  }
  .stats div {
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--surface-2);
  }
  .stats dt {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .stats dd {
    margin: var(--space-1) 0 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
  }
  .summary-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  @media (max-width: 720px) {
    .choices,
    .ratings {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
