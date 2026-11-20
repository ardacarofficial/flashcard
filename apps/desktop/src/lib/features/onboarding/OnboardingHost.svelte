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

  onMount(() => void load());
</script>

{#if open && choices}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
      <h1 id="onboarding-title">{t('onboarding.title')}</h1>
      {#if step === 'subject'}
        <p>{t('onboarding.intro')}</p>
        {#each GOALS as goal (goal)}
          <fieldset>
            <legend>{td(`onboarding.goal.${goal}`)}</legend>
            {#each choices.subjects.filter((s) => s.goal === goal) as s (s.subject)}
              <label class="choice">
                <input type="radio" name="subject" checked={subject?.subject === s.subject} onchange={() => (subject = s)} />
                <span>{td(`onboarding.subject.${s.subject}`)}</span>
                <span class="muted">
                  {s.template_id
                    ? t('onboarding.template', { name: td(`onboarding.templates.${s.template_id}`) })
                    : t('onboarding.template_none')}
                </span>
              </label>
            {/each}
          </fieldset>
        {/each}
        <div class="actions">
          <button onclick={() => void skip()}>{t('onboarding.skip')}</button>
          <button class="primary" disabled={!subject} onclick={next}>{t('onboarding.next')}</button>
        </div>
      {:else if step === 'details'}
        <label class="field">
          {t('onboarding.deck_name')}
          <input bind:value={deckName} />
        </label>
        <h2>{t('onboarding.starters')}</h2>
        {#if offered.length === 0}
          <p class="muted">{t('onboarding.starters_none')}</p>
        {/if}
        {#each offered as s (s.id)}
          <label class="choice">
            <input type="checkbox" bind:checked={picked[s.id]} />
            <span>{s.name}</span>
            <span class="muted">{t('onboarding.notes', { count: s.note_count })}</span>
          </label>
        {/each}
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <div class="actions">
          <button onclick={() => (step = 'subject')}>{t('onboarding.back')}</button>
          <button onclick={() => void skip()}>{t('onboarding.skip')}</button>
          <button class="primary" onclick={() => void apply()}>{t('onboarding.apply')}</button>
        </div>
      {:else}
        <p role="status">{t('onboarding.done', { count: created })}</p>
        <div class="actions">
          <button class="primary" onclick={() => (open = false)}>{t('onboarding.close')}</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--fc-text) 40%, transparent);
    z-index: 20;
  }
  .dialog {
    background: var(--fc-bg);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-6);
    width: min(560px, 90vw);
    max-height: 90vh;
    overflow: auto;
  }
  fieldset {
    border: none;
    padding: 0;
    margin: 0 0 var(--fc-space-4);
  }
  legend {
    font-weight: 600;
    margin-bottom: var(--fc-space-1);
  }
  .choice {
    display: flex;
    gap: var(--fc-space-2);
    align-items: baseline;
    padding: var(--fc-space-1) 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--fc-space-2);
    margin-top: var(--fc-space-4);
  }
</style>
