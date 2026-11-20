<script lang="ts">
  import { api } from '$lib/api';
  import type { JobDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';

  let { job }: { job: JobDto } = $props();
</script>

<div class="job" role="status">
  <span>{job.message ? td(job.message.key, job.message.params) : t('io.job.working')}</span>
  <progress value={job.progress ?? undefined} max="1"></progress>
  <button onclick={() => void api.call('jobs.cancel', { id: job.id })}>{t('io.job.cancel')}</button>
</div>

<style>
  .job {
    display: flex;
    align-items: center;
    gap: var(--fc-space-2);
  }
  progress {
    flex: 1;
  }
</style>
