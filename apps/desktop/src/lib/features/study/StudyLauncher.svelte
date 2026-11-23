<!--
  The study launcher ("Çalış"): one keyboard-first dialog for "I want to study".
  v1.4 (prototype `prototypes/v14/modlar.html`): left, "How do you want to study?" — every mode
  `study.options.modes` lists (Cards, Test, Learn, Exam, Match; only the ones switched on) with
  its icon, its name, ONE line on what it does and whether it changes the schedule ("takvimi
  etkiler" / "pratik"), all from the API. Right, "Which cards?" — today's cards by default, the
  other four scopes one click away (their exact counts; an empty one is disabled and says why);
  the Exam's own choices (question types, how many) when the Exam is picked; and, behind the ONE
  "Gelişmiş" switch, everything else the v1.3 options had (count, practice, time limit,
  confidence scale, filters, saved searches, postpone). The footer says what Start will do:
  mode · cards · how many, and what it does to the schedule. Unfinished sessions sit on top
  (first and focused). Enter starts (unless focus is on another button), Esc closes. Start
  re-validates: when the backend says there is nothing to study the dialog stays open with the
  reason.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { api, ApiError } from '$lib/api';
  import type { AttributeDefDto, SavedQueryDto, SessionDto, StartModes, StudyModeOption, TestQuestionKind } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Checkbox,
    ChevronDown,
    ChevronUp,
    ClipboardCheck,
    Dialog,
    FileText,
    GraduationCap,
    Icon,
    Info,
    Input,
    Layers,
    LayoutGrid,
    OptionTile,
    Play,
    Select,
    Skeleton,
    Switch,
  } from '$lib/ui';
  import OptRow from '../modes/OptRow.svelte';
  import StudyModeOptions from '../modes/StudyModeOptions.svelte';
  import { moduleKnownOn, moduleOn } from '../cards/modules.svelte';
  import { tzOffsetMins } from './format';
  import { nothingToStudy, PRACTICE_SCOPES, startStudy, studyOptions, type LaunchKind, type ResumableSession, type StudyOptions, type StudyScope } from './contract';
  import {
    emptyReasonKey,
    isCustom,
    loadAdvanced,
    loadPrefs,
    minutesUntil,
    pickKind,
    pickScope,
    plannedCount,
    rememberAdvanced,
    rememberChoice,
    SCOPES_PROGRESSIVE,
    startRequest,
    type LaunchChoice,
  } from './launch';
  import { launchKindOf, type Handoff, type LaunchRequest } from './launcher.svelte';

  interface Props {
    open?: boolean;
    request: LaunchRequest;
    /** A session was started or picked to continue: the host takes it to its screen. */
    onstart: (h: Handoff) => void;
  }
  let { open = $bindable(false), request, onstart }: Props = $props();

  const MODE_ICON = { normal: Layers, quiz: ClipboardCheck, learn: GraduationCap, test: FileText, match: LayoutGrid } as const;

  let options = $state<StudyOptions | null>(null);
  let failed = $state<string | null>(null);
  let kind = $state<LaunchKind | null>(null);
  let scope = $state<StudyScope>('due');
  let limit = $state<number | string>('');
  /** The one "Gelişmiş" switch: the rarely needed options (remembered). */
  let advanced = $state(false);
  /** Progressive scopes: today's cards only, the other four one click away. */
  let moreScopes = $state(false);
  let notice = $state<string | null>(null);
  let starting = $state(false);
  // Options of the old setup panels (no option is lost).
  let practice = $state(false);
  let query = $state('');
  let savedId = $state('');
  let attrId = $state('');
  let attrValue = $state('');
  let modes = $state<StartModes | null>(null);
  /** Exam: question types, from the API's defaults (true/false is off). */
  let testKinds = $state<Partial<Record<TestQuestionKind, boolean>>>({});
  let postponeDays = $state<number | string>(1);
  let attrs = $state<AttributeDefDto[]>([]);
  let saved = $state<SavedQueryDto[]>([]);
  let prefs = loadPrefs();
  let now = $state(Date.now());
  let root = $state<HTMLDivElement | null>(null);

  const deckId = $derived(request.deckId);

  async function load(preselect: boolean) {
    failed = null;
    try {
      const o = await studyOptions({ deck_id: deckId, include_subdecks: null });
      options = o;
      const exam = o.modes.find((m) => m.kind === 'test');
      if (preselect) {
        testKinds = Object.fromEntries((exam?.question_kinds ?? []).map((k) => [k, exam!.default_question_kinds.includes(k)]));
        const k = pickKind(o, request.kind, prefs, deckId);
        kind = k;
        if (k) scope = pickScope(o.scopes, k, request.scope, prefs);
        moreScopes = !SCOPES_PROGRESSIVE || scope !== 'due';
        // The dialog focused its first control while loading: move to the real first stop —
        // "Continue" when something is unfinished, else the chosen mode. Opened for one mode
        // (the deck page's Learn / Test / Match): only an unfinished session OF THAT MODE comes
        // first — Enter never resumes a Test when Learn was asked for.
        await tick();
        const resumeSel = request.kind ? `[data-resume][data-kind="${request.kind}"]` : '[data-resume]';
        (root?.querySelector<HTMLElement>(resumeSel) ?? root?.querySelector<HTMLElement>('[data-mode][aria-checked="true"]'))?.focus();
      }
      now = Date.now();
    } catch (e) {
      failed = e instanceof ApiError ? td(e.messageKey) : t('common.load_failed');
    }
  }

  function loadExtras() {
    attrs = [];
    saved = [];
    if (moduleKnownOn('attributes') && deckId) {
      api.call('attributes.effective', { deck_id: deckId, target: 'note' }).then(
        (r) => (attrs = r.attributes.map((a) => a.def).filter((d) => d.kind === 'enum' || d.kind === 'ordered_enum' || d.kind === 'multi_enum')),
        () => {},
      );
    }
    if (moduleKnownOn('browse')) {
      api.call('saved_queries.list', {}).then(
        (r) => (saved = r.items),
        () => {},
      );
    }
  }

  // Every opening starts from the opener's request and the remembered choices.
  $effect(() => {
    if (!open) return;
    void request;
    untrack(() => {
      prefs = loadPrefs();
      options = null;
      notice = null;
      limit = '';
      advanced = loadAdvanced() || !!request.query || !!request.practice;
      practice = request.practice ?? false;
      query = request.query ?? '';
      savedId = '';
      attrId = '';
      attrValue = '';
      void load(true);
      loadExtras();
    });
  });

  // A new classification clears its value.
  $effect(() => {
    void attrId;
    attrValue = '';
  });

  // Live "next card in N min" in the scope reasons.
  $effect(() => {
    if (!open) return;
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  const scopes = $derived(options?.scopes ?? []);
  const selected = $derived(scopes.find((s) => s.scope === scope));
  const resumable = $derived(options?.resumable ?? []);
  const deckName = $derived(options?.deck_name ?? t('study.launcher.deck_all'));
  const modeList = $derived((options?.modes ?? []).filter((m): m is StudyModeOption & { kind: LaunchKind } => m.kind !== 'custom' && m.kind !== 'mistakes'));
  const mode = $derived(modeList.find((m) => m.kind === kind) ?? null);
  const shownScopes = $derived(moreScopes ? scopes : scopes.filter((s) => s.scope === 'due' || s.scope === scope));
  const hiddenScopes = $derived(scopes.length - shownScopes.length);
  const examKinds = $derived((mode?.question_kinds ?? []).filter((k) => testKinds[k]));

  const choice = $derived<LaunchChoice | null>(
    kind
      ? {
          kind,
          scope,
          deckId,
          limit: Number(limit) > 0 ? Math.floor(Number(limit)) : null,
          practice: advanced && practice,
          query: advanced && (kind === 'normal' || moduleOn('browse')) ? query : '',
          savedQueryId: advanced && kind === 'normal' ? savedId : '',
          attribute: advanced && kind === 'normal' && attrId !== '' && attrValue !== '' ? { attrId, value: attrValue } : null,
          modes: advanced ? modes : null,
          testKinds: examKinds,
          testQuestions: mode?.default_questions ?? 20,
        }
      : null,
  );
  const custom = $derived(choice ? isCustom(choice) : false);
  const planned = $derived(choice ? plannedCount(choice, selected) : null);
  /** Why Start is off (shown in the footer), or null. */
  const blocked = $derived(kind === 'test' && examKinds.length === 0 ? t('study.launcher.exam_no_kinds') : null);
  const canStart = $derived(!!choice && !blocked && (custom || (planned ?? 0) > 0) && !starting);
  /** The schedule line of the footer: what this start does to "when do I see it again". */
  const schedLine = $derived.by(() => {
    if (!mode) return '';
    if (!mode.affects_schedule) return t('study.launcher.sched.practice_mode');
    if (!custom && selected?.mode === 'trial') return t('study.launcher.sched.practice_scope', { scope: t(`study.launcher.scope.${scope}`) });
    if (choice?.practice) return t('study.launcher.sched.practice_switch');
    return t('study.launcher.sched.affects');
  });
  const summary = $derived(
    `${mode ? td(mode.name_key) : ''} · ${custom ? t('study.launcher.filtered') : t(`study.launcher.scope.${scope}`)}${
      planned === null ? '' : ` · ${kind === 'test' ? t('study.launcher.questions', { count: planned }) : t('study.launcher.cards', { count: planned })}`
    }`,
  );

  function reasonText(s: (typeof scopes)[number]): string {
    if (!s.empty_reason) return '';
    if (s.empty_reason === 'waiting' && s.next_available_at) return t('study.launcher.empty.waiting', { n: minutesUntil(s.next_available_at, now) });
    return t(emptyReasonKey(s.scope, s.empty_reason));
  }

  async function start() {
    if (!choice || !canStart) return;
    starting = true;
    notice = null;
    try {
      const s = await startStudy(startRequest(choice, selected, tzOffsetMins()));
      // Older backends start an empty Cards/Learn session instead of refusing: close it here.
      if ((choice.kind === 'normal' || choice.kind === 'learn') && !s.current && s.next_due === null) {
        await api.call('study.end', { session_id: s.id });
        notice = t('study.launcher.nothing_now', { reason: t('study.launcher.empty.nothing_due') });
        return;
      }
      prefs = rememberChoice(prefs, deckId, choice.kind, custom ? null : choice.scope);
      onstart({ kind: choice.kind, session: s, scope: custom ? null : choice.scope });
      open = false;
    } catch (e) {
      const why = nothingToStudy(e);
      if (why) {
        const reason = why.reason === 'waiting' && why.next_available_at
          ? t('study.launcher.empty.waiting', { n: minutesUntil(why.next_available_at, Date.now()) })
          : t(emptyReasonKey(choice.scope, why.reason));
        notice = t('study.launcher.nothing_now', { reason });
        await load(false);
      } else {
        notice = e instanceof ApiError ? td(e.messageKey) : String(e);
      }
    } finally {
      starting = false;
    }
  }

  function resume(r: ResumableSession) {
    onstart({ kind: launchKindOf(r.session.kind), session: r.session as SessionDto, scope: null });
    open = false;
  }

  async function postpone() {
    notice = null;
    try {
      const r = await api.call('study.postpone', { deck_id: deckId, days: Number(postponeDays) || 1, tz_offset_mins: tzOffsetMins() });
      notice = t('study.postponed', { count: r.cards });
      await load(false);
    } catch (e) {
      notice = e instanceof ApiError ? td(e.messageKey) : String(e);
    }
  }

  // The mode rows and the scope rows are radio groups: one Tab stop each, arrows move inside.
  $effect(() => {
    void scope;
    void kind;
    void shownScopes;
    void custom;
    void tick().then(() => {
      for (const el of root?.querySelectorAll<HTMLElement>('[data-scope] button, [data-mode]') ?? []) {
        el.tabIndex = el.getAttribute('aria-checked') === 'true' ? 0 : -1;
      }
    });
  });

  /** Arrow keys walk the scope rows (one radio group). */
  function onScopeKey(e: KeyboardEvent) {
    const dir = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const enabled = shownScopes.filter((s) => s.count > 0);
    const i = enabled.findIndex((s) => s.scope === scope);
    const next = enabled[(i + dir + enabled.length) % enabled.length];
    if (!next) return;
    scope = next.scope;
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-scope="${next.scope}"] button`)?.focus();
  }

  /** Arrow keys walk the modes (one radio group); Space picks the focused one. */
  function onModeKey(e: KeyboardEvent) {
    const dir = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = modeList.findIndex((m) => m.kind === kind);
    const next = modeList[(i + dir + modeList.length) % modeList.length];
    if (!next) return;
    pickMode(next.kind);
    root?.querySelector<HTMLElement>(`[data-mode="${next.kind}"]`)?.focus();
  }

  /** Enter starts — unless the focus is on a control whose own Enter means something else. */
  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.repeat || e.isComposing || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
    const el = e.target as HTMLElement | null;
    // Buttons with an Enter of their own: continue a session, toggles, menus, pickers.
    if (el?.closest('[data-enter-self], [role=listbox], [role=combobox], [role=menu], [role=switch], [aria-haspopup], input[type=checkbox]')) return;
    e.preventDefault();
    void start();
  }

  // When the mode changes, show its remembered (or default) scope.
  function pickMode(k: LaunchKind) {
    if (kind === k) return;
    kind = k;
    if (options) scope = pickScope(options.scopes, k, undefined, prefs);
    if (scope !== 'due') moreScopes = true;
    notice = null;
  }

  function toggleMore() {
    moreScopes = !moreScopes;
    if (!moreScopes && scope !== 'due' && (scopes.find((s) => s.scope === 'due')?.count ?? 0) > 0) scope = 'due';
  }

  function setAdvanced(v: boolean) {
    advanced = v;
    rememberAdvanced(v);
  }

  const attrOptions = $derived([{ value: '', label: t('study.filter_any') }, ...attrs.map((a) => ({ value: a.id, label: a.name }))]);
  const attrDef = $derived(attrs.find((a) => a.id === attrId) ?? null);
  const valueOptions = $derived([{ value: '', label: t('study.filter_any') }, ...(attrDef?.options ?? []).map((o) => ({ value: o.id, label: o.label }))]);
  const savedOptions = $derived([{ value: '', label: t('study.filter_any') }, ...saved.map((q) => ({ value: q.id, label: q.name }))]);
</script>

<Dialog
  bind:open
  title={deckId ? t('study.launcher.title_deck', { deck: deckName }) : t('study.launcher.title')}
  size="lg"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="launcher" bind:this={root} onkeydown={onKey} data-testid="study-launcher">
    {#if failed}
      <p class="notice error" role="alert">{failed}</p>
    {:else if !options}
      <p class="sr-only" role="status">{t('study.launcher.loading')}</p>
      <Skeleton shape="text" lines={6} />
    {:else if !kind}
      <p class="notice" role="status">{t('study.launcher.no_modes')}</p>
    {:else}
      {#if resumable.length}
        <section class="block" aria-labelledby="launch-resume">
          <h3 id="launch-resume" class="label">{t('study.launcher.resume_heading')}</h3>
          <div class="resume-list">
            {#each resumable as r (r.session.id)}
              {@const k = launchKindOf(r.session.kind)}
              <button type="button" class="resume" data-resume data-kind={k} data-enter-self data-testid="launcher-resume" onclick={() => resume(r)}>
                <Icon icon={Play} size={14} />
                <span class="r-main"><strong>{t('study.launcher.resume')}</strong> · {r.deck_name ?? t('study.launcher.deck_all')} · {t(`study.launcher.kind.${k}`)}</span>
                <span class="r-meta num">{t('study.launcher.resume_meta', { answered: formatCount(r.answered), remaining: formatCount(r.remaining) })}</span>
              </button>
            {/each}
          </div>
        </section>
      {/if}

      <div class="cols">
        <section class="block">
          <span class="label" id="launch-mode"><span class="step num" aria-hidden="true">1</span>{t('study.launcher.mode_label')}</span>
          <div class="modes" role="radiogroup" aria-labelledby="launch-mode" tabindex="-1" onkeydown={onModeKey}>
            {#each modeList as m (m.kind)}
              {@const on = m.kind === kind}
              <button
                type="button"
                class="mode"
                class:on
                role="radio"
                aria-checked={on}
                aria-label={t(`study.launcher.kind.${m.kind}`)}
                aria-describedby="mode-desc-{m.kind}"
                data-mode={m.kind}
                data-testid="launcher-mode-{m.kind}"
                onclick={() => pickMode(m.kind)}
              >
                <span class="m-icon"><Icon icon={MODE_ICON[m.kind]} size={16} /></span>
                <span class="m-name">{td(m.name_key)}</span>
                <span class="m-chip" class:cal={m.affects_schedule}>
                  {#if m.affects_schedule}<Icon icon={Calendar} size={12} />{t('study.launcher.chip.affects')}{:else}{t('study.launcher.chip.practice')}{/if}
                </span>
                <span class="m-line" id="mode-desc-{m.kind}" data-testid={on ? 'launcher-mode-desc' : undefined}>{td(m.description_key)}</span>
              </button>
            {/each}
          </div>
          <p class="legend"><Icon icon={Info} size={14} /><span>{t('study.launcher.legend')}</span></p>
        </section>

        <div class="right">
          <section class="block" class:dim={custom}>
            <span class="label" id="launch-scope"><span class="step num" aria-hidden="true">2</span>{t('study.launcher.scope_label')}</span>
            {#if custom}<p class="custom-note">{t('study.launcher.custom_note')}</p>{/if}
            <!-- svelte-ignore a11y_interactive_supports_focus -->
            <div class="scopes" role="radiogroup" aria-labelledby="launch-scope" onkeydown={onScopeKey}>
              {#each shownScopes as s (s.scope)}
                {@const empty = s.count === 0}
                <span class="scope-cell" data-scope={s.scope} data-testid="launcher-scope-{s.scope}">
                  <OptionTile class="scope-row" checked={scope === s.scope} disabled={empty || custom} onclick={() => (scope = s.scope)}>
                    <span class="s-text">
                      <span class="s-name">
                        {t(`study.launcher.scope.${s.scope}`)}
                        <!-- Only a mode that changes the schedule turns practice in these scopes. -->
                        {#if PRACTICE_SCOPES.includes(s.scope) && mode?.affects_schedule}<span class="tag">{t('study.launcher.practice_tag')}</span>{/if}
                      </span>
                      <span class="s-desc" class:reason={empty}>{empty ? reasonText(s) : t(!deckId && s.scope === 'all' ? 'study.launcher.scope_desc.all_decks' : `study.launcher.scope_desc.${s.scope}`)}</span>
                    </span>
                    <span class="s-count num" class:zero={empty}>{formatCount(s.count)}</span>
                  </OptionTile>
                </span>
              {/each}
            </div>
            {#if SCOPES_PROGRESSIVE}
              <div class="more">
                <Button
                  variant="ghost"
                  size="sm"
                  data-enter-self
                  data-testid="launcher-more-scopes"
                  iconEnd={moreScopes ? ChevronUp : ChevronDown}
                  aria-expanded={moreScopes}
                  onclick={toggleMore}
                >
                  {moreScopes ? t('study.launcher.scopes_less') : t('study.launcher.scopes_more', { n: hiddenScopes })}
                </Button>
              </div>
            {/if}
          </section>

          {#if kind === 'test' && mode}
            <section class="block" aria-labelledby="launch-exam">
              <span class="label" id="launch-exam"><span class="step num" aria-hidden="true">3</span>{t('study.launcher.exam_label')}</span>
              <div class="box">
                <div class="kinds" role="group" aria-label={t('modes.test.kinds')}>
                  {#each mode.question_kinds as k (k)}
                    <Checkbox
                      label={t(`modes.test.kind.${k}`)}
                      description={t(`study.launcher.exam_kind_desc.${k}`)}
                      checked={!!testKinds[k]}
                      onchange={(v) => (testKinds[k] = v)}
                    />
                  {/each}
                </div>
                <OptRow label={t('study.launcher.count_label_test')} description={t('study.launcher.count_hint_test', { n: formatCount(mode.default_questions ?? 20) })} labelFor="launch-count">
                  <Input
                    id="launch-count"
                    class="count"
                    size="sm"
                    type="number"
                    min="1"
                    max="200"
                    label={t('study.launcher.count_label_test')}
                    hideLabel
                    placeholder={formatCount(mode.default_questions ?? 20)}
                    bind:value={limit}
                  />
                </OptRow>
              </div>
            </section>
          {/if}

          {#if advanced}
            <section class="block" id="launch-options" aria-labelledby="launch-adv">
              <span class="label" id="launch-adv"><span class="step num" aria-hidden="true">{kind === 'test' ? 4 : 3}</span>{t('study.launcher.advanced')}</span>
              <div class="box options">
                {#if kind !== 'test'}
                  <OptRow label={t('study.launcher.count_label')} description={t('study.launcher.count_hint')} labelFor="launch-count">
                    <Input
                      id="launch-count"
                      class="count"
                      size="sm"
                      type="number"
                      min="1"
                      max="9999"
                      label={t('study.launcher.count_label')}
                      hideLabel
                      placeholder={t('study.launcher.count_all')}
                      bind:value={limit}
                    />
                  </OptRow>
                {/if}
                {#if (kind === 'normal' || kind === 'learn') && selected?.mode !== 'trial'}
                  <OptRow label={t('study.practice_label')} description={t('study.launcher.practice_desc')}>
                    <Switch ariaLabel={t('study.practice_label')} bind:checked={practice} />
                  </OptRow>
                {/if}
                {#if kind === 'normal'}
                  <StudyModeOptions bind:value={modes} />
                  {#if attrs.length > 0}
                    <OptRow label={t('study.filter_label')}>
                      <span data-enter-self class="ctl"><Select class="sel" size="sm" label={t('study.filter_label')} hideLabel options={attrOptions} bind:value={attrId} /></span>
                      {#if attrDef}
                        <span data-enter-self class="ctl"><Select class="sel" size="sm" label={attrDef.name} hideLabel options={valueOptions} bind:value={attrValue} /></span>
                      {/if}
                    </OptRow>
                  {/if}
                {/if}
                {#if moduleOn('browse')}
                  <OptRow label={t('study.query_label')} description={t('study.launcher.query_desc')} labelFor="launch-query">
                    <Input
                      id="launch-query"
                      class="query"
                      size="sm"
                      label={t('study.query_label')}
                      hideLabel
                      bind:value={query}
                      disabled={savedId !== ''}
                      placeholder={t('study.query_placeholder')}
                      spellcheck="false"
                    />
                    {#if kind === 'normal' && saved.length > 0}
                      <span data-enter-self class="ctl"><Select class="sel" size="sm" label={t('study.saved_label')} hideLabel options={savedOptions} bind:value={savedId} /></span>
                    {/if}
                  </OptRow>
                {/if}
                {#if kind === 'normal'}
                  <OptRow label={t('study.postpone_label')} description={t('study.launcher.postpone_desc')} labelFor="launch-postpone">
                    <Input id="launch-postpone" class="days" size="sm" type="number" min="1" max="365" label={t('study.postpone_label')} hideLabel bind:value={postponeDays} />
                    <span class="unit">{t('study.postpone_days')}</span>
                    <Button size="sm" data-enter-self onclick={() => void postpone()}>{t('study.postpone')}</Button>
                  </OptRow>
                {/if}
              </div>
            </section>
          {/if}
        </div>
      </div>

      {#if notice}<p class="notice" role="status" data-testid="launcher-notice">{notice}</p>{/if}
    {/if}
  </div>

  {#snippet footer()}
    {#if kind && options}
      <div class="foot">
        <span class="adv" data-enter-self>
          <Switch label={t('study.launcher.advanced')} description={t('study.launcher.advanced_desc')} checked={advanced} onchange={setAdvanced} />
        </span>
        <p class="foot-sum" data-testid="launcher-summary" aria-live="polite">
          {#if blocked}
            <span class="warn">{blocked}</span>
          {:else}
            <strong>{summary}</strong><br /><span class="sched" data-testid="launcher-schedule">{schedLine}</span>
          {/if}
        </p>
        <span data-enter-start class="start">
          <Button variant="primary" size="lg" kbd="Enter" aria-keyshortcuts="Enter" icon={Play} disabled={!canStart} loading={starting} data-testid="launcher-start" onclick={() => void start()}>
            {t('study.launcher.start')}
          </Button>
        </span>
      </div>
    {/if}
  {/snippet}
</Dialog>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-5);
    align-items: start;
  }
  @media (max-width: 960px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
  }
  .label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .step {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    font-size: var(--text-2xs);
    color: var(--text-1);
  }
  .resume-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .resume {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--control-h-lg);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--accent-text);
    border-radius: var(--radius-card);
    background: var(--accent-soft);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  .resume:hover {
    background: var(--surface-2);
  }
  .resume:focus-visible,
  .mode:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .r-main {
    flex: 1;
    min-width: 0;
  }
  .r-meta {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  /* Modes: icon · name · schedule chip, one line under it. */
  .modes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .mode {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    grid-template-areas: 'icon name chip' 'icon line line';
    column-gap: var(--space-3);
    row-gap: 2px;
    align-items: center;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .mode:hover {
    background: var(--surface-2);
  }
  .mode.on {
    border-color: var(--accent-text);
    background: var(--accent-soft);
  }
  .m-icon {
    grid-area: icon;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    color: var(--text-2);
    align-self: start;
  }
  .mode.on .m-icon {
    color: var(--accent-text);
    border-color: var(--accent-text);
  }
  .m-name {
    grid-area: name;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .m-chip {
    grid-area: chip;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-2xs);
    color: var(--text-2);
    white-space: nowrap;
  }
  .m-chip.cal {
    color: var(--accent-text);
    border-color: var(--accent-text);
  }
  .m-line {
    grid-area: line;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .legend {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .legend :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .tag {
    display: inline-block;
    margin-left: var(--space-1);
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-2xs);
    font-weight: var(--weight-regular, 400);
    color: var(--text-2);
    white-space: nowrap;
  }
  .scopes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .scope-cell {
    display: block;
  }
  .scopes :global(.fc-option.scope-row) {
    min-height: 0;
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
  }
  .scopes :global(.fc-option.scope-row .text) {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .s-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .s-name {
    font-weight: var(--weight-medium);
  }
  .s-desc {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .s-desc.reason {
    color: var(--text-2);
    font-style: italic;
  }
  .s-count {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .s-count.zero {
    color: var(--text-3);
  }
  .dim .scopes {
    opacity: 0.6;
  }
  .more {
    display: flex;
  }
  .custom-note {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--warning);
  }
  .box {
    display: flex;
    flex-direction: column;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .box :global(.opt-row) {
    padding-inline: 0;
  }
  .kinds {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--border);
  }
  .box :global(.count) {
    width: 80px;
  }
  .options :global(.query) {
    width: 200px;
  }
  .options :global(.days) {
    width: 72px;
  }
  .options :global(.sel) {
    min-width: 150px;
  }
  .ctl {
    display: contents;
  }
  .unit {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .notice {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .notice.error {
    border-color: var(--danger);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    flex-wrap: wrap;
  }
  .adv {
    flex: none;
  }
  .foot-sum {
    flex: 1;
    min-width: 200px;
    margin: 0;
    text-align: end;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .sched {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .warn {
    color: var(--warning);
  }
  .start {
    display: contents;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
