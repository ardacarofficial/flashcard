<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import type { BackupInfo, ExportReport, JobDto, RestoreReport } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { pickFile, pickSavePath } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';

  const FILTER = [{ name: 'Flashcard', extensions: ['fcbackup'] }];

  let job = $state<JobDto | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pending = $state<{ path: string; info: BackupInfo } | null>(null);

  async function run<T>(start: () => Promise<JobDto>, done: (r: T) => string) {
    error = null;
    message = null;
    try {
      const started = await start();
      job = started;
      const finished = await followJob(started, (j) => (job = j));
      job = null;
      const r = resultOf<T>(finished);
      if (r) message = done(r);
      else if (finished.status === 'cancelled') message = t('io.job.cancelled');
      else if (finished.error) error = td(finished.error.message_key);
    } catch (e) {
      job = null;
      error = td((e as ApiError).messageKey);
    }
  }

  async function create() {
    const day = new Date().toISOString().slice(0, 10);
    const path = await pickSavePath(`flashcard-${day}.fcbackup`, FILTER, t('io.path_prompt'));
    if (!path) return;
    await run<ExportReport>(
      () => api.call('io.backup.create', { path }),
      (r) => t('io.backup.created', { path: r.path }),
    );
  }

  async function chooseRestore() {
    error = null;
    message = null;
    const path = await pickFile(FILTER, t('io.path_prompt'));
    if (!path) return;
    try {
      const info = await api.call('io.backup.inspect', { path });
      if (!info.compatible) error = t('io.backup.incompatible');
      else pending = { path, info };
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  async function restore(path: string) {
    pending = null;
    await run<RestoreReport>(
      () => api.call('io.backup.restore', { path, safety_backup_path: null }),
      (r) => t('io.backup.restored', { path: r.safety_backup_path }),
    );
  }
</script>

<div class="panel">
  <p class="muted">{t('io.backup.hint')}</p>
  {#if job}
    <JobProgress {job} />
  {:else}
    <div class="row">
      <button class="primary" onclick={create}>{t('io.backup.create')}</button>
      <button onclick={chooseRestore}>{t('io.backup.restore')}</button>
    </div>
  {/if}
  {#if message}<p role="status">{message}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

{#if pending}
  <ConfirmDialog
    message={t('io.backup.confirm_title')}
    hint={t('io.backup.confirm_body', {
      date: new Date(pending.info.created_at).toLocaleString(i18n.locale),
      notes: pending.info.counts['notes'] ?? 0,
    })}
    confirmLabel={t('io.backup.restore')}
    onconfirm={() => restore(pending!.path)}
    oncancel={() => (pending = null)}
  />
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    align-items: flex-start;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
  }
</style>
