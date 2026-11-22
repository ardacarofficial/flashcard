<!--
  The study launcher (v1.3): one keyboard-first dialog for "I want to study". Top to bottom:
  unfinished sessions to continue (first and focused when there are any), the mode (Cards /
  Learn / Test / Match — only those `study.options` says are on) with one line on what it does
  to the schedule, what to study (five scopes with their exact counts; an empty one is disabled
  and says why), how many, and the collapsed options the old setup panels had. Enter starts
  (unless focus is on another button), Esc closes. Start re-validates: when the backend says
  there is nothing to study the dialog stays open with the reason.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { api, ApiError } from '$lib/api';
  import type { AttributeDefDto, SavedQueryDto, SessionDto, StartModes, TestQuestionKind } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, ChevronDown, ChevronUp, Chip, Dialog, Icon, Input, OptionTile, Play, SegmentedControl, Select, Skeleton, Switch } from '$lib/ui';
  import OptRow from '../modes/OptRow.svelte';
  import StudyModeOptions from '../modes/StudyModeOptions.svelte';
  import { moduleKnownOn, moduleOn } from '../cards/modules.svelte';
  import { tzOffsetMins } from './format';
  import { nothingToStudy, PRACTICE_SCOPES, startStudy, studyOptions, type LaunchKind, type ResumableSession, type StudyOptions, type StudyScope } from './contract';
  import {
    emptyReasonKey,
    isCustom,
    loadPrefs,
    minutesUntil,
    pickKind,
    pickScope,
    plannedCount,
    rememberChoice,
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

  const TEST_KINDS: TestQuestionKind[] = ['true_false', 'multiple_choice', 'written'];

  let options = $state<StudyOptions | null>(null);
  let failed = $state<string | null>(null);
  let kind = $state<LaunchKind | null>(null);
  let scope = $state<StudyScope>('due');
  let limit = $state<number | string>('');
  let showOptions = $state(false);
  let notice = $state<string | null>(null);
  let starting = $state(false);
  // Options of the old setup panels (no option is lost).
  let practice = $state(false);
  let query = $state('');
  let savedId = $state('');
  let attrId = $state('');
  let attrValue = $state('');
  let modes = $state<StartModes | null>(null);
  let testKinds = $state<Record<TestQuestionKind, boolean>>({ true_false: true, multiple_choice: true, written: true });
  let testQuestions = $state(20);
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
      if (preselect) {
        const k = pickKind(o, request.kind, prefs, deckId);
        kind = k;
        if (k) scope = pickScope(o.scopes, k, request.scope, prefs);
        // The dialog focused its first control while loading: move to the real first stop —
        // "Continue" when something is unfinished, else the chosen mode.
        await tick();
        root?.querySelector<HTMLElement>(o.resumable.length ? '[data-resume]' : '.fc-seg [aria-checked="true"]')?.focus();
      }
      now = Date.now();
    } catch (e) {
      failed = e instanceof ApiError ? td(e.messageKey) : t('common.load_failed');
    }
  }

  async function loadExtras() {
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
    if (moduleKnownOn('practice_test')) {
      api.call('settings.get', { scope: { kind: 'app' }, keys: ['practice_test.questions'] }).then(
        (r) => {
          const v = r.values.find((x) => x.key === 'practice_test.questions')?.value;
          if (typeof v === 'number') testQuestions = v;
        },
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
      showOptions = !!request.query;
      practice = request.practice ?? false;
      query = request.query ?? '';
      savedId = '';
      attrId = '';
      attrValue = '';
      void load(true);
      void loadExtras();
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
  const kindOptions = $derived((options?.modes ?? []).filter((k): k is LaunchKind => k !== 'custom' && k !== 'mistakes').map((k) => ({ value: k, label: t(`study.launcher.kind.${k}`) })));

  const choice = $derived<LaunchChoice | null>(
    kind
      ? {
          kind,
          scope,
          deckId,
          limit: Number(limit) > 0 ? Math.floor(Number(limit)) : null,
          practice,
          query: kind === 'normal' || moduleOn('browse') ? query : '',
          savedQueryId: kind === 'normal' ? savedId : '',
          attribute: kind === 'normal' && attrId !== '' && attrValue !== '' ? { attrId, value: attrValue } : null,
          modes,
          testKinds: TEST_KINDS.filter((k) => testKinds[k]),
          testQuestions,
        }
      : null,
  );
  const custom = $derived(choice ? isCustom(choice) : false);
  const planned = $derived(choice ? plannedCount(choice, selected) : null);
  const canStart = $derived(!!choice && (custom || (planned ?? 0) > 0) && !starting);
  const trial = $derived(!!choice && (selected?.mode === 'trial' || ((kind === 'normal' || kind === 'learn') && practice)));
  const startLabel = $derived(
    `${deckName} · ${
      planned === null
        ? t('study.launcher.filtered')
        : kind === 'test'
          ? t('study.launcher.questions', { count: planned })
          : t('study.launcher.cards', { count: planned })
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

  // The scope rows are one radio group: one Tab stop (the chosen row), arrows move inside.
  $effect(() => {
    void scope;
    void scopes;
    void custom;
    void tick().then(() => {
      for (const el of root?.querySelectorAll<HTMLElement>('[data-scope] button') ?? []) {
        el.tabIndex = el.getAttribute('aria-checked') === 'true' ? 0 : -1;
      }
    });
  });

  /** Arrow keys walk the scope rows (one radio group). */
  function onScopeKey(e: KeyboardEvent) {
    const dir = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const enabled = scopes.filter((s) => s.count > 0);
    const i = enabled.findIndex((s) => s.scope === scope);
    const next = enabled[(i + dir + enabled.length) % enabled.length];
    if (!next) return;
    scope = next.scope;
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-scope="${next.scope}"] button`)?.focus();
  }

  /** Enter starts — unless the focus is on a control whose own Enter means something else. */
  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.repeat || e.isComposing || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
    const el = e.target as HTMLElement | null;
    // Buttons with an Enter of their own: continue a session, toggles, menus, pickers.
    if (el?.closest('[data-enter-self], [role=listbox], [role=combobox], [role=menu], [role=switch], [aria-haspopup]')) return;
    e.preventDefault();
    void start();
  }

  // When the mode changes, show its remembered (or default) scope.
  function onKind(k: LaunchKind) {
    if (options) scope = pickScope(options.scopes, k, undefined, prefs);
    notice = null;
  }

  const attrOptions = $derived([{ value: '', label: t('study.filter_any') }, ...attrs.map((a) => ({ value: a.id, label: a.name }))]);
  const attrDef = $derived(attrs.find((a) => a.id === attrId) ?? null);
  const valueOptions = $derived([{ value: '', label: t('study.filter_any') }, ...(attrDef?.options ?? []).map((o) => ({ value: o.id, label: o.label }))]);
  const savedOptions = $derived([{ value: '', label: t('study.filter_any') }, ...saved.map((q) => ({ value: q.id, label: q.name }))]);
</script>

<Dialog
  bind:open
  title={deckId ? t('study.launcher.title_deck', { deck: deckName }) : t('study.launcher.title')}
  size="md"
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
              <button type="button" class="resume" data-resume data-enter-self data-testid="launcher-resume" onclick={() => resume(r)}>
                <Icon icon={Play} size={14} />
                <span class="r-main"><strong>{t('study.launcher.resume')}</strong> · {r.deck_name ?? t('study.launcher.deck_all')} · {t(`study.launcher.kind.${k}`)}</span>
                <span class="r-meta num">{t('study.launcher.resume_meta', { answered: formatCount(r.answered), remaining: formatCount(r.remaining) })}</span>
              </button>
            {/each}
          </div>
        </section>
      {/if}

      <section class="block">
        <span class="label" id="launch-mode">{t('study.launcher.mode_label')}</span>
        <SegmentedControl label={t('study.launcher.mode_label')} size="sm" fullWidth options={kindOptions} bind:value={kind} onchange={onKind} />
        <p class="mode-desc" data-testid="launcher-mode-desc">
          {t(`study.launcher.kind_desc.${kind}`)}
          {#if trial && kind !== 'test' && kind !== 'match'}<span class="tag">{t('study.launcher.practice_tag')}</span>{/if}
        </p>
      </section>

      <section class="block" class:dim={custom}>
        <span class="label" id="launch-scope">{t('study.launcher.scope_label')}</span>
        {#if custom}<p class="custom-note">{t('study.launcher.custom_note')}</p>{/if}
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="scopes" role="radiogroup" aria-labelledby="launch-scope" onkeydown={onScopeKey}>
          {#each scopes as s (s.scope)}
            {@const empty = s.count === 0}
            <span class="scope-cell" data-scope={s.scope} data-testid="launcher-scope-{s.scope}">
              <OptionTile class="scope-row" checked={scope === s.scope} disabled={empty || custom} onclick={() => (scope = s.scope)}>
                <span class="s-text">
                  <span class="s-name">
                    {t(`study.launcher.scope.${s.scope}`)}
                    <!-- Test and Match never change the schedule: the tag only matters for Cards / Learn. -->
                    {#if PRACTICE_SCOPES.includes(s.scope) && (kind === 'normal' || kind === 'learn')}<span class="tag">{t('study.launcher.practice_tag')}</span>{/if}
                  </span>
                  <span class="s-desc" class:reason={empty}>{empty ? reasonText(s) : t(`study.launcher.scope_desc.${s.scope}`)}</span>
                </span>
                <span class="s-count num" class:zero={empty}>{formatCount(s.count)}</span>
              </OptionTile>
            </span>
          {/each}
        </div>
      </section>

      <section class="block row">
        <OptRow label={kind === 'test' ? t('study.launcher.count_label_test') : t('study.launcher.count_label')} description={kind === 'test' ? t('study.launcher.count_hint_test', { n: formatCount(testQuestions) }) : t('study.launcher.count_hint')} labelFor="launch-count">
          <Input
            id="launch-count"
            class="count"
            size="sm"
            type="number"
            min="1"
            max="9999"
            label={kind === 'test' ? t('study.launcher.count_label_test') : t('study.launcher.count_label')}
            hideLabel
            placeholder={kind === 'test' ? formatCount(testQuestions) : t('study.launcher.count_all')}
            bind:value={limit}
          />
        </OptRow>
      </section>

      <div class="opts-toggle">
        <Button
          variant="ghost"
          size="sm"
          data-enter-self
          iconEnd={showOptions ? ChevronUp : ChevronDown}
          aria-expanded={showOptions}
          aria-controls="launch-options"
          onclick={() => (showOptions = !showOptions)}
        >
          {t('study.launcher.options')}
        </Button>
      </div>
      {#if showOptions}
        <div id="launch-options" class="options">
          {#if kind === 'normal' || kind === 'learn'}
            {#if selected?.mode !== 'trial'}
              <OptRow label={t('study.practice_label')} description={t('study.launcher.practice_desc')}>
                <Switch ariaLabel={t('study.practice_label')} bind:checked={practice} />
              </OptRow>
            {/if}
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
          {#if kind === 'test'}
            <OptRow label={t('modes.test.kinds')} description={TEST_KINDS.some((k) => testKinds[k]) ? undefined : t('modes.test.all_kinds')}>
              <div class="chips" role="group" aria-label={t('modes.test.kinds')} data-enter-self>
                {#each TEST_KINDS as k (k)}
                  <Chip label={t(`modes.test.kind.${k}`)} selected={testKinds[k]} onclick={() => (testKinds[k] = !testKinds[k])} />
                {/each}
              </div>
            </OptRow>
          {/if}
          {#if moduleOn('browse')}
            <OptRow label={t('study.query_label')} labelFor="launch-query">
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
            <OptRow label={t('study.postpone_label')} labelFor="launch-postpone">
              <Input id="launch-postpone" class="days" size="sm" type="number" min="1" max="365" label={t('study.postpone_label')} hideLabel bind:value={postponeDays} />
              <span class="unit">{t('study.postpone_days')}</span>
              <Button size="sm" data-enter-self onclick={() => void postpone()}>{t('study.postpone')}</Button>
            </OptRow>
          {/if}
        </div>
      {/if}

      {#if notice}<p class="notice" role="status" data-testid="launcher-notice">{notice}</p>{/if}
    {/if}
  </div>

  {#snippet footer()}
    {#if kind && options}
      <span data-enter-start class="start">
        <Button variant="primary" kbd="Enter" aria-keyshortcuts="Enter" icon={Play} disabled={!canStart} loading={starting} data-testid="launcher-start" onclick={() => void start()}>
          {t('study.launcher.start')} · {startLabel}
        </Button>
      </span>
    {/if}
  {/snippet}
</Dialog>

<style>
  .launcher {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
  }
  .label {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
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
  .resume:focus-visible {
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
  .mode-desc {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
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
  .custom-note {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--warning);
  }
  .block.row :global(.opt-row) {
    padding-inline: 0;
  }
  .row :global(.count) {
    width: 96px;
  }
  .opts-toggle {
    display: flex;
    border-top: 1px solid var(--border);
    padding-top: var(--space-2);
  }
  .options {
    display: flex;
    flex-direction: column;
  }
  .options :global(.query) {
    width: 220px;
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
  .chips {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
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
