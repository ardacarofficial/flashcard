<!-- A running backend job: message, progress (a spinner while the job gives no fraction) and Cancel. -->
<script lang="ts">
  import { api } from '$lib/api';
  import type { JobDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, ProgressBar, Spinner, X } from '$lib/ui';

  let { job }: { job: JobDto } = $props();

  const text = $derived(job.message ? td(job.message.key, job.message.params) : t('io.job.working'));
</script>

<div class="job" role="status">
  <div class="line">
    {#if job.progress === null || job.progress === undefined}<Spinner />{/if}
    <span class="text">{text}</span>
    <Button size="sm" variant="ghost" icon={X} onclick={() => void api.call('jobs.cancel', { id: job.id })}>{t('io.job.cancel')}</Button>
  </div>
  {#if job.progress !== null && job.progress !== undefined}
    <ProgressBar label={text} value={job.progress} max={1} size="sm" />
  {/if}
</div>

<style>
  .job {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .text {
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
</style>
