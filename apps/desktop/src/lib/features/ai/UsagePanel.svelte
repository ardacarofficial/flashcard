<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { AiUsageGroup, AiUsageSummary, AiUsageTotals } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { usd } from './ai';
  import { Activity, EmptyState, Icon, Info, ProgressBar, Skeleton, TriangleAlert } from '$lib/ui';
  import { SettingsSection } from '../core/kit';

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
  const tokens = (x: AiUsageTotals) => t('ai.usage.tokens', { input: x.input_tokens, output: x.output_tokens });
</script>

<div class="panel">
  {#if error}
    <p class="banner danger" role="alert"><Icon icon={TriangleAlert} size={16} /><span>{error}</span></p>
  {/if}
  {#if !summary}
    {#if !error}
      <div class="tiles"><Skeleton shape="card" height="112px" /><Skeleton shape="card" height="112px" /></div>
    {/if}
  {:else}
    <div class="tiles">
      {#each [{ label: t('ai.usage.month'), x: summary.month }, { label: t('ai.usage.week'), x: summary.week }] as tile (tile.label)}
        <div class="tile">
          <h3>{tile.label}</h3>
          <p class="big">{usd(tile.x.cost_usd) ?? '$0'}</p>
          <p class="sub">{t('ai.usage.requests', { n: tile.x.requests })} · {tokens(tile.x)}</p>
        </div>
      {/each}
    </div>

    <SettingsSection title={t('ai.usage.budget')}>
      {#if summary.budget.limit_usd !== null}
        <ProgressBar
          label={t('ai.usage.budget')}
          value={summary.budget.month_spent_usd}
          max={summary.budget.limit_usd}
          tone={summary.budget.exceeded ? 'danger' : summary.budget.warning ? 'warning' : 'accent'}
        />
        <p class="budget" class:warn={summary.budget.warning} role={summary.budget.warning ? 'alert' : undefined}>
          {#if summary.budget.warning}<Icon icon={TriangleAlert} size={16} />{/if}
          <span>
            {summary.budget.exceeded
              ? t('ai.budget.exceeded', { limit: usd(summary.budget.limit_usd) ?? '' })
              : summary.budget.warning
                ? t('ai.budget.warning', { limit: usd(summary.budget.limit_usd) ?? '' })
                : t('ai.budget.status', {
                    spent: usd(summary.budget.month_spent_usd) ?? '$0',
                    limit: usd(summary.budget.limit_usd) ?? '',
                  })}
          </span>
        </p>
      {:else}
        <p class="muted">{t('ai.budget.none')}</p>
      {/if}
    </SettingsSection>

    {#if summary.by_feature.length > 0}
      <div class="groups">
        <SettingsSection title={t('ai.usage.by_feature')}>
          <ul class="list">
            {#each summary.by_feature as g (g.key)}
              <li><span class="name">{featureName(g)}</span><span class="val">{usd(g.totals.cost_usd) ?? '$0'}</span><span class="detail">{line(g.totals)}</span></li>
            {/each}
          </ul>
        </SettingsSection>
        <SettingsSection title={t('ai.usage.by_model')}>
          <ul class="list">
            {#each summary.by_model as g (g.key)}
              <li><span class="name mono">{g.key}</span><span class="val">{usd(g.totals.cost_usd) ?? '$0'}</span><span class="detail">{line(g.totals)}</span></li>
            {/each}
          </ul>
        </SettingsSection>
      </div>
    {:else}
      <EmptyState size="sm" icon={Activity} title={t('ai.usage.empty')} />
    {/if}
    <p class="note"><Icon icon={Info} size={16} /><span>{t('ai.usage.estimate_note')}</span></p>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: var(--space-4);
  }
  .tile {
    padding: var(--space-4) var(--space-6);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .tile h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .big {
    margin: var(--space-1) 0;
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
    line-height: var(--leading-tight);
  }
  .sub,
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .budget {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-3) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .budget.warn {
    color: var(--warning);
  }
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: var(--space-4);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .list li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 2px var(--space-3);
    padding: var(--space-2) 0;
    font-size: var(--text-sm);
  }
  .list li + li {
    border-top: 1px solid var(--border);
  }
  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .val {
    font-variant-numeric: tabular-nums;
  }
  .detail {
    grid-column: 1 / -1;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .note {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .banner {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
  }
  .banner.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
