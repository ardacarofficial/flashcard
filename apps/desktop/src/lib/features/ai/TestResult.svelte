<!--
  A connection test's verdict in plain words (v1.3): what happened, what to do, and the
  provider's own message underneath.
-->
<script lang="ts">
  import type { AiConnectionTest, AiProviderDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { formatCount } from '$lib/format';
  import { CircleCheck, Icon, TriangleAlert } from '$lib/ui';

  let { test, provider }: { test: AiConnectionTest; provider: Pick<AiProviderDto, 'name' | 'local'> } = $props();

  const ok = $derived(test.outcome === 'ok');
  // An unreachable local server is almost always "the app is not running".
  const key = $derived(
    test.outcome === 'cannot_reach' && provider.local ? 'ai.test.outcome.cannot_reach_local' : `ai.test.outcome.${test.outcome}`,
  );
</script>

<div class="result" class:ok role="status" data-testid="ai-test-result" data-outcome={test.outcome}>
  <Icon icon={ok ? CircleCheck : TriangleAlert} size={16} />
  <div class="text">
    <p class="verdict">{td(key, { name: provider.name })}</p>
    {#if ok && test.answered_model}
      <p class="detail">{t('ai.test.answered', { model: test.answered_model, ms: formatCount(test.latency_ms) })}</p>
    {:else if ok && test.model === null}
      <p class="detail">{t('ai.test.list_only')}</p>
    {/if}
    {#if test.message}
      <p class="detail">
        {t('ai.test.provider_says')}
        <span class="msg">{test.message}</span>
        {#if test.http_status !== null}<span class="status">{t('ai.test.http', { status: test.http_status })}</span>{/if}
      </p>
    {/if}
  </div>
</div>

<style>
  .result {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--warning-soft);
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .result :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
  .result.ok {
    background: var(--success-soft);
  }
  .result.ok :global(svg) {
    color: var(--success);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  p {
    margin: 0;
  }
  .detail {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .msg,
  .status {
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }
  .status {
    margin-inline-start: var(--space-2);
  }
</style>
