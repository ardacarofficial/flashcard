<!--
  Home (v1.2, flow 2) answers "what do I do now": the first line is today's total with its
  Due · Learning · New breakdown and "Start studying" (`Enter` when the focus is not in a
  field); then the decks with work as dense rows (due / learning / new in mono, mastery bar,
  per-row "Study"; the row opens the deck page); a compact stat strip (streak, goal, today's
  answers); the inbox as one slim row when something waits. Every number is the backend's:
  `decks.summaries`, `stats.overview`, `goals.list`, `inbox.count`; parts whose module is off
  are left out (rule 7). v1.4: today's counts are the core's queue counts as sent (`today_*`);
  one line with the words you know (`words.stats`) leads to İlerleme; a "son yedek" line
  appears when `io.backup.status.stale` says the backup needs attention (link to Settings ›
  Data).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { BackupStale, DeckSummaries, DeckTreeNode, GoalItem, ModuleDto, ReviewTotals, Streak, WordCounts } from '$lib/api/types';
  import type { ScreenCtx } from '$lib/features';
  import { formatCount, formatDuration } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { usePageKeys } from '$lib/shell/page-keys.svelte';
  import {
    Button,
    BarChart3,
    ChevronRight,
    HardDrive,
    CircleCheck,
    EmptyState,
    Flame,
    Icon,
    Inbox,
    Library,
    ListRow,
    PageHeader,
    Play,
    Plus,
    ProgressBar,
    Skeleton,
    StatTile,
    Target,
    Timer,
  } from '$lib/ui';
  import MasteryBar from '../decks/MasteryBar.svelte';
  import { pageEnter } from '../decks/keys';
  import { launcher, openLauncher } from '../study/launcher.svelte';
  import { moduleOn, nodesById, summariesById, todayCounts } from '../decks/library';

  let { ctx }: { ctx: ScreenCtx } = $props();

  usePageKeys(() => [
    ...(canStudy ? [{ keys: 'Enter', label_key: 'help.page.start' as const }] : []),
    { keys: 'N', label_key: 'help.page.add' as const },
  ]);

  /** Decks listed when nothing is due (recently studied first). */
  const RECENT = 6;

  let summaries = $state<DeckSummaries | null>(null);
  let failed = $state(false);
  let modules = $state<ModuleDto[]>([]);
  let streak = $state<Streak | null>(null);
  let totals = $state<ReviewTotals | null>(null);
  let goal = $state<GoalItem | null>(null);
  let goalsLoaded = $state(false);
  let inbox = $state(0);
  let known = $state<WordCounts | null>(null);
  let backupStale = $state<BackupStale | null>(null);

  const nodes = $derived(nodesById(ctx.decks));
  const byId = $derived(summariesById(summaries));
  const today = $derived(summaries ? todayCounts(summaries.today) : null);
  const todayCount = $derived(today?.total ?? 0);
  const work = (n: DeckTreeNode) => todayCounts(n.counts).total;
  /** The study launcher is there (the study module is on): Start and the row buttons open it. */
  const canStudy = $derived(launcher.available);

  type Row = { node: DeckTreeNode; depth: number };
  /** Decks with work today in tree order (sub-decks indented under their parent). */
  const workRows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (list: DeckTreeNode[], depth: number) => {
      for (const node of list) {
        if (work(node) === 0) continue;
        out.push({ node, depth });
        walk(node.children, depth + 1);
      }
    };
    walk(ctx.decks, 0);
    return out;
  });
  /** Nothing due: the recently studied decks (or the first ones), so Home still leads somewhere. */
  const recentRows = $derived.by(() => {
    const ids = summaries?.recent.length ? summaries.recent : ctx.decks.map((n) => n.deck.id);
    return ids.flatMap((id) => nodes.get(id) ?? []).slice(0, RECENT).map((node) => ({ node, depth: 0 }));
  });
  const rows = $derived(workRows.length ? workRows : recentRows);

  const statsOn = $derived(moduleOn(modules, 'stats'));
  const goalsOn = $derived(moduleOn(modules, 'goals'));

  const open = (route: string) => ctx.navigate?.(route);

  /**
   * "Start studying" (v1.3): the study launcher over every deck. It shows what each choice
   * holds, so even with nothing due it leads somewhere (practice what you studied today, all
   * cards) — never a dead click; without the study module the button is not there.
   */
  function study() {
    openLauncher({ deckId: null });
  }

  function openDeck(id: string) {
    ctx.selectDeck(id);
    open('/decks');
  }

  function onWindowKey(e: KeyboardEvent) {
    if (canStudy && pageEnter(e)) {
      e.preventDefault();
      study();
    }
  }

  async function loadSummary() {
    try {
      summaries = await api.call('decks.summaries', {});
      failed = false;
    } catch {
      failed = true;
    }
  }

  async function loadExtras(list: ModuleDto[]) {
    const tz = -new Date().getTimezoneOffset();
    if (moduleOn(list, 'stats')) {
      api.call('stats.overview', { scope: { deck_id: null, tag: null, query: null }, tz_offset_mins: tz, days: 1, forecast_days: 1, fresh: null }).then(
        (o) => {
          streak = o.reviews.streak;
          totals = o.reviews.totals;
        },
        () => (streak = null),
      );
    }
    if (moduleOn(list, 'goals')) {
      api.call('goals.list', { tz_offset_mins: tz }).then(
        (g) => {
          goal = g.items.find((i) => !i.progress.done) ?? g.items[0] ?? null;
          goalsLoaded = true;
        },
        () => (goalsLoaded = true),
      );
    }
    if (moduleOn(list, 'words')) {
      // The main language's counts (the word index's one truth; İlerleme shows the rest).
      api.call('words.stats', {}).then(
        (w) => (known = w.languages[0]?.counts ?? null),
        () => (known = null),
      );
    } else known = null;
    if (moduleOn(list, 'io')) {
      api.call('io.backup.status', {}).then(
        (b) => (backupStale = b.stale),
        () => (backupStale = null),
      );
    } else backupStale = null;
    if (moduleOn(list, 'inbox')) {
      api.call('inbox.count', {}).then(
        (c) => (inbox = c.pending),
        () => (inbox = 0),
      );
    }
  }

  async function loadAll() {
    void loadSummary();
    try {
      modules = (await api.modules.list()).modules;
      await loadExtras(modules);
    } catch {
      /* the rest of Home still shows */
    }
  }

  onMount(() => {
    void loadAll();
    const unsub = api.on(['deck_', 'note', 'card', 'study_', 'undone', 'redone', 'data_changed', 'imported', 'collection_', 'goals_', 'inbox_'], () => void loadAll());
    return () => void unsub.then((u) => u());
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<!-- The top bar already names the page: the title stays for screen readers and the outline. -->
<PageHeader title={t('home.title')} class="sr-only" />

{#if failed}
  <div class="notice error" role="alert">
    <span>{t('home.load_failed')}</span>
    <Button size="sm" variant="ghost" onclick={() => void loadAll()}>{t('home.retry')}</Button>
  </div>
{:else if !summaries}
  <div class="stack" aria-busy="true" aria-label={t('common.loading')}>
    <Skeleton shape="card" height="88px" />
    <Skeleton shape="card" height="200px" />
  </div>
{:else if ctx.decks.length === 0}
  <div class="panel empty">
    <EmptyState
      icon={Library}
      title={t('home.empty_title')}
      description={t('home.empty_body')}
      action={{ label: t('home.empty_action'), icon: Plus, onclick: () => open(moduleOn(modules, 'notes') ? '/notes/new' : '/decks') }}
    />
  </div>
{:else}
  <div class="stack">
    <section class="today" aria-labelledby="home-today">
      <div class="today-text">
        <h2 id="home-today" class="eyebrow">{t('home.today')}</h2>
        {#if todayCount > 0}
          <p class="total"><span class="big num">{formatCount(todayCount)}</span> <span class="unit">{t('home.today_label')}</span></p>
        {:else}
          <p class="total caught"><Icon icon={CircleCheck} size={20} /> {t('home.caught_up')}</p>
        {/if}
        {#if today && todayCount > 0}
          <ul class="counts">
            <li><span class="dot due" aria-hidden="true"></span>{t('home.count_due')} <strong class="num">{formatCount(today.due)}</strong></li>
            <li><span class="dot learning" aria-hidden="true"></span>{t('home.count_learning')} <strong class="num">{formatCount(today.learning)}</strong></li>
            <li><span class="dot new" aria-hidden="true"></span>{t('home.count_new')} <strong class="num">{formatCount(today.new)}</strong></li>
          </ul>
        {:else}
          <p class="sub">{t('home.caught_up_body')}</p>
        {/if}
      </div>
      {#if canStudy}
        <Button
          variant={todayCount > 0 ? 'primary' : 'secondary'}
          size="lg"
          icon={Play}
          kbd="Enter"
          aria-keyshortcuts="Enter"
          data-testid="home-start"
          onclick={study}
        >
          {todayCount > 0 ? t('home.start') : t('home.practice')}
        </Button>
      {:else}
        <p class="sub off" data-testid="home-study-off">{t('home.study_off')}</p>
      {/if}
    </section>

    {#if inbox > 0}
      <button type="button" class="inbox-row" onclick={() => open('/inbox')}>
        <Icon icon={Inbox} size={16} />
        <span class="inbox-text">{t('home.inbox_notice', { count: inbox })}</span>
        <span class="inbox-open">{t('home.inbox_open')}<Icon icon={ChevronRight} size={14} /></span>
      </button>
    {/if}

    {#if known && (known.known > 0 || known.learning > 0)}
      <button type="button" class="inbox-row" data-testid="home-words" onclick={() => open(statsOn ? '/stats' : '/words')}>
        <Icon icon={BarChart3} size={16} />
        <span class="inbox-text">{t('home.words_line', { known: known.known, learning: known.learning })}</span>
        <span class="inbox-open">{statsOn ? t('home.words_progress') : t('home.words_open')}<Icon icon={ChevronRight} size={14} /></span>
      </button>
    {/if}

    {#if backupStale}
      <button type="button" class="inbox-row backup" data-testid="home-backup" onclick={() => ctx.navigate?.('/io', 'settings')}>
        <Icon icon={HardDrive} size={16} />
        <span class="inbox-text">{td(`home.backup.${backupStale}`)}</span>
        <span class="inbox-open">{t('home.backup.open')}<Icon icon={ChevronRight} size={14} /></span>
      </button>
    {/if}

    {#if statsOn || goalsOn}
      <div class="strip">
        {#if statsOn}
          {#if streak}
            <StatTile
              size="sm"
              icon={Flame}
              label={t('home.streak')}
              value={formatCount(streak.current)}
              unit={t('home.streak_unit', { count: streak.current })}
              hint={`${streak.studied_today ? t('home.streak_today') : t('home.streak_not_today')} · ${t('home.streak_best', { count: streak.longest })}`}
              tone={streak.studied_today ? 'neutral' : 'warning'}
            />
            <StatTile
              size="sm"
              icon={Timer}
              label={t('home.answered')}
              value={formatCount(totals?.reviews ?? 0)}
              unit={t('home.answered_unit', { count: totals?.reviews ?? 0 })}
              hint={totals && totals.time_ms > 0 ? t('home.answered_time', { time: formatDuration(totals.time_ms) }) : t('home.answered_none')}
            />
          {:else}
            <Skeleton shape="card" height="76px" />
          {/if}
        {/if}
        {#if goalsOn}
          {#if goal}
            <StatTile
              size="sm"
              icon={Target}
              label={`${t('home.goal')} · ${td(`goals.kind.${goal.goal.kind}`)}`}
              value={`${formatCount(goal.progress.current)} / ${formatCount(goal.progress.target)}`}
              tone={goal.progress.done ? 'success' : 'neutral'}
              hint={goal.progress.done ? t('home.goal_done') : undefined}
            >
              <ProgressBar
                size="sm"
                label={td(`goals.kind.${goal.goal.kind}`)}
                value={Math.min(goal.progress.current, goal.progress.target)}
                max={Math.max(1, goal.progress.target)}
                tone={goal.progress.done ? 'success' : 'accent'}
              />
            </StatTile>
          {:else if goalsLoaded}
            <div class="goal-empty">
              <span class="goal-label"><Icon icon={Target} size={14} />{t('home.goal')}</span>
              <span class="goal-none">{t('home.goal_none')}</span>
              <Button size="sm" variant="ghost" icon={Plus} onclick={() => open('/goals')}>{t('home.goal_set')}</Button>
            </div>
          {:else}
            <Skeleton shape="card" height="76px" />
          {/if}
        {/if}
      </div>
    {/if}

    <section aria-labelledby="home-decks">
      <div class="section-head">
        <h2 id="home-decks" class="section-title">{workRows.length ? t('home.decks_due') : t('home.recent')}</h2>
        <Button size="sm" variant="ghost" iconEnd={ChevronRight} onclick={() => open('/decks')}>{t('home.all_decks')}</Button>
      </div>
      <div class="panel">
        <ul class="deck-list" aria-labelledby="home-decks">
          <ListRow header cellColumns="var(--home-cols)">
            <span class="col-name">{t('home.col_deck')}</span>
            {#snippet cells()}
              <span>{t('home.count_due')}</span>
              <span>{t('home.count_learning')}</span>
              <span>{t('home.count_new')}</span>
              <span class="start">{t('home.col_mastery')}</span>
            {/snippet}
            {#snippet primary()}<span class="actions-space"></span>{/snippet}
          </ListRow>
          {#each rows as { node, depth } (node.deck.id)}
            <ListRow depth={depth} onclick={() => openDeck(node.deck.id)} buttonLabel={t('home.deck_open', { name: node.deck.name })} cellColumns="var(--home-cols)">
              <span class="name" data-testid="home-deck">
                <span class="swatch" style:background={node.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
                <span class="label">{node.deck.name}</span>
              </span>
              {#snippet cells()}
                {@const c = todayCounts(node.counts)}
                <span class="num n due" class:zero={c.due === 0}>{formatCount(c.due)}</span>
                <span class="num n" class:zero={c.learning === 0}>{formatCount(c.learning)}</span>
                <span class="num n" class:zero={c.new === 0}>{formatCount(c.new)}</span>
                <MasteryBar summary={byId.get(node.deck.id) ?? null} name={node.deck.name} />
              {/snippet}
              {#snippet primary()}
                {#if canStudy}
                  <Button size="sm" variant="secondary" icon={Play} aria-label={t('home.study_deck', { name: node.deck.name })} onclick={() => openLauncher({ deckId: node.deck.id })}>
                    {t('home.study')}
                  </Button>
                {/if}
              {/snippet}
            </ListRow>
          {/each}
        </ul>
      </div>
    </section>
  </div>
{/if}

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
  .today {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4) var(--space-6);
    flex-wrap: wrap;
    padding-bottom: var(--space-5);
    border-bottom: 1px solid var(--border);
  }
  .today-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .eyebrow {
    margin: 0;
    color: var(--accent-text);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }
  .total {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
  }
  .big {
    font-size: 40px;
    font-weight: var(--weight-semibold);
    letter-spacing: -0.03em;
    line-height: 1.05;
  }
  .unit {
    color: var(--text-2);
    font-size: var(--text-lg);
  }
  .total.caught {
    align-items: center;
    color: var(--success);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .sub {
    margin: 0;
    color: var(--text-2);
  }
  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-5);
    margin: var(--space-1) 0 0;
    padding: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    list-style: none;
  }
  .counts li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .counts strong {
    color: var(--text-1);
    font-weight: var(--weight-semibold);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .dot.due {
    background: var(--accent);
  }
  .dot.learning {
    background: var(--warning-solid);
  }
  .dot.new {
    background: var(--border-strong);
  }
  .inbox-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--row-h);
    padding: 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
    transition: background var(--motion-fast) var(--ease);
  }
  .inbox-row :global(svg) {
    flex: none;
    color: var(--accent-text);
  }
  .inbox-row.backup :global(svg) {
    color: var(--warning);
  }
  .inbox-row:hover {
    background: var(--surface-2);
  }
  .inbox-row:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .inbox-text {
    flex: 1;
    min-width: 0;
  }
  .inbox-open {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .inbox-open :global(svg) {
    color: var(--text-2);
  }
  .strip {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--space-3);
  }
  .strip :global(.fc-progress) {
    margin-top: var(--space-1);
  }
  .goal-empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .goal-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .goal-label :global(svg) {
    color: var(--text-3);
  }
  .goal-none {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .goal-empty :global(.fc-btn) {
    margin-inline-start: -8px;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-2);
  }
  .section-title {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .panel :global(.fc-list-row:last-child) {
    border-radius: 0 0 calc(var(--radius-card) - 1px) calc(var(--radius-card) - 1px);
  }
  .panel.empty {
    padding: var(--space-8) var(--space-4);
  }
  .deck-list {
    --home-cols: 88px 88px 56px 132px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .start {
    text-align: start;
  }
  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .label {
    overflow: hidden;
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .swatch {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .n {
    color: var(--text-1);
  }
  .due:not(.zero) {
    color: var(--accent-text);
    font-weight: var(--weight-medium);
  }
  .zero {
    color: var(--text-2);
  }
  .actions-space {
    display: block;
    width: 72px;
  }
  .deck-list :global(.fc-list-row .primary) {
    min-width: 72px;
    justify-content: flex-end;
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    font-size: var(--text-sm);
  }
  .notice.error {
    background: var(--danger-soft);
    color: var(--danger);
  }
  @media (max-width: 1023.98px) {
    .deck-list {
      --home-cols: 80px 80px 48px 96px;
    }
  }
</style>
