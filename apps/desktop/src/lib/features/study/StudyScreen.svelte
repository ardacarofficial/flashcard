<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type {
    AnswerCheckDto,
    AttributeDefDto,
    Rating,
    SessionDto,
    SessionSummary,
    StartSession,
  } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { intervalLabel, tzOffsetMins } from './format';
  import { keyAction, RATINGS, type StudyAction, type StudyPhase } from './keys';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let session = $state<SessionDto | null>(null);
  let summary = $state<SessionSummary | null>(null);
  let resumable = $state<SessionDto | null>(null);
  let revealed = $state(false);
  let hint = $state<string | null>(null);
  let typed = $state('');
  let check = $state<AnswerCheckDto | null>(null);
  let practice = $state(false);
  let attrs = $state<AttributeDefDto[]>([]);
  let filterAttr = $state('');
  let filterValue = $state('');
  let postponeDays = $state(1);
  let notice = $state<string | null>(null);
  let busy = false;
  let shownAt = 0;
  let revealedAt: number | null = null;
  let input = $state<HTMLInputElement | null>(null);

  const card = $derived(session?.current ?? null);
  const phase = $derived<StudyPhase>(summary ? 'done' : card ? (revealed ? 'answer' : 'question') : 'idle');
  const filterDef = $derived(attrs.find((a) => a.id === filterAttr) ?? null);

  onMount(() => {
    void ctx.guarded(async () => {
      resumable = (await api.call('study.active', {})).session;
    });
  });

  // Classifications of the selected deck feed the custom-session filter.
  $effect(() => {
    const deck = ctx.selectedDeck;
    filterAttr = '';
    filterValue = '';
    attrs = [];
    if (!deck) return;
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

  function show(s: SessionDto) {
    session = s;
    summary = null;
    revealed = false;
    hint = null;
    typed = '';
    check = null;
    shownAt = Date.now();
    revealedAt = null;
  }

  async function settle(s: SessionDto) {
    if (s.current) show(s);
    else await finish(s);
  }

  async function finish(s: SessionDto) {
    session = s;
    summary = await api.call('study.end', { session_id: s.id });
    resumable = null;
  }

  function startRequest(): StartSession {
    const filtered = filterAttr !== '' && filterValue !== '';
    return {
      kind: filtered ? 'custom' : 'normal',
      mode: practice ? 'trial' : 'scheduled',
      deck_id: ctx.selectedDeck,
      filter: filtered
        ? { attributes: [{ attr_id: filterAttr, values: [filterValue] }], due_only: true }
        : null,
      tz_offset_mins: tzOffsetMins(),
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
    show(s);
  }

  async function reload() {
    const id = session?.id ?? summary?.session_id;
    if (!id) return;
    await settle(await api.call('study.session', { session_id: id }));
  }

  async function checkTyped() {
    if (!session || !card || typed.trim() === '') return;
    check = await api.call('study.check_answer', {
      session_id: session.id,
      card_id: card.card_id,
      given: typed,
    });
  }

  async function reveal() {
    if (card?.type_answer && !check) await checkTyped();
    revealed = true;
    revealedAt = Date.now();
    input?.blur();
  }

  async function rate(rating: Rating) {
    if (!session || !card) return;
    const now = Date.now();
    const next = await api.call('study.answer', {
      session_id: session.id,
      card_id: card.card_id,
      rating,
      duration_ms: Math.max(0, now - shownAt),
      reveal_ms: revealedAt === null ? null : Math.max(0, revealedAt - shownAt),
    });
    await settle(next);
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
    const el = e.target as HTMLElement | null;
    const tag = el?.tagName ?? '';
    const typing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
    // Buttons handle their own Enter/Space; keep shortcuts working everywhere else.
    if (tag === 'BUTTON' && (e.key === 'Enter' || e.key === ' ')) return;
    const action = keyAction(phase, e, typing);
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

  async function postpone() {
    notice = null;
    await ctx.guarded(async () => {
      const r = await api.call('study.postpone', {
        deck_id: ctx.selectedDeck,
        days: postponeDays,
        tz_offset_mins: tzOffsetMins(),
      });
      notice = t('study.postponed', { count: r.cards });
    });
  }

  function interval(ms: number): string {
    const l = intervalLabel(ms);
    return t(l.key, { n: l.n });
  }

  const accuracy = $derived(summary && summary.answered > 0 ? Math.round((summary.correct * 100) / summary.answered) : 0);
</script>

<svelte:window onkeydown={onKey} />

<section class="study">
  <h2>{t('study.heading')}</h2>

  {#if phase === 'idle'}
    <div class="panel">
      <p class="muted">{ctx.selectedPath ?? t('study.whole_collection')}</p>
      {#if resumable}
        <button onclick={() => resumable && void ctx.guarded(() => settle(resumable!))}>{t('study.resume')}</button>
      {/if}
      <label class="row">
        <input type="checkbox" bind:checked={practice} />
        {t('study.practice')}
      </label>
      {#if attrs.length > 0}
        <div class="row">
          <label>
            {t('study.filter_label')}
            <select bind:value={filterAttr}>
              <option value="">{t('study.filter_any')}</option>
              {#each attrs as a (a.id)}
                <option value={a.id}>{a.name}</option>
              {/each}
            </select>
          </label>
          {#if filterDef}
            <select bind:value={filterValue} aria-label={filterDef.name}>
              <option value="">{t('study.filter_any')}</option>
              {#each filterDef.options as o (o.id)}
                <option value={o.id}>{o.label}</option>
              {/each}
            </select>
          {/if}
        </div>
      {/if}
      <button class="primary" onclick={() => void run({ kind: 'start' })}>
        {filterAttr && filterValue ? t('study.custom_start') : t('study.start')}
        <kbd>{t('study.start_hint')}</kbd>
      </button>
      {#if notice}<p class="muted" role="status">{notice}</p>{/if}
      <div class="row postpone">
        <label>
          {t('study.postpone_label')}
          <input type="number" min="1" max="365" bind:value={postponeDays} aria-label={t('study.postpone_days')} />
        </label>
        <button onclick={() => void postpone()}>{t('study.postpone')}</button>
      </div>
    </div>
  {:else if phase === 'done' && summary}
    <div class="panel" role="status">
      <h3>{t('study.summary.heading')}</h3>
      <p>{t('study.summary.answered', { n: summary.answered })}</p>
      <p>{t('study.summary.accuracy', { n: accuracy })}</p>
      <p>
        {t('study.summary.time', {
          min: Math.floor(summary.time_ms / 60_000),
          sec: Math.floor((summary.time_ms % 60_000) / 1000),
        })}
      </p>
      <p>{t('study.summary.mistakes', { n: summary.mistakes })}</p>
      <div class="row">
        {#if summary.mistakes > 0}
          <button onclick={() => void run({ kind: 'repeat' })}>{t('study.summary.repeat')} <kbd>{'R'}</kbd></button>
        {/if}
        <button class="primary" onclick={() => void run({ kind: 'back' })}>{t('study.summary.back')}</button>
      </div>
    </div>
  {:else if session && card}
    <div class="meta muted">
      <span>{t('study.remaining', session.counts)}</span>
      {#if session.mode === 'trial'}<span class="badge">{t('study.practice_badge')}</span>{/if}
    </div>
    <div class="card">
      <p class="side" data-testid="question">{card.question}</p>
      {#if hint && !revealed}<p class="hint muted" data-testid="hint">{hint}</p>{/if}
      {#if card.type_answer}
        <input
          bind:this={input}
          bind:value={typed}
          disabled={revealed}
          placeholder={t('study.type_placeholder')}
          aria-label={t('study.type_placeholder')}
          onkeydown={onTypedKey}
        />
      {/if}
      {#if revealed}
        <hr />
        {#if check}
          <p class="check {check.grade}" role="status">
            {t(`study.grade.${check.grade}`)}:
            {#each check.diff as d, i (i)}<span class="diff {d.kind}">{d.text}</span>{/each}
          </p>
          {#if check.grade !== 'correct'}<p class="muted">{t('study.expected', { text: check.expected })}</p>{/if}
        {/if}
        <p class="side" data-testid="answer">{card.answer}</p>
      {/if}
    </div>

    {#if revealed}
      <div class="ratings">
        {#each RATINGS as r, i (r)}
          <button
            class:suggested={check?.suggested_rating === r}
            onclick={() => void run({ kind: 'rate', rating: r })}
          >
            <small>{interval(card.intervals[i]?.interval_ms ?? 0)}</small>
            {t(`study.rating.${r}`)}
            <kbd>{i + 1}</kbd>
          </button>
        {/each}
      </div>
    {:else}
      <div class="row">
        <button class="primary" onclick={() => void run({ kind: 'reveal' })}>{t('study.show_answer')} <kbd>␣</kbd></button>
        <button onclick={() => void run({ kind: 'hint' })}>{t('study.hint')} <kbd>{'H'}</kbd></button>
      </div>
    {/if}
    <div class="row tools">
      <button onclick={() => void run({ kind: 'undo' })}>{t('study.undo')} <kbd>{'Z'}</kbd></button>
      <button onclick={() => void run({ kind: 'suspend' })}>{t('study.suspend')} <kbd>{'S'}</kbd></button>
      <button onclick={() => void run({ kind: 'bury' })}>{t('study.bury')} <kbd>{'B'}</kbd></button>
      <button onclick={() => void run({ kind: 'end' })}>{t('study.end')} <kbd>{'Esc'}</kbd></button>
    </div>
    <p class="muted keys">{t('study.keys')}</p>
  {/if}
</section>

<style>
  .study {
    max-width: 720px;
  }
  .panel,
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  .card {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-6);
    background: var(--fc-surface);
    margin: var(--fc-space-3) 0;
  }
  .side {
    white-space: pre-wrap;
    font-size: 1.4em;
    margin: 0;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .meta {
    display: flex;
    gap: var(--fc-space-2);
  }
  .badge {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: 0 var(--fc-space-1);
  }
  .ratings {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--fc-space-2);
  }
  .ratings button {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .ratings button.suggested {
    border-color: var(--fc-accent);
  }
  .tools {
    margin-top: var(--fc-space-4);
  }
  kbd {
    color: var(--fc-text-muted);
    font-size: 0.8em;
    margin-left: var(--fc-space-1);
  }
  .diff.missing {
    text-decoration: underline;
    color: var(--fc-danger);
  }
  .diff.extra {
    text-decoration: line-through;
    color: var(--fc-danger);
  }
  .check.correct {
    color: var(--fc-success);
  }
  .keys {
    font-size: 0.85em;
  }
  .postpone input {
    width: 5em;
  }
</style>
