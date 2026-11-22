<script lang="ts">
  /**
   * Flashcards (spaced repetition, docs/11 §3). v1.3: sessions start in the study launcher
   * (one dialog for every entry); this screen runs them. The running session owns the window
   * (`SessionFrame`: progress, "12 / 40", remaining, Esc) with the card centred (max 720) and
   * answer buttons exactly as wide as it — two by default ("Still learning" = again, "I know
   * it" = good) or the four ratings with their next intervals (`study.answer_buttons`).
   * Keys (keys.ts): Space flips front ⇄ back as often as you like and never grades; the rating
   * keys grade once the back has been seen; H hint, Z undo, Esc leave; Enter only presses the
   * focused control. When every remaining card waits for its learning step the session stays
   * open and says when the next one comes. The backend runs the session; this screen only
   * shows and reports.
   */
  import { formatCount, formatPercent100 } from '$lib/format';
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { AnswerCheckDto, FlagDto, Rating, SessionDto, SessionSummary } from '$lib/api/types';
  import { ApiError } from '$lib/api';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Chip,
    CircleCheck,
    Clock,
    Flag,
    Flashcard,
    Icon,
    Input,
    Menu,
    OptionTile,
    PageHeader,
    Play,
    ProgressRing,
    Repeat,
    Skeleton,
    Undo2,
    type MenuItem,
  } from '$lib/ui';
  import StudyAi from '../ai/StudyAi.svelte';
  import CardView from '../cards/CardView.svelte';
  import PluginToolbar from '../plugins/PluginToolbar.svelte';
  import { defaultFlagKey, flagColor, hasFlag } from '../cards/flags';
  import { moduleOn, trackModules } from '../cards/modules.svelte';
  import type { ScreenCtx } from '../types';
  import { intervalLabel, tzOffsetMins } from './format';
  // v0.10 study modes: timer, confidence scale, focus, inline cards.
  import ConfidenceButtons from '../modes/ConfidenceButtons.svelte';
  import FocusToggle from '../modes/FocusToggle.svelte';
  import InlineComposer from '../modes/InlineComposer.svelte';
  import SessionFrame from '../modes/SessionFrame.svelte';
  import TimerBar from '../modes/TimerBar.svelte';
  import { isEditableTarget } from '$lib/keyboard';
  import { plainKey } from '../modes/modes';
  import type { HintDto, StudyScope } from './contract';
  import { ariaKeys, keyAction, keyFor, RATINGS, studyBindings, type StudyAction, type StudyPhase } from './keys';
  import { handoffSeq, launcher, openLauncher, takeHandoff, type Handoff } from './launcher.svelte';
  import LaunchIdle from './LaunchIdle.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  /** v1.1 display preference (`study.answer_buttons`): two or four answer buttons. */
  const BUTTONS_KEY = 'study.answer_buttons';

  let session = $state<SessionDto | null>(null);
  let summary = $state<SessionSummary | null>(null);
  /** The scope the session was started with (the summary's "Study again" builds on it). */
  let startedScope = $state<StudyScope | null>(null);
  /** The back has been seen at least once (the rating keys work from then on). */
  let revealed = $state(false);
  /** Which face the card shows (Space flips it back and forth). */
  let flipped = $state(false);
  let hint = $state<HintDto | null>(null);
  let typed = $state('');
  let check = $state<AnswerCheckDto | null>(null);
  let composing = $state(false);
  let buttons = $state<'two' | 'four'>('two');
  let busy = false;
  let shownAt = 0;
  let revealedAt: number | null = null;
  let input = $state<HTMLInputElement | null>(null);
  let doneBtn = $state<HTMLButtonElement | null>(null);
  let waitBtn = $state<HTMLButtonElement | null>(null);
  /** Flag names/colors (`flags.list`, Anki numbering from `card_flag_defs`). */
  let flagDefs = $state<FlagDto[]>([]);
  /** Picked option of a choice card. */
  let picked = $state<number | null>(null);
  /** Wall clock for the waiting countdown. */
  let now = $state(Date.now());
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
  const cardFlags = $derived(card && moduleOn('flags') ? flagDefs.filter((f) => hasFlag(card.flags, f.flag)) : []);
  /** Open, nothing to show now, a learning card due later: "next card in N min". */
  const waiting = $derived(!summary && !!session && !card && session.ended_at === null && session.next_due !== null);
  const phase = $derived<StudyPhase>(summary ? 'done' : card ? (revealed ? 'answer' : 'question') : waiting ? 'waiting' : 'idle');
  // A running session owns the window (v1.2: no sidebar, no top bar); the summary is a
  // normal page again, so the learner can go anywhere from it.
  $effect(() => ctx.setFocus?.(phase === 'question' || phase === 'answer' || phase === 'waiting'));
  const twoButtons = $derived(buttons === 'two');

  // The launcher hands a started (or continued) session over: when the screen opens, and
  // whenever it hands over another while this screen is already open ("Study again").
  const initial = takeHandoff('normal');
  let accepting = $state(initial !== null);
  /** A session ran here: back on the empty page the launcher does not pop up again. */
  let hadSession = $state(false);

  function accept(h: Handoff) {
    startedScope = h.scope;
    accepting = true;
    hadSession = true;
    void ctx.guarded(() => settle(h.session)).finally(() => (accepting = false));
  }

  $effect(() => {
    void handoffSeq();
    const h = takeHandoff('normal');
    if (h) accept(h);
  });

  onMount(() => {
    trackModules();
    if (initial) accept(initial);
  });

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

  // Waiting: tick every second; when the next card is due, fetch it.
  $effect(() => {
    if (!waiting) return;
    now = Date.now();
    const id = setInterval(() => {
      now = Date.now();
      if (session?.next_due !== null && session?.next_due !== undefined && now >= session.next_due && !busy) void ctx.guarded(reload);
    }, 1000);
    return () => clearInterval(id);
  });
  $effect(() => {
    if (waiting) void tick().then(() => waitBtn?.focus());
  });
  $effect(() => {
    if (phase === 'done') void tick().then(() => doneBtn?.focus());
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

  /** Shows the session's card; with only waiting cards the session stays open; else it ends. */
  async function settle(s: SessionDto) {
    if (s.current) show(await startTimer(s));
    else if (s.ended_at === null && s.next_due !== null) {
      session = s;
      summary = null;
    } else await finish(s);
  }

  async function finish(s: SessionDto) {
    session = s;
    summary = await api.call('study.end', { session_id: s.id });
  }

  /** "What you did so far" of a session that stays open (`study.summary` does not end it). */
  async function pause(s: SessionDto) {
    session = s;
    summary = await api.call('study.summary', { session_id: s.id });
  }

  /** Back into a paused session, where it was. */
  async function resumePaused() {
    const id = summary?.session_id;
    if (!id) return;
    const s = await api.call('study.session', { session_id: id });
    summary = null;
    await settle(s);
  }

  async function reload() {
    const id = session?.id ?? summary?.session_id;
    if (!id || summary) return;
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

  /** First sight of the back: checks a typed answer, then shows the back. */
  async function reveal() {
    if (card?.type_answer && !check) await checkTyped();
    revealed = true;
    flipped = true;
    revealedAt = Date.now();
    input?.blur();
  }

  /** Space: the first time shows the back (the answer); after that it just turns the card. */
  async function flip() {
    if (!revealed) return reveal();
    flipped = !flipped;
  }

  async function rate(rating: Rating) {
    if (!session || !card || !revealed) return;
    const at = Date.now();
    try {
      const next = await api.call('study.answer', {
        session_id: session.id,
        card_id: card.card_id,
        rating,
        duration_ms: Math.max(0, at - shownAt),
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
    if (!session || !card || !revealed) return;
    const at = Date.now();
    try {
      const next = await api.call('modes.confidence.answer', {
        session_id: session.id,
        card_id: card.card_id,
        level,
        duration_ms: Math.max(0, at - shownAt),
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

  /** Two or four answer buttons: a display preference the backend validates and stores. */
  async function setButtons(v: 'two' | 'four') {
    buttons = v;
    await api.call('settings.set', { scope: { kind: 'app' }, key: BUTTONS_KEY, value: v });
  }

  /** The launcher, on this deck — "Study again" preselects what makes sense after a session. */
  function studyAgain() {
    const deckId = session?.deck_id ?? ctx.selectedDeck;
    const struggled = (summary?.mistakes ?? 0) > 0 || (session?.mistakes ?? 0) > 0;
    openLauncher({ deckId, kind: 'normal', scope: struggled ? 'struggled_today' : (startedScope ?? undefined) });
  }

  async function run(a: StudyAction) {
    if (busy) return;
    busy = true;
    try {
      await ctx.guarded(async () => {
        const s = session;
        const c = card;
        switch (a.kind) {
          case 'flip':
            return flip();
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
            if (s && c && !revealed) {
              const h = await api.call('study.hint', { session_id: s.id, card_id: c.card_id });
              if (h.text || h.html) hint = h;
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
            if (!s) return;
            // Nothing answered yet: no "0 of 0" summary.
            if (s.answered === 0) {
              await api.call('study.end', { session_id: s.id });
              session = null;
              return;
            }
            // v1.3: leaving with cards left is a pause — the session stays open (the launcher
            // offers "Continue" with the same session); only an empty queue ends it.
            if (s.current || s.next_due !== null) return pause(s);
            return finish(s);
          case 'repeat':
            if (summary && summary.mistakes > 0) {
              const r = await api.call('study.start', {
                kind: 'mistakes',
                mode: summary.mode,
                from_session: summary.session_id,
                tz_offset_mins: tzOffsetMins(),
                modes: null,
              });
              await settle(r);
            }
            return;
        }
      });
    } finally {
      busy = false;
    }
  }

  function back() {
    summary = null;
    session = null;
  }

  /** Space on an answer button would press it: here Space only ever flips the card. */
  const ANSWER_BUTTON = '.answer-bar button, .tools .flip-btn';

  function onKey(e: KeyboardEvent) {
    // The card (or another control) handled it already (e.g. Space on the focused card).
    if (e.defaultPrevented) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    const el = e.target as HTMLElement | null;
    const tag = el?.tagName ?? '';
    const typing = isEditableTarget(el);
    const asButton = tag === 'BUTTON' || (el instanceof HTMLElement && el.getAttribute('role') === 'button');
    const flipKey = bindings ? keyFor('study.reveal', bindings) : 'Space';
    if (asButton && e.key === ' ' && flipKey === 'Space' && el?.closest(ANSWER_BUTTON) && (phase === 'question' || phase === 'answer')) {
      e.preventDefault();
      if (!e.repeat) void run({ kind: 'flip' });
      return;
    }
    // Buttons handle their own Enter/Space; keep shortcuts working everywhere else.
    if (asButton && (e.key === 'Enter' || e.key === ' ')) return;
    if (e.repeat) return;
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
    if (e.key === 'Enter' && !e.repeat) {
      e.preventDefault();
      void run({ kind: 'flip' });
    }
  }

  /** The learner turned the card (click, or Space/Enter on the focused card). */
  function onflip(f: boolean) {
    if (f && !revealed) void run({ kind: 'flip' });
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

  // The keys as bound now (the shortcut editor may have changed them): legend, hints, aria.
  const keys = $derived({
    flip: keyFor('study.reveal', bindings),
    again: keyFor('study.again', bindings),
    hard: keyFor('study.hard', bindings),
    good: keyFor('study.good', bindings),
    easy: keyFor('study.easy', bindings),
    hint: keyFor('study.hint', bindings),
    undo: keyFor('study.undo', bindings),
    end: keyFor('study.end', bindings),
    suspend: keyFor('study.suspend', bindings),
    bury: keyFor('study.bury', bindings),
  });
  const rateKeys = $derived(twoButtons ? [keys.again, keys.hard] : [keys.again, keys.hard, keys.good, keys.easy]);
  const keyText = (k: string | null) => (k === 'Escape' ? 'Esc' : (k ?? ''));

  const toolItems = $derived<MenuItem[]>([
    { id: 'suspend', label: `${t('study.suspend')} — ${t('study.suspend_desc')}`, keys: keys.suspend ? [keyText(keys.suspend)] : undefined, onselect: () => void run({ kind: 'suspend' }) },
    { id: 'bury', label: `${t('study.bury')} — ${t('study.bury_desc')}`, keys: keys.bury ? [keyText(keys.bury)] : undefined, onselect: () => void run({ kind: 'bury' }) },
    ...(moduleOn('inline_cards')
      ? [{ id: 'inline', label: t('modes.inline.open'), keys: ['N'], onselect: () => (composing = true) }]
      : []),
    { id: 'sep', separator: true as const },
    twoButtons
      ? { id: 'buttons', label: t('study.buttons.use_four'), onselect: () => void ctx.guarded(() => setButtons('four')) }
      : { id: 'buttons', label: t('study.buttons.use_two'), onselect: () => void ctx.guarded(() => setButtons('two')) },
  ]);

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
  const waitLeft = $derived(session?.next_due ? Math.max(0, session.next_due - now) : 0);
</script>

<svelte:window onkeydown={onKey} />

{#snippet counts(c: { due: number; learning: number; new: number })}
  <span class="cnt" title={t('study.count.due_desc')}><span class="k">{t('study.count.due')}</span><span class="num v">{formatCount(c.due)}</span></span>
  <span class="cnt" title={t('study.count.learning_desc')}><span class="k">{t('study.count.learning')}</span><span class="num v">{formatCount(c.learning)}</span></span>
  <span class="cnt" title={t('study.count.new_desc')}><span class="k">{t('study.count.new')}</span><span class="num v">{formatCount(c.new)}</span></span>
{/snippet}

{#if phase === 'idle'}
  {#if accepting}
    <section class="study">
      <PageHeader title={t('study.heading')} subtitle={ctx.selectedPath ? undefined : t('study.whole_collection')} />
      <Skeleton shape="card" height="120px" />
    </section>
  {:else}
    <LaunchIdle {ctx} kind="normal" title={t('study.heading')} auto={!hadSession} />
  {/if}
{:else if phase === 'done' && summary}
  <section class="summary" role="status">
    <header class="sum-head">
      <ProgressRing label={t('study.summary.accuracy_label')} value={accuracy} size={56} stroke={5} tone="success">
        <span class="ring-value num">{formatPercent100(accuracy)}</span>
      </ProgressRing>
      <div>
        <h1>{summary.ended_at === null ? t('study.summary.paused') : t('study.summary.heading')}</h1>
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
      {#if summary.ended_at === null}
        <Button variant="primary" icon={Play} data-testid="study-continue" onclick={() => void ctx.guarded(resumePaused)}>{t('study.summary.continue')}</Button>
      {/if}
      <Button variant={summary.ended_at === null ? 'secondary' : 'primary'} bind:element={doneBtn} onclick={back}>
        {t('study.summary.back')}
      </Button>
      {#if launcher.available}
        <Button icon={Play} title={t('study.summary.again_desc')} data-testid="study-again" onclick={studyAgain}>
          {t('study.summary.again')}
        </Button>
      {/if}
      {#if summary.mistakes > 0}
        <Button icon={Repeat} kbd="R" aria-keyshortcuts="R" onclick={() => void run({ kind: 'repeat' })}>
          {t('study.summary.repeat')}
        </Button>
      {/if}
    </div>
  </section>
{:else if session && waiting}
  <SessionFrame
    title={t('study.heading')}
    subtitle={ctx.selectedPath ?? t('study.whole_collection')}
    progressLabel={t('study.progress_label')}
    value={session.answered}
    max={total}
    count={t('study.position', { n: session.answered, total })}
    countTestid="study-count"
    exitLabel={t('study.end_label')}
    exitText={t('study.end')}
    exitTestid="study-end"
    onexit={() => void run({ kind: 'end' })}
  >
    {#snippet meta()}
      {@render counts({ due: session!.counts.review, learning: session!.counts.learning, new: session!.counts.new })}
    {/snippet}
    <section class="waiting" data-testid="study-waiting" aria-live="polite">
      <span class="wait-icon"><Icon icon={Clock} size={24} /></span>
      <h2>{waitLeft < 60_000 ? t('study.waiting.soon') : t('study.waiting.title', { n: Math.ceil(waitLeft / 60_000) })}</h2>
      <p class="wait-clock num" role="timer" aria-label={t('study.waiting.countdown_label')}>{minutes(waitLeft)}:{seconds(waitLeft)}</p>
      <p class="wait-body">{t('study.waiting.body')}</p>
      <div class="wait-actions">
        <Button variant="primary" bind:element={waitBtn} data-testid="study-wait" onclick={() => void ctx.guarded(reload)}>{t('study.waiting.wait')}</Button>
        {#if launcher.available}
          <Button icon={Play} onclick={() => openLauncher({ deckId: session?.deck_id ?? ctx.selectedDeck })}>{t('study.waiting.other')}</Button>
        {/if}
        <Button data-testid="study-finish" onclick={() => session && void ctx.guarded(() => finish(session!))}>{t('study.waiting.finish')}</Button>
      </div>
    </section>
  </SessionFrame>
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
      {#if session!.mode === 'trial'}<span title={t('study.practice_hint')}><Chip size="sm" tone="accent" label={t('study.practice_badge')} /></span>{/if}
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
    {#if hint && !revealed}
      {#if hint.kind === 'context' && hint.html}
        <p class="hint context" data-testid="hint"><span class="hint-label">{t('study.hint_context')}</span><span class="hint-sentence">{@html hint.html}</span></p>
      {:else}
        <p class="hint" data-testid="hint">{hint.text}</p>
      {/if}
    {/if}
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
          <button type="button" class="ans again" aria-keyshortcuts={ariaKeys(keys.again)} data-testid="rate-again" onclick={() => void run({ kind: 'rate', rating: 'again' })}>
            <Icon icon={Repeat} size={16} />
            <span class="lbl">{t('study.buttons.still_learning')}</span>
            {#if keys.again}<kbd class="key" aria-hidden="true">{keyText(keys.again)}</kbd>{/if}
          </button>
          <button type="button" class="ans good" aria-keyshortcuts={ariaKeys(keys.hard)} data-testid="rate-good" onclick={() => void run({ kind: 'rate', rating: 'good' })}>
            <Icon icon={CircleCheck} size={16} />
            <span class="lbl">{t('study.buttons.know_it')}</span>
            {#if keys.hard}<kbd class="key" aria-hidden="true">{keyText(keys.hard)}</kbd>{/if}
          </button>
        </div>
      {:else if revealed}
        <div class="answers four" role="group" aria-label={t('study.rate_label')}>
          {#each RATINGS as r, i (r)}
            <button
              type="button"
              class="ans rating {r}"
              class:suggested={check?.suggested_rating === r}
              aria-keyshortcuts={ariaKeys(rateKeys[i] ?? null)}
              data-testid="rate-{r}"
              onclick={() => void run({ kind: 'rate', rating: r })}
            >
              <span class="lbl">{t(`study.rating.${r}`)}</span>
              <span class="sub"><span class="num ivl">{interval(card.intervals[i]?.interval_ms ?? 0)}</span>{#if rateKeys[i]}<kbd class="key" aria-hidden="true">{keyText(rateKeys[i] ?? null)}</kbd>{/if}</span>
            </button>
          {/each}
        </div>
      {:else}
        <button type="button" class="ans reveal" data-testid="flip" aria-keyshortcuts={ariaKeys(keys.flip)} onclick={() => void run({ kind: 'flip' })}>
          <span class="lbl">{t('study.show_answer')}</span>
          {#if keys.flip}<kbd class="key" aria-hidden="true">{keyText(keys.flip)}</kbd>{/if}
        </button>
      {/if}
    </div>

    <div class="tools">
      <div class="tool-btns">
        {#if revealed}
          <Button variant="ghost" size="sm" class="flip-btn" kbd={keys.flip ? keyText(keys.flip) : undefined} aria-keyshortcuts={ariaKeys(keys.flip)} data-testid="flip-again" onclick={() => void run({ kind: 'flip' })}>
            {t('study.flip')}
          </Button>
        {/if}
        <Button variant="ghost" size="sm" kbd={keys.hint ? keyText(keys.hint) : undefined} aria-keyshortcuts={ariaKeys(keys.hint)} disabled={revealed} onclick={() => void run({ kind: 'hint' })}>
          {t('study.hint')}
        </Button>
        <Button variant="ghost" size="sm" icon={Undo2} kbd={keys.undo ? keyText(keys.undo) : undefined} aria-keyshortcuts={ariaKeys(keys.undo)} onclick={() => void run({ kind: 'undo' })}>
          {t('study.undo')}
        </Button>
        {#if moduleOn('flags') && flagItems.length > 0}
          <Menu items={flagItems} icon={Flag} label={t('study.flag_menu')} placement="top" align="start" />
        {/if}
        <FocusToggle />
        <Menu items={toolItems} label={t('study.more')} placement="top" align="start" />
      </div>
      <!-- What the keys do right now (they follow the shortcut editor). -->
      <p class="legend" data-focus-hide data-testid="study-legend">
        {#if keys.flip}<span><kbd>{keyText(keys.flip)}</kbd> {t('study.legend.flip')}</span>{/if}
        {#if revealed && !confidence && rateKeys.some(Boolean)}<span><kbd>{rateKeys.filter(Boolean).map(keyText).join(' ')}</kbd> {t('study.legend.rate')}</span>{/if}
        {#if keys.end}<span><kbd>{keyText(keys.end)}</kbd> {t('study.legend.leave')}</span>{/if}
      </p>
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
    /* The start page is one column: the header (deck picker) lines up with the panel. */
    max-width: 760px;
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
  .wait-icon {
    display: inline-flex;
    color: var(--accent-text);
  }
  /* Waiting for a learning step: the session stays open. */
  .waiting {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-8) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-flashcard);
    background: var(--surface-1);
    text-align: center;
  }
  .waiting h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .wait-clock {
    margin: 0;
    font-size: var(--text-2xl);
    color: var(--text-1);
  }
  .wait-body {
    margin: 0;
    max-width: 52ch;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .wait-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    justify-content: center;
  }
  /* Card back of the built-in vocabulary types (CONTRACT §2): the meaning is the answer, the
     example a separate labelled block (sentence, then its translation, muted), the source a
     small caption. Only presentation; the parts come from the card template. */
  .face-body :global(.fc-part-meaning) {
    font-size: var(--text-card);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .face-body :global(.fc-part-example) {
    margin: var(--space-4) auto 0;
    max-width: 56ch;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-2);
    font-size: var(--text-md);
    font-weight: var(--weight-regular, 400);
    line-height: var(--leading);
    text-align: start;
    color: var(--text-1);
  }
  .face-body :global(.fc-part-example .fc-part-label),
  .face-body :global(.fc-part-source .fc-part-label) {
    display: block;
    margin-bottom: var(--space-1);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .face-body :global(.fc-example-translation),
  .face-body :global(.fc-part-example .fc-part-translation) {
    display: block;
    margin-top: var(--space-1);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .face-body :global(.fc-part-source) {
    /* The study face sets its own muted colour: no extra dimming from the card CSS (light theme). */
    opacity: 1;
    margin-top: var(--space-3);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular, 400);
    line-height: var(--leading);
    color: var(--text-2);
  }
  .face-body :global(.fc-part-source .fc-part-label) {
    display: inline;
    margin: 0 var(--space-1) 0 0;
  }
  /* Context hint: the example sentence with a visible blank. */
  .hint.context {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-family: inherit;
    letter-spacing: normal;
  }
  .hint-label {
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  .hint-sentence {
    font-size: var(--text-lg);
    color: var(--text-1);
  }
  /* v1.3 X1: a card asking for the meaning shows its word highlighted in the sentence. */
  .hint-sentence :global(.fc-context-word) {
    color: var(--accent-text);
    font-weight: var(--weight-semibold);
  }
  .hint-sentence :global(.fc-blank),
  .hint-sentence :global(mark),
  .hint-sentence :global(u) {
    display: inline-block;
    min-width: 3em;
    border-bottom: 2px solid var(--accent-text);
    background: none;
    color: transparent;
  }
  .legend {
    display: inline-flex;
    gap: var(--space-3);
    flex-wrap: wrap;
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
