<!--
  Accepted answers (v1.4): other answers a test counts as right for this note. Adds with Enter,
  removes with ×; fc-app `accepted_answers.*` decides (blank / duplicate answers are skipped
  there). Re-reads the list after every write.
-->
<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AcceptedAnswers } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Chip, Input } from '$lib/ui';

  interface Props {
    noteId: string;
  }
  let { noteId }: Props = $props();

  let lists = $state<AcceptedAnswers[]>([]);
  let draft = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    const id = noteId;
    try {
      const r = await api.call('accepted_answers.list', { note_id: id });
      if (id === noteId) lists = r.fields;
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  $effect(() => {
    void noteId;
    void load();
  });

  async function edit(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await fn();
    } catch (e) {
      error = td((e as ApiError).messageKey);
    } finally {
      busy = false;
      await load();
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.ctrlKey || e.metaKey || e.isComposing) return;
    e.preventDefault();
    e.stopPropagation();
    const answer = draft;
    if (!answer.trim() || busy) return;
    void edit(async () => {
      await api.call('accepted_answers.add', { note_id: noteId, field: null, answers: [answer] });
      draft = '';
    });
  }

  const remove = (field: string, answer: string) =>
    edit(() => api.call('accepted_answers.remove', { note_id: noteId, field, answers: [answer] }));
</script>

<section class="answers" aria-labelledby="answers-title" data-testid="note-answers">
  <h4 id="answers-title">{t('note_extras.answers_title')}</h4>
  <p class="lead">{t('note_extras.answers_lead')}</p>
  {#if lists.some((l) => l.answers.length > 0)}
    <ul class="chips">
      {#each lists as l (l.field)}
        {#each l.answers as a (a)}
          <li>
            <Chip size="sm" label={a} onremove={() => remove(l.field, a)} />
          </li>
        {/each}
      {/each}
    </ul>
  {/if}
  <Input
    size="sm"
    label={t('note_extras.answers_add')}
    hideLabel
    placeholder={t('note_extras.answers_placeholder')}
    bind:value={draft}
    disabled={busy}
    {onkeydown}
  />
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</section>

<style>
  .answers {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h4 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .lead {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .chips {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .error {
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>
