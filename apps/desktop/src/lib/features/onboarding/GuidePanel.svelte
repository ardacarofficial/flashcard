<script lang="ts">
  /**
   * "Getting started" checklist (v1.3): a small floating panel at the bottom right, never
   * modal. Each step's button does the thing (opens the add page, a study session, Home,
   * Statistics, the Subtitle page) and ticks the step. "Hide" folds it into a small button,
   * "Finish the guide" closes it; Help and the starters page open it again (`openGuide`).
   * It takes the keyboard only when the user opened it on purpose.
   */
  import { tick } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { formatCount } from '$lib/format';
  import { Button, Check, ChevronDown, ChevronUp, Icon, IconButton, ProgressBar } from '$lib/ui';
  import type { DeckTreeNode } from '$lib/api/types';
  import type { ScreenCtx } from '../types';
  import { GUIDE_STEPS, guide, markStep, setGuideView, type GuideStep } from './guide.svelte';

  interface Props {
    ctx?: ScreenCtx;
    /** Opens Home (the shell's own page: it has no route). */
    home?: () => void;
    /** Cards the first-run setup added (this session), for step 1's line. */
    created?: number | null;
    /** Opens the first-run setup (step 1 on a collection without decks). */
    onsetup?: () => void;
  }
  let { ctx, home, created = null, onsetup }: Props = $props();

  const done = (s: GuideStep) => guide.done.includes(s);
  const count = $derived(guide.done.length);
  /** The required steps (the extension is optional). */
  const finished = $derived(GUIDE_STEPS.slice(0, 3).every((s) => guide.done.includes(s)));
  const progress = $derived(t('onboarding.guide.progress', { done: formatCount(count), total: formatCount(GUIDE_STEPS.length) }));
  const hasDecks = $derived((ctx?.decks.length ?? 0) > 0);
  /** Step 1 runs the first-run setup (no deck yet) instead of opening the add page. */
  const setup = $derived(!hasDecks && !!onsetup);
  /** The next step to do: only it shows its explanation (the others stay one line). */
  const current = $derived<GuideStep | null>(GUIDE_STEPS.find((s) => !guide.done.includes(s)) ?? null);
  const variant = (s: GuideStep) => (current === s ? 'primary' : 'ghost');

  // Step 1 ticks itself as soon as the collection holds a card (however it was added).
  const hasCards = (nodes: DeckTreeNode[]): boolean => nodes.some((n) => n.counts.total > 0 || hasCards(n.children));
  $effect(() => {
    if (!done('add') && hasCards(ctx?.decks ?? [])) markStep('add');
  });

  let panel = $state<HTMLElement | null>(null);
  let pill = $state<HTMLButtonElement | null>(null);

  // `openGuide()` asks for the keyboard: the user opened the guide on purpose.
  let seen = guide.focus;
  $effect(() => {
    const n = guide.focus;
    if (n === seen) return;
    seen = n;
    void tick().then(() => panel?.focus());
  });

  function contains(nodes: DeckTreeNode[], id: string): boolean {
    return nodes.some((n) => n.deck.id === id || contains(n.children, id));
  }

  /** Does the step's thing, ticks it and folds the checklist away while the user works. */
  function go(step: GuideStep, route: string | null, deck: string | null = null) {
    markStep(step);
    setGuideView('pill');
    if (deck && ctx && contains(ctx.decks, deck)) ctx.selectDeck(deck);
    if (route) ctx?.navigate?.(route);
  }

  function add() {
    if (setup) {
      onsetup?.();
      return;
    }
    go('add', '/notes/new', guide.deck);
  }

  function openHome() {
    markStep('where');
    setGuideView('pill');
    home?.();
  }

  async function hide() {
    const inside = !!panel?.contains(document.activeElement);
    setGuideView('pill');
    if (inside) {
      await tick();
      pill?.focus();
    }
  }

  async function show() {
    setGuideView('panel');
    await tick();
    panel?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    void hide();
  }
</script>

