<script lang="ts">
  /**
   * First-run guide (v1.0, docs/02 §32): shown once on a new, empty collection. Two short
   * steps — what you study, then the deck name and starter decks — and skippable at any
   * time. The backend decides what each subject sets up (`onboarding.options`) and does it
   * in one undo step (`onboarding.apply`).
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { OnboardingChoices, OnboardingSubjectDto, StudyGoal } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { Button, Check, Checkbox, CircleCheck, Dialog, Icon, Input } from '$lib/ui';

  interface Props {
    /** Called after setup or skip (the shell re-reads decks). */
    ondone?: () => void;
  }
  let { ondone }: Props = $props();

  const GOALS: StudyGoal[] = ['language', 'exam', 'school', 'general'];

  let open = $state(false);
  let choices = $state<OnboardingChoices | null>(null);
  let subject = $state<OnboardingSubjectDto | null>(null);
  let step = $state<'subject' | 'details' | 'done'>('subject');
  let deckName = $state('');
  let picked = $state<Record<string, boolean>>({});
  let created = $state(0);
  let error = $state<string | null>(null);

  const offered = $derived(
    subject && choices ? choices.starters.filter((s) => subject?.starters.includes(s.id)) : [],
  );

  async function load() {
    try {
      const status = await api.call('onboarding.status', {});
      if (!status.show) return;
      choices = await api.call('onboarding.options', { locale: i18n.locale });
      open = true;
    } catch {
      // Module off, or the app is locked: no guide now.
    }
  }

  function next() {
    if (!subject) return;
    deckName = subject.deck_name;
    picked = Object.fromEntries(subject.starters.map((id) => [id, true]));
    step = 'details';
  }

  async function apply() {
    if (!subject) return;
    error = null;
    try {
      const r = await api.call('onboarding.apply', {
        subject: subject.subject,
        deck_name: deckName,
        starters: subject.starters.filter((id) => picked[id]),
        locale: i18n.locale,
      });
      created = r.notes_created;
      step = 'done';
      ondone?.();
    } catch (e) {
      error = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }

  async function skip() {
    open = false;
    try {
      await api.call('onboarding.skip', {});
    } catch {
      /* nothing to do */
    }
    ondone?.();
  }

  const stepNo = $derived(step === 'subject' ? 1 : 2);
  let radios = $state<HTMLButtonElement[]>([]);
  const all = $derived(choices ? GOALS.flatMap((g) => choices!.subjects.filter((s) => s.goal === g)) : []);

  /** Arrow keys move the choice through every subject (one radio group, one tab stop). */
  function onkey(e: KeyboardEvent, i: number) {
    const dir = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir || all.length === 0) return;
    e.preventDefault();
    const n = (i + dir + all.length) % all.length;
    subject = all[n]!;
    radios[n]?.focus();
  }

  onMount(() => void load());
</script>

{#if choices}
  <Dialog
    bind:open
    size="lg"
    dismissible={false}
    title={t('onboarding.title')}
    description={step === 'subject' ? t('onboarding.intro') : step === 'details' ? t('onboarding.details_intro') : undefined}
  >
    {#if step !== 'done'}
      <div class="steps" aria-label={t('onboarding.step', { n: stepNo, total: 2 })} role="img">
        <span class="bar" class:on={stepNo >= 1}></span>
        <span class="bar" class:on={stepNo >= 2}></span>
        <span class="step-text" aria-hidden="true">{t('onboarding.step', { n: stepNo, total: 2 })}</span>
      </div>
    {/if}
    {#if step === 'subject'}
      <div class="goals" role="radiogroup" aria-label={t('onboarding.title')}>
        {#each GOALS as goal (goal)}
          {@const list = choices.subjects.filter((s) => s.goal === goal)}
          {#if list.length > 0}
            <div class="goal">
              <h3>{td(`onboarding.goal.${goal}`)}</h3>
              <div class="grid">
                {#each list as s (s.subject)}
                  {@const i = all.indexOf(s)}
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
                    onclick={() => (subject = s)}
                    onkeydown={(e) => onkey(e, i)}
                  >
                    <span class="choice-title" id="ob-{s.subject}">{td(`onboarding.subject.${s.subject}`)}</span>
                    {#if chosen}<span class="tick" aria-hidden="true"><Icon icon={Check} size={14} /></span>{/if}
                    <span class="choice-desc" id="ob-{s.subject}-d">
                      {s.template_id
                        ? t('onboarding.template', { name: td(`onboarding.templates.${s.template_id}`) })
                        : t('onboarding.template_none')}
                    </span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {:else if step === 'details'}
      <div class="details">
        <Input label={t('onboarding.deck_name')} bind:value={deckName} />
        <div class="starters">
          <h3>{t('onboarding.starters')}</h3>
          {#if offered.length === 0}
            <p class="muted">{t('onboarding.starters_none')}</p>
          {/if}
          {#each offered as s (s.id)}
            <Checkbox label={s.name} description={t('onboarding.notes', { count: s.note_count })} bind:checked={picked[s.id]} />
          {/each}
        </div>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
      </div>
    {:else}
      <div class="done">
        <span class="done-icon" aria-hidden="true"><Icon icon={CircleCheck} size={24} /></span>
        <p role="status">{t('onboarding.done', { count: created })}</p>
      </div>
    {/if}
    {#snippet footer()}
      {#if step === 'subject'}
        <Button variant="ghost" onclick={() => void skip()}>{t('onboarding.skip')}</Button>
        <span class="spacer"></span>
        <Button variant="primary" disabled={!subject} onclick={next}>{t('onboarding.next')}</Button>
      {:else if step === 'details'}
        <Button variant="ghost" onclick={() => void skip()}>{t('onboarding.skip')}</Button>
        <span class="spacer"></span>
        <Button onclick={() => (step = 'subject')}>{t('onboarding.back')}</Button>
        <Button variant="primary" onclick={() => void apply()}>{t('onboarding.apply')}</Button>
      {:else}
        <Button variant="primary" onclick={() => (open = false)}>{t('onboarding.close')}</Button>
      {/if}
    {/snippet}
  </Dialog>
{/if}

<style>
  .steps {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-6);
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
    font-weight: var(--weight-medium);
  }
  .goals {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
    gap: var(--space-2);
  }
  .choice {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
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
  /* Focus is a neutral outer ring; selection is the accent fill + border + tick. */
  .choice:focus-visible {
    outline: 2px solid var(--text-1);
    outline-offset: 3px;
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
  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .starters {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .done {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }
  .done p {
    margin: 0;
  }
  .done-icon {
    display: inline-flex;
    color: var(--success);
  }
  .spacer {
    flex: 1;
  }
</style>
