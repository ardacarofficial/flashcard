<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { BackupInfo, ExportReport, JobDto, RestoreReport } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { pickFile, pickSavePath } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';
  import { Button, CircleAlert, CircleCheck, Dialog, HardDrive, History, Icon, RotateCcw, Save } from '$lib/ui';
  import { SettingsSection } from '../core/kit';

  const FILTER = [{ name: 'Flashcard', extensions: ['fcbackup'] }];

  let job = $state<JobDto | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pending = $state<{ path: string; info: BackupInfo } | null>(null);
  let confirmOpen = $state(false);
  $effect(() => {
    confirmOpen = pending !== null;
  });

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

<SettingsSection title={t('io.tabs.backup')} description={t('io.backup.hint')} icon={HardDrive}>
  <div class="body">
    {#if job}
      <JobProgress {job} />
    {:else}
      <div class="choices">
        <div class="choice">
          <span class="ic" aria-hidden="true"><Icon icon={Save} size={20} /></span>
          <div class="txt">
            <p class="ct">{t('io.backup.create_title')}</p>
            <p class="cd">{t('io.backup.create_hint')}</p>
          </div>
          <Button variant="primary" onclick={create}>{t('io.backup.create')}</Button>
        </div>
        <div class="choice">
          <span class="ic" aria-hidden="true"><Icon icon={History} size={20} /></span>
          <div class="txt">
            <p class="ct">{t('io.backup.restore_title')}</p>
            <p class="cd">{t('io.backup.restore_hint')}</p>
          </div>
          <Button icon={RotateCcw} onclick={chooseRestore}>{t('io.backup.restore')}</Button>
        </div>
      </div>
    {/if}
    {#if message}<p class="note ok" role="status"><Icon icon={CircleCheck} size={16} /><span>{message}</span></p>{/if}
    {#if error}<p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}
  </div>
</SettingsSection>

<Dialog
  bind:open={confirmOpen}
  role="alertdialog"
  size="sm"
  title={t('io.backup.confirm_title')}
  description={pending
    ? t('io.backup.confirm_body', {
        date: new Date(pending.info.created_at).toLocaleString(i18n.locale),
        notes: pending.info.counts['notes'] ?? 0,
      })
    : undefined}
  onclose={() => (pending = null)}
>
  {#snippet footer()}
    <Button onclick={() => (pending = null)}>{t('common.cancel')}</Button>
    <Button variant="danger" onclick={() => pending && restore(pending.path)}>{t('io.backup.restore')}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .choice {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-0);
  }
  .ic {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .txt {
    flex: 1 1 14rem;
    min-width: 0;
  }
  .ct {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .cd {
    margin: 2px 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .note :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .ok {
    color: var(--success);
  }
  .bad {
    color: var(--danger);
  }
</style>
