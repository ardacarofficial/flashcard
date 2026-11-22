<script lang="ts">
  /**
   * First-run guide. v1.0 (docs/02 §32): on a new, empty collection the backend decides what
   * each subject sets up (`onboarding.options`) and does it in one undo step
   * (`onboarding.apply`). v1.3: that setup is step 1 of a four-step "Getting started" guide
   * where every step does the thing — add your first words (or start with a ready deck),
   * study, see where today's cards and progress live, optionally connect the browser
   * extension. After step 1 the guide is a small non-modal checklist (`GuidePanel`);
   * skippable at any time, re-opened from Help (`openGuide`).
   */
  import { onMount, tick, untrack } from 'svelte';
  import { api } from '$lib/api';
  import type { OnboardingChoices, OnboardingSubjectDto, StudyGoal } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { formatCount } from '$lib/format';
  import { Button, Check, Checkbox, Dialog, Icon, Input } from '$lib/ui';
  import type { DeckTreeNode } from '$lib/api/types';
  import type { ScreenCtx } from '../types';
  import GuidePanel from './GuidePanel.svelte';
  import { setGuideView, startGuide } from './guide.svelte';

  interface Props {
    /** Called after setup or skip (the shell re-reads decks). */
    ondone?: () => void;
    /** The shell's context: the guide's steps select decks and open pages with it. */
    ctx?: ScreenCtx;
    /** Opens Home (step 3). */
    home?: () => void;
  }
  let { ondone, ctx, home }: Props = $props();

  const GOALS: StudyGoal[] = ['language', 'exam', 'school', 'general'];
  /** Picked first: the app's main user learns English (any other subject is one click away). */
  const PRESELECT = 'english';

  let open = $state(false);
  /** Opened from the checklist (not the first run): closing it just goes back there. */
  let fromGuide = $state(false);
  let choices = $state<OnboardingChoices | null>(null);
  let subject = $state<OnboardingSubjectDto | null>(null);
  let deckName = $state('');
  /** The user typed a name: changing the subject no longer replaces it. */
  let named = false;
  let picked = $state<Record<string, boolean>>({});
  let created = $state<number | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const offered = $derived(
    subject && choices ? choices.starters.filter((s) => subject?.starters.includes(s.id)) : [],
  );
  const chosenStarters = $derived(subject ? subject.starters.filter((id) => picked[id]) : []);
  const all = $derived(choices ? GOALS.flatMap((g) => choices!.subjects.filter((s) => s.goal === g)) : []);

  function choose(s: OnboardingSubjectDto) {
    subject = s;
    if (!named) deckName = s.deck_name;
    picked = Object.fromEntries(s.starters.map((id) => [id, true]));
  }

  async function openSetup(first: boolean) {
    error = null;
    try {
      choices = await api.call('onboarding.options', { locale: i18n.locale });
    } catch {
      return; // Module off, or the app is locked.
    }
    named = false;
    const pre = choices.subjects.find((s) => s.subject === PRESELECT) ?? choices.subjects[0];
    if (pre) choose(pre);
    fromGuide = !first;
    open = true;
  }

  async function load() {
    try {
      const status = await api.call('onboarding.status', {});
      if (status.show) await openSetup(true);
    } catch {
      // Module off, or the app is locked: no guide now.
    }
  }

  /** Waits (briefly) for the shell to know the new deck, then opens `route` on it. */
  let pending = $state<{ deck: string; route: string } | null>(null);
  let pendingTimer: ReturnType<typeof setTimeout> | undefined;

  function contains(nodes: DeckTreeNode[], id: string): boolean {
    return nodes.some((n) => n.deck.id === id || contains(n.children, id));
  }

  function flush() {
    const p = pending;
    if (!p || !ctx) return;
    pending = null;
    clearTimeout(pendingTimer);
    ctx.selectDeck(p.deck);
    ctx.navigate?.(p.route);
  }

  $effect(() => {
    const p = pending;
    const decks = ctx?.decks ?? [];
    if (p && contains(decks, p.deck)) untrack(flush);
  });

  async function apply(withStarters: boolean) {
    if (!subject || busy) return;
    error = null;
    busy = true;
    try {
      const r = await api.call('onboarding.apply', {
        subject: subject.subject,
        deck_name: deckName,
        starters: withStarters ? chosenStarters : [],
        locale: i18n.locale,
      });
      created = r.notes_created;
      open = false;
      startGuide(r.deck_id);
      if (withStarters) {
        ctx?.selectDeck(r.deck_id);
      } else {
        // The add page opens on the new deck once the shell has re-read the decks.
        pending = { deck: r.deck_id, route: '/notes/new' };
        pendingTimer = setTimeout(flush, 3000);
      }
      ondone?.();
    } catch (e) {
      error = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    } finally {
      busy = false;
    }
  }

  async function skip() {
    open = false;
    if (fromGuide) return;
    setGuideView('closed');
    try {
      await api.call('onboarding.skip', {});
    } catch {
      /* nothing to do */
    }
    ondone?.();
  }

  let radios = $state<HTMLButtonElement[]>([]);

  /** Arrow keys move the choice through every subject (one radio group, one tab stop). */
  function onkey(e: KeyboardEvent, i: number) {
    // Enter on the chosen subject is the main action (keyboard-first: arrows, then Enter).
    if (e.key === 'Enter' && subject?.subject === all[i]?.subject) {
      e.preventDefault();
      void apply(false);
      return;
    }
    const dir = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir || all.length === 0) return;
    e.preventDefault();
    const n = (i + dir + all.length) % all.length;
    choose(all[n]!);
    void tick().then(() => radios[n]?.focus());
  }

  onMount(() => {
    void load();
    return () => clearTimeout(pendingTimer);
  });
