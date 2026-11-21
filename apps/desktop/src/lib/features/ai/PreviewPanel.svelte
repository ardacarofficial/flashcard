<script lang="ts">
  import type { AiPreview } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Card } from '$lib/ui';
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

<Card padding="lg" as="section" label={t('ai.preview.heading')} class="ai-preview">
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
    <Button variant="ghost" disabled={busy} onclick={oncancel}>{t('ai.preview.cancel')}</Button>
    <Button variant="primary" loading={busy} disabled={busy || preview.missing_key} onclick={onsend}>
      {t('ai.preview.send')}
    </Button>
  </div>
</Card>

<style>
  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  h4 {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  p {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
  }
  summary {
    cursor: pointer;
    font-size: var(--text-sm);
    color: var(--accent-text);
  }
  pre {
    white-space: pre-wrap;
    max-height: 20rem;
    overflow: auto;
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-xs);
  }
  .muted {
    color: var(--text-2);
  }
  .warn {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }
</style>
