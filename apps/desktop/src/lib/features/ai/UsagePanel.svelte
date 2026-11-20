<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiUsageGroup, AiUsageSummary, AiUsageTotals } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { usd } from './ai';

  let summary = $state<AiUsageSummary | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try {
      summary = await api.call('ai.usage.summary', {});
    } catch (e) {
      error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    }
  }

  $effect(() => {
    void load();
    const off = api.on('job_progress', (env) => {
      if (env.event.type === 'job_progress' && env.event.data.job.kind === 'ai.request') void load();
    });
    return () => void off.then((u) => u());
  });

  const line = (x: AiUsageTotals) =>
    t('ai.usage.line', {
      requests: x.requests,
      input: x.input_tokens,
      output: x.output_tokens,
      cost: usd(x.cost_usd) ?? '$0',
    });
  const featureName = (g: AiUsageGroup) => td(`ai.feature.${g.key}`);
</script>

<section>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if summary}
    <dl>
      <dt>{t('ai.usage.month')}</dt>
      <dd>{line(summary.month)}</dd>
      <dt>{t('ai.usage.week')}</dt>
      <dd>{line(summary.week)}</dd>
    </dl>
    {#if summary.budget.limit_usd !== null}
      <p class:warn={summary.budget.warning} role={summary.budget.warning ? 'alert' : undefined}>
        {summary.budget.exceeded
          ? t('ai.budget.exceeded', { limit: usd(summary.budget.limit_usd) ?? '' })
          : summary.budget.warning
            ? t('ai.budget.warning', { limit: usd(summary.budget.limit_usd) ?? '' })
            : t('ai.budget.status', {
                spent: usd(summary.budget.month_spent_usd) ?? '$0',
                limit: usd(summary.budget.limit_usd) ?? '',
              })}
      </p>
    {:else}
      <p class="muted">{t('ai.budget.none')}</p>
    {/if}
    {#if summary.by_feature.length > 0}
      <h3>{t('ai.usage.by_feature')}</h3>
      <ul>
        {#each summary.by_feature as g (g.key)}
          <li>{featureName(g)}: {line(g.totals)}</li>
        {/each}
      </ul>
      <h3>{t('ai.usage.by_model')}</h3>
      <ul>
        {#each summary.by_model as g (g.key)}
          <li>{g.key}: {line(g.totals)}</li>
        {/each}
      </ul>
    {:else}
      <p>{t('ai.usage.empty')}</p>
    {/if}
    <p class="muted">{t('ai.usage.estimate_note')}</p>
  {/if}
</section>

<style>
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--fc-space-1) var(--fc-space-3);
  }
  dd {
    margin: 0;
  }
  .muted {
    color: var(--fc-text-muted);
  }
  .warn,
  .error {
    color: var(--fc-danger);
  }
</style>