</script>

{#if choices}
  <Dialog
    bind:open
    size="lg"
    dismissible={fromGuide}
    title={t('onboarding.title')}
    description={t('onboarding.intro')}
    initialFocus={'.choice[tabindex="0"]'}
  >
    <form
      class="ob"
      onsubmit={(e) => {
        e.preventDefault();
        void apply(false);
      }}
    >
      <div class="steps" aria-label={t('onboarding.step', { n: formatCount(1), total: formatCount(4) })} role="img">
        <span class="bar on"></span>
        <span class="bar"></span>
        <span class="bar"></span>
        <span class="bar"></span>
        <span class="step-text" aria-hidden="true">{t('onboarding.step', { n: formatCount(1), total: formatCount(4) })} · {t('onboarding.guide.add')}</span>
      </div>

      <section>
        <h3 id="ob-subjects">{t('onboarding.subject_heading')}</h3>
        <div class="grid" role="radiogroup" aria-labelledby="ob-subjects">
          {#each all as s, i (s.subject)}
            {@const chosen = subject?.subject === s.subject}
            <button
              type="button"
              role="radio"
              class="choice"
              class:chosen
              aria-checked={chosen}
              aria-labelledby="ob-{s.subject}"
              aria-describedby="ob-{s.subject}-d"
              tabindex={chosen || (!subject && i === 0) ? 0 : -1}
              bind:this={radios[i]}
              onclick={() => choose(s)}
              onkeydown={(e) => onkey(e, i)}
            >
              <span class="choice-title" id="ob-{s.subject}">{td(`onboarding.subject.${s.subject}`)}</span>
              {#if chosen}<span class="tick" aria-hidden="true"><Icon icon={Check} size={12} /></span>{/if}
              <span class="choice-desc" id="ob-{s.subject}-d">
                {s.template_id
                  ? t('onboarding.template', { name: td(`onboarding.templates.${s.template_id}`) })
                  : t('onboarding.template_none')}
              </span>
            </button>
          {/each}
        </div>
      </section>

      <Input label={t('onboarding.deck_name')} bind:value={deckName} oninput={() => (named = true)} />

      <section class="starters">
        <h3>{t('onboarding.starters')}</h3>
        <p class="muted">{offered.length === 0 ? t('onboarding.starters_none') : t('onboarding.starters_hint')}</p>
        {#each offered as s (s.id)}
          <Checkbox label={s.name} description={t('onboarding.notes', { count: s.note_count })} bind:checked={picked[s.id]} />
        {/each}
      </section>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    </form>
    {#snippet footer()}
      <Button variant="ghost" onclick={() => void skip()}>{fromGuide ? t('onboarding.cancel') : t('onboarding.skip')}</Button>
      <span class="spacer"></span>
      <Button disabled={chosenStarters.length === 0 || busy} onclick={() => void apply(true)}>{t('onboarding.apply_starters')}</Button>
      <Button variant="primary" kbd="Enter" disabled={!subject} loading={busy} onclick={() => void apply(false)}>{t('onboarding.apply_own')}</Button>
    {/snippet}
  </Dialog>
{/if}

<GuidePanel {ctx} {home} {created} onsetup={() => void openSetup(false)} />

<style>
  .ob {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .steps {
    display: grid;
    grid-template-columns: repeat(4, 1fr) auto;
    align-items: center;
    gap: var(--space-2);
  }
  .bar {
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
  }
  .bar.on {
    background: var(--accent);
  }
  .step-text {
    margin-inline-start: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .choice {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    padding-inline-end: var(--space-6);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease);
  }
  .choice:hover {
    border-color: var(--border-strong);
  }
  .choice.chosen {
    border-color: var(--accent-text);
    box-shadow: 0 0 0 1px var(--accent-text);
    background: var(--accent-soft);
  }
  /* Focus is the app's ring (outside the border); selection is the fill + border + tick. */
  .choice:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .tick {
    position: absolute;
    top: var(--space-2);
    inset-inline-end: var(--space-2);
    display: inline-flex;
    padding: 2px;
    border-radius: var(--radius-pill);
    background: var(--accent);
    color: var(--accent-fg);
  }
  .choice-title {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .choice-desc {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .spacer {
    flex: 1;
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
