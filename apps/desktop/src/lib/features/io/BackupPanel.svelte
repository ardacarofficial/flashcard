<script lang="ts">
  import { formatDateTime } from '$lib/format';
  import { api, type ApiError } from '$lib/api';
  import type { BackupInfo, BackupReport, JobDto, RestoreReport } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { pickFile, pickSavePath } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';
  import { Button, CircleAlert, CircleCheck, Dialog, HardDrive, Icon, RotateCcw, Save } from '$lib/ui';
  import { FormRow, SchemaRows, SettingsSection } from '../core/kit';
  import type { BackupStatus, BackupVerifyReport } from '$lib/api/types';
  import type { ScreenCtx } from '../types';
  import { onMount } from 'svelte';

  let { ctx }: { ctx?: ScreenCtx } = $props();

  // v1.4: "are my cards safe?" — the core's backup status (last verified copy, next automatic
  // one, last failure) and the automatic-backup settings.
  let status = $state<BackupStatus | null>(null);
  const loadStatus = async () => {
    status = await api.call('io.backup.status', {}).catch(() => null);
  };
  onMount(() => void loadStatus());

  async function verifyLast() {
    const path = status?.last_verified?.path;
    if (!path) return;
    await run<BackupVerifyReport>(
      () => api.call('io.backup.verify', { path }),
      (r) => (r.ok ? t('io.status.verified_ok') : t('io.status.verified_bad', { problems: r.problems.length })),
    );
    void loadStatus();
  }

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
    await run<BackupReport>(
      () => api.call('io.backup.create', { path }),
      (r) => {
        // v1.4.1: the backup itself is fine; a second location that failed is said next to it.
        if (r.mirror?.disk_full) error = t('io.backup.mirror_disk_full');
        else if (r.mirror?.error) error = t('io.backup.mirror_failed', { error: r.mirror.error });
        return t('io.backup.created', { path: r.path });
      },
    );
    void loadStatus();
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

<SettingsSection title={t('io.status.title')} description={t('io.status.hint')} icon={CircleCheck} body="rows">
  <div class="status" data-testid="backup-status">
    {#if status?.last_failure}
      <p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{t('io.status.failed', { date: formatDateTime(status.last_failure.at) })}</span></p>
    {/if}
    {#if status?.last_verified}
      <p class="line">{t('io.status.last', { date: formatDateTime(status.last_verified.verified_at) })}</p>
    {:else if status}
      <p class="line">{t('io.status.never')}</p>
    {/if}
    {#if status?.next_due}<p class="line muted">{t('io.status.next', { date: formatDateTime(status.next_due) })}</p>{/if}
    {#if status?.folder}<p class="line muted">{t('io.status.folder', { folder: status.folder })}</p>{/if}
    {#if status?.mirror}
      {@const mirror = status.mirror}
      {#if mirror.state === 'ok'}
        <p class="line">
          {mirror.copies[0]
            ? t('io.status.mirror_ok', { count: mirror.copies.length, date: formatDateTime(mirror.copies[0].verified_at) })
            : t('io.status.mirror_ok_empty')}
        </p>
        <p class="line muted">{t('io.status.mirror_folder', { folder: mirror.folder })}</p>
      {:else}
        <p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{td(`io.status.mirror_${mirror.state}`, { folder: mirror.folder })}</span></p>
      {/if}
      {#if mirror.last_failure && mirror.disk_full}
        <p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{t('io.status.mirror_disk_full', { folder: mirror.folder, date: formatDateTime(mirror.last_failure.at) })}</span></p>
      {:else if mirror.last_failure}
        <p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{t('io.status.mirror_failed', { date: formatDateTime(mirror.last_failure.at) })}</span></p>
      {/if}
    {:else if status}
      <p class="line muted">{t('io.status.mirror_off')}</p>
    {/if}
  </div>
  <SchemaRows keys={['io.auto_backup', 'io.auto_backup_keep', 'io.backup_mirror_dir']} guarded={ctx?.guarded} onsaved={() => void loadStatus()} />
  {#if status?.last_verified && !job}
    <FormRow label={t('io.status.verify_title')} description={t('io.status.verify_hint')}>
      <Button onclick={verifyLast}>{t('io.status.verify')}</Button>
    </FormRow>
  {/if}
</SettingsSection>

<SettingsSection title={t('io.tabs.backup')} description={t('io.backup.hint')} icon={HardDrive} body="rows">
  {#if job}
    <div class="job"><JobProgress {job} /></div>
  {:else}
    <FormRow label={t('io.backup.create_title')} description={t('io.backup.create_hint')}>
      <Button variant="primary" icon={Save} onclick={create}>{t('io.backup.create')}</Button>
    </FormRow>
    <FormRow label={t('io.backup.restore_title')} description={t('io.backup.restore_hint')}>
      <Button icon={RotateCcw} onclick={chooseRestore}>{t('io.backup.restore')}</Button>
    </FormRow>
  {/if}
  {#if message}<p class="note ok" role="status"><Icon icon={CircleCheck} size={16} /><span>{message}</span></p>{/if}
  {#if error}<p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}
</SettingsSection>

<Dialog
  bind:open={confirmOpen}
  role="alertdialog"
  size="sm"
  title={t('io.backup.confirm_title')}
  description={pending
    ? t('io.backup.confirm_body', {
        date: formatDateTime(pending.info.created_at),
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
  .status .line {
    margin: 0 0 var(--space-1);
    font-size: var(--text-sm);
  }
  .status .muted {
    color: var(--text-2);
  }
  .job {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin: 0;
    padding-top: var(--space-3);
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
