<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiPreview, AiTask, AiVerdict } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { aiResult, previewTask, sendPreviewed } from './ai';
  import PreviewPanel from './PreviewPanel.svelte';

  /**
   * AI help on the study screen: an explanation (after the answer), a hint (before it) and a
   * meaning-based check of a typed answer. Hidden when the `ai` module is off or no provider
   * is set up. Every request is previewed before it is sent; explanations and hints appear
   * while they stream in.
   */
  interface Props {
    cardId: string;
    revealed: boolean;
    /** The typed answer (`null` = the card has no typing). */
    typed: string | null;
  }
  let { cardId, revealed, typed }: Props = $props();

  let ready = $state(false);
  let preview = $state.raw<AiPreview | null>(null);
  let task = $state.raw<AiTask | null>(null);
  let busy = $state(false);
  let text = $state<string | null>(null);
  let verdict = $state<{ verdict: AiVerdict; feedback: string } | null>(null);
  let error = $state<string | null>(null);

  async function loadReady() {
    try {
      const mods = await api.modules.list();
      const on = mods.modules.some((m) => m.id === 'ai' && m.enabled);
      ready = on && (await api.call('ai.providers.list', {})).providers.length > 0;
    } catch {
      ready = false;
    }
  }

  $effect(() => {
    void loadReady();
    const off = api.on(['ai_config_changed', 'module_toggled'], () => void loadReady());
    return () => void off.then((u) => u());
  });

  // A new card starts clean.
  $effect(() => {
    void cardId;
    preview = null;
    task = null;
    text = null;
    verdict = null;
    error = null;
  });

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  async function ask(next: AiTask) {
    error = null;
    text = null;
    verdict = null;
    try {
      preview = await previewTask(next);
      task = next;
    } catch (e) {
      fail(e);
    }
  }

  async function send() {
    const shown = preview;
    const sent = task;
    if (!shown || !sent) return;
    busy = true;
    preview = null;
    try {
      const done = await sendPreviewed(sent, shown, undefined, undefined, (partial) => {
        // Still the same request (the card may have moved on).
        if (task === sent) text = partial;
      });
      if (task !== sent) return;
      const r = aiResult(done);
      if (!r) {
        error = td(done.error?.message_key ?? 'errors.internal.unexpected');
        return;
      }
      const out = r.output;
      if (out.feature === 'explain' || out.feature === 'hint') text = out.text;
      else if (out.feature === 'check_answer') verdict = { verdict: out.verdict, feedback: out.feedback };
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  const canCheck = $derived(revealed && typed !== null && typed.trim() !== '');
</script>

{#if ready}
  <div class="study-ai">
    <div class="row">
      {#if !revealed}
        <button disabled={busy} onclick={() => void ask({ feature: 'hint', card_id: cardId })}>
          {t('ai.study.hint')}
        </button>
      {:else}
        <button disabled={busy} onclick={() => void ask({ feature: 'explain', card_id: cardId, question: null })}>
          {t('ai.study.explain')}
        </button>
        {#if canCheck}
          <button disabled={busy} onclick={() => void ask({ feature: 'check_answer', card_id: cardId, given: typed ?? '' })}>
            {t('ai.study.check')}
          </button>
        {/if}
      {/if}
    </div>
    {#if preview}
      <PreviewPanel {preview} {busy} onsend={() => void send()} oncancel={() => (preview = null)} />
    {/if}
    {#if busy && !text}
      <p class="muted" role="status">{t('ai.job.waiting')}</p>
    {/if}
    {#if text}
      <p class="answer" role="status" aria-busy={busy} data-testid="ai-text">{text}</p>
    {/if}
    {#if verdict}
      <p class="verdict {verdict.verdict}" role="status">
        {t(`ai.study.verdict.${verdict.verdict}`)}: {verdict.feedback}
      </p>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </div>
{/if}

<style>
  .study-ai {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    flex-wrap: wrap;
  }
  .answer {
    white-space: pre-wrap;
    border-left: 0.25em solid var(--fc-accent);
    padding-left: var(--fc-space-2);
  }
  .verdict.correct {
    color: var(--fc-success);
  }
  .verdict.wrong,
  .error {
    color: var(--fc-danger);
  }
  .muted {
    color: var(--fc-text-muted);
  }
</style>
