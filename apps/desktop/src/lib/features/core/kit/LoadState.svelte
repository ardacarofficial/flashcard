<!--
  Loading / error placeholder of a Settings or Statistics block: skeleton rows while loading,
  an error line with "Try again" when the load failed. Render the real content otherwise.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Button, CircleAlert, Icon, RefreshCw, Skeleton } from '$lib/ui';

  interface Props {
    /** `loading` shows skeletons; `error` the message and a retry button. */
    state: 'loading' | 'error';
    /** Error text (default: the generic "Could not load"). */
    message?: string;
    retry?: () => void;
    /** Skeleton shape: rows of text or a block (charts). */
    shape?: 'rows' | 'block';
    lines?: number;
  }

  let { state, message, retry, shape = 'rows', lines = 3 }: Props = $props();
</script>

{#if state === 'loading'}
  <div class="fc-load" role="status" aria-label={t('common.loading')}>
    {#if shape === 'block'}
      <Skeleton shape="card" height="160px" />
    {:else}
      <Skeleton {lines} />
    {/if}
  </div>
{:else}
  <div class="fc-load error" role="alert">
    <Icon icon={CircleAlert} size={16} />
    <span>{message ?? t('common.load_failed')}</span>
    {#if retry}<Button size="sm" icon={RefreshCw} onclick={retry}>{t('kit.retry')}</Button>{/if}
  </div>
{/if}

<style>
  .fc-load {
    padding: var(--space-2) 0;
  }
  .error {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .error span {
    flex: 1 1 12rem;
  }
</style>
