<script lang="ts">
  import type { AiPreview } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { usd } from './ai';

  interface Props {
    preview: AiPreview;
    busy?: boolean;
    onsend: () => void;
    oncancel: () => void;
  }
  let { preview, busy = false, onsend, oncancel }: Props = $props();

  const cost = $derived(usd(preview.estimated_cost_usd));
  const limit = $derived(usd(preview.budget.limit_usd) ?? '');
</script>

<section class="preview" aria-label={t('ai.preview.heading')}>
  <h3>{t('ai.preview.heading')}</h3>
  <p>
    {t('ai.preview.summary', {
      provider: preview.provider.name,
      model: preview.model,
      items: preview.items,
    })}
  </p>
  <p class="muted">
    {t('ai.preview.estimate', { tokens: preview.estimated_input_tokens, max: preview.max_tokens })}
    {cost ? t('ai.preview.cost', { cost }) : t('ai.preview.cost_unknown')}
  </p>
  {#if preview.provider.local}
    <p class="muted">{t('ai.preview.local')}</p>
  {/if}
  {#if preview.missing_key}
    <p class="warn" role="alert">{t('ai.preview.missing_key')}</p>
  {/if}
  {#if preview.budget.exceeded}
    <p class="warn" role="alert">{t('ai.budget.exceeded', { limit })}</p>
  {:else if preview.budget.warning}
    <p class="warn">{t('ai.budget.warning', { limit })}</p>
  {/if}
  <details>
    <summary>{t('ai.preview.show')}</summary>
    <h4>{t('ai.preview.system')}</h4>
    <pre>{preview.system}</pre>
    {#each preview.messages as m, i (i)}
      <h4>{t('ai.preview.message')}</h4>
      <pre>{m.text}</pre>
      {#if m.images.length > 0}
        <p class="muted">{t('ai.preview.images', { count: m.images.length })}</p>
      {/if}
    {/each}
  </details>
  <div class="actions">
    <button class="primary" disabled={busy || preview.missing_key} onclick={onsend}>
      {t('ai.preview.send')}
    </button>
    <button disabled={busy} onclick={oncancel}>{t('ai.preview.cancel')}</button>
  </div>
</section>

<style>
  .preview {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
    margin: var(--fc-space-3) 0;
  }
  h3 {
    margin-top: 0;
  }
  pre {
    white-space: pre-wrap;
    max-height: 20rem;
    overflow: auto;
    background: var(--fc-bg-subtle);
    padding: var(--fc-space-2);
  }
  .muted {
    color: var(--fc-text-muted);
  }
  .warn {
    color: var(--fc-danger);
  }
  .actions {
    display: flex;
    gap: var(--fc-space-2);
  }
</style>