{#if guide.view === 'panel'}
  <!-- The panel takes focus when opened from Help; Esc inside it hides it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <aside class="guide" aria-labelledby="fc-guide-title" tabindex="-1" bind:this={panel} {onkeydown}>
    <header>
      <h2 id="fc-guide-title">{t('onboarding.guide.title')} <span class="num">· {progress}</span></h2>
      <IconButton size="sm" icon={ChevronDown} label={t('onboarding.guide.hide')} onclick={() => void hide()} />
    </header>
    <ProgressBar size="sm" label={t('onboarding.guide.title')} value={count} max={GUIDE_STEPS.length} />

    <ol>
      {#snippet mark(step: GuideStep, n: number)}
        <span class="mark" aria-hidden="true">
          {#if done(step)}<Icon icon={Check} size={12} />{:else}<span class="num">{formatCount(n)}</span>{/if}
        </span>
      {/snippet}
      {#snippet title(step: GuideStep, text: string)}
        <h3>
          {text}{#if step === 'extension'}<span class="opt">· {t('onboarding.guide.optional')}</span>{/if}
          {#if done(step)}<span class="sr-only">{t('onboarding.guide.done_sr')}</span>{/if}
        </h3>
      {/snippet}

      <li class:done={done('add')} class:current={current === 'add'}>
        {@render mark('add', 1)}
        <div class="text">
          {@render title('add', t('onboarding.guide.add'))}
          {#if created !== null && created > 0}
            <p role="status">{t('onboarding.done', { count: created })}</p>
          {:else if current === 'add'}
            <p>{setup ? t('onboarding.guide.add_setup_hint') : t('onboarding.guide.add_hint')}</p>
          {/if}
        </div>
        <Button size="sm" variant={variant('add')} onclick={add}>
          {setup ? t('onboarding.guide.setup_action') : t('onboarding.guide.add_action')}
        </Button>
      </li>

      <li class:done={done('study')} class:current={current === 'study'}>
        {@render mark('study', 2)}
        <div class="text">
          {@render title('study', t('onboarding.guide.study'))}
          {#if current === 'study'}<p>{t('onboarding.guide.study_hint')}</p>{/if}
        </div>
        <Button size="sm" variant={variant('study')} onclick={() => go('study', '/study', guide.deck)}>
          {t('onboarding.guide.study_action')}
        </Button>
      </li>

      <li class:done={done('where')} class:current={current === 'where'}>
        {@render mark('where', 3)}
        <div class="text">
          {@render title('where', t('onboarding.guide.where'))}
          {#if current === 'where'}
            <p>{t('onboarding.guide.home_hint')} {t('onboarding.guide.stats_hint')}</p>
            <div class="row">
              <Button size="sm" variant="secondary" onclick={openHome}>{t('onboarding.guide.home_action')}</Button>
              <Button size="sm" variant="secondary" onclick={() => go('where', '/stats')}>{t('onboarding.guide.stats_action')}</Button>
            </div>
          {:else}
            <div class="row quiet">
              <Button size="sm" variant="ghost" onclick={openHome}>{t('onboarding.guide.home_action')}</Button>
              <Button size="sm" variant="ghost" onclick={() => go('where', '/stats')}>{t('onboarding.guide.stats_action')}</Button>
            </div>
          {/if}
        </div>
      </li>

      <li class:done={done('extension')} class:current={current === 'extension'}>
        {@render mark('extension', 4)}
        <div class="text">
          {@render title('extension', t('onboarding.guide.extension'))}
          {#if current === 'extension'}<p>{t('onboarding.guide.extension_hint')}</p>{/if}
        </div>
        <Button size="sm" variant={current === 'extension' ? 'secondary' : 'ghost'} onclick={() => go('extension', '/subtitle')}>
          {t('onboarding.guide.extension_action')}
        </Button>
      </li>
    </ol>

    <footer>
      {#if finished}<p role="status">{t('onboarding.guide.all_done')}</p>{/if}
      <Button size="sm" variant={finished ? 'primary' : 'ghost'} onclick={() => setGuideView('closed')}>{t('onboarding.guide.finish')}</Button>
    </footer>
  </aside>
{:else if guide.view === 'pill'}
  <div class="pill">
    <Button size="sm" iconEnd={ChevronUp} bind:element={pill} onclick={() => void show()}>
      {t('onboarding.guide.title')} <span class="num">· {progress}</span>
    </Button>
  </div>
{/if}

<style>
  .guide,
  .pill {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    /* Under menus (50), tooltips (60) and dialogs (100): it never covers what the user opened. */
    z-index: 45;
  }
  /* A running study session owns the window (docs/11): the guide waits until it ends. */
  :global(body:has(.shell.session)) .guide,
  :global(body:has(.shell.session)) .pill {
    display: none;
  }
  .guide {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(21rem, calc(100vw - 2 * var(--space-4)));
    max-height: calc(100vh - 2 * var(--space-4));
    overflow-y: auto;
    padding: var(--space-2) var(--space-3) var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-float);
    font-size: var(--text-sm);
  }
  .guide:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    font-weight: var(--weight-regular);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: start;
    gap: var(--space-2);
    padding: var(--space-2) 0;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
  }
  .mark .num {
    color: var(--text-2);
  }
  li.done .mark {
    border-color: var(--success);
    background: var(--success-soft);
    color: var(--success);
  }
  li.done h3 {
    color: var(--text-2);
  }
  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .text p {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .row {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  /* Ghost buttons line their text up with the title. */
  .row.quiet {
    gap: var(--space-1);
    margin: 2px 0 0 calc(-1 * var(--space-2));
  }
  .opt {
    margin-inline-start: var(--space-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
    color: var(--text-3);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  footer p {
    flex: 1;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--success);
  }
</style>
