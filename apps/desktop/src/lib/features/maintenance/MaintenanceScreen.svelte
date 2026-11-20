<script lang="ts">
  /**
   * Maintenance (v1.0): version and folders, log export for a bug report, database check,
   * crash reports, updates. Every decision is the backend's; this screen shows and asks.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { IntegrityDto, MaintenanceStatus, ModuleDto, UpdateStatus } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { pickSavePath } from '../io/files';
  import type { ScreenCtx } from '../types';
  import { bundleName, formatSize } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let status = $state<MaintenanceStatus | null>(null);
  let integrity = $state<IntegrityDto | null>(null);
  let update = $state<UpdateStatus | null>(null);
  let updatesOn = $state(false);
  let notice = $state<string | null>(null);
  let checking = $state(false);
  let upToDate = $state(false);

  const logBytes = $derived(status ? status.logs.reduce((n, f) => n + f.bytes, 0) : 0);

  async function load() {
    await ctx.guarded(async () => {
      status = await api.call('maintenance.status', {});
      integrity = status.integrity;
      const modules: ModuleDto[] = (await api.call('modules.list', {})).modules;
      updatesOn = modules.some((m) => m.id === 'updates' && m.enabled);
      update = updatesOn ? await api.call('updates.status', {}) : null;
    });
  }

  async function exportLogs() {
    if (!status) return;
    const path = await pickSavePath(bundleName(status.version), [{ name: 'Zip', extensions: ['zip'] }], t('app_maintenance.logs.save_prompt'));
    if (!path) return;
    await ctx.guarded(async () => {
      const r = await api.call('maintenance.logs.export', { path });
      notice = t('app_maintenance.logs.exported', { files: r.files, path: r.path, redactions: r.redactions });
    });
  }

  async function checkDb() {
    checking = true;
    await ctx.guarded(async () => {
      integrity = await api.call('maintenance.integrity.check', { quick: false });
    });
    checking = false;
  }

  async function clearCrashes() {
    await ctx.guarded(async () => {
      const r = await api.call('maintenance.crash_reports.clear', {});
      notice = t('app_maintenance.crashes.cleared', { count: r.removed });
    });
    await load();
  }

  async function checkUpdate() {
    await ctx.guarded(async () => {
      update = await api.call('updates.check', {});
      upToDate = !update.available;
    });
  }

  async function download() {
    await ctx.guarded(() => api.call('updates.install', {}));
    await ctx.guarded(async () => (update = await api.call('updates.status', {})));
  }

  async function apply() {
    await ctx.guarded(async () => (update = await api.call('updates.apply', {})));
  }

  onMount(() => {
    void load();
    const unsub = api.on(['job_progress', 'module_toggled'], (env) => {
      if (env.event.type === 'job_progress' && env.event.data.job.kind !== 'updates.install') return;
      void api.call('updates.status', {}).then((s) => (update = s)).catch(() => {});
    });
    return () => void unsub.then((u) => u());
  });
</script>

<section>
  <h1>{t('app_maintenance.heading')}</h1>
  {#if status}
    <p class="muted">{t('app_maintenance.version', { version: status.version })}</p>
    {#if status.previous_run}
      <p class="muted">{status.previous_run.clean ? t('app_maintenance.previous_run.clean') : t('app_maintenance.previous_run.unclean')}</p>
    {/if}
    {#if notice}<p class="notice" role="status">{notice}</p>{/if}

    {#if updatesOn && update}
      <h2>{t('app_maintenance.updates.heading')}</h2>
      <p>{t('app_maintenance.updates.current', { version: update.current_version })}</p>
      {#if !update.supported}
        <p class="muted">{t('app_maintenance.updates.unsupported')}</p>
      {:else}
        {#if update.last_error}<p class="error">{td(update.last_error.message_key)}</p>{/if}
        {#if update.available}
          <p>{t('app_maintenance.updates.available', { version: update.available.version })}</p>
          {#if update.available.notes}<pre class="notes">{update.available.notes}</pre>{/if}
          {#if update.phase === 'idle'}
            <button onclick={download}>{t('app_maintenance.updates.install')}</button>
          {:else if update.phase === 'downloading'}
            <p class="muted">{t('app_maintenance.updates.downloading')}</p>
          {:else}
            <p>{t('app_maintenance.updates.ready')}</p>
            <button onclick={apply}>{t('app_maintenance.updates.apply')}</button>
          {/if}
        {:else}
          {#if upToDate}<p class="muted">{t('app_maintenance.updates.up_to_date')}</p>{/if}
          <button onclick={checkUpdate}>{t('app_maintenance.updates.check')}</button>
        {/if}
      {/if}
    {/if}

    <h2>{t('app_maintenance.logs.heading')}</h2>
    <p class="muted">{t('app_maintenance.logs.intro')}</p>
    {#if status.log_dir}
      <p>{t('app_maintenance.log_dir')}: <code>{status.log_dir}</code></p>
      <p class="muted">{t('app_maintenance.logs.files', { count: status.logs.length, size: formatSize(logBytes) })}</p>
    {:else}
      <p class="muted">{t('app_maintenance.no_log')}</p>
    {/if}
    <button onclick={exportLogs}>{t('app_maintenance.logs.export')}</button>

    <h2>{t('app_maintenance.integrity.heading')}</h2>
    {#if status.data_dir}<p>{t('app_maintenance.data_dir')}: <code>{status.data_dir}</code></p>{/if}
    {#if integrity}
      {#if integrity.ok}
        <p>{t('app_maintenance.integrity.ok', { ms: integrity.duration_ms })}</p>
      {:else}
        <p class="error">{t('app_maintenance.integrity.problems', { count: integrity.problems.length })}</p>
        <ul>
          {#each integrity.problems as p, i (i)}<li><code>{p}</code></li>{/each}
        </ul>
      {/if}
    {:else}
      <p class="muted">{t('app_maintenance.integrity.never')}</p>
    {/if}
    <button onclick={checkDb} disabled={checking}>{t('app_maintenance.integrity.check')}</button>

    <h2>{t('app_maintenance.crashes.heading')}</h2>
    {#if status.crash_reports.length === 0}
      <p class="muted">{t('app_maintenance.crashes.none')}</p>
    {:else}
      <ul class="crashes">
        {#each status.crash_reports as c (c.file)}
          <li><code>{c.time}</code> · {c.version} · {c.message}</li>
        {/each}
      </ul>
      <button class="danger" onclick={clearCrashes}>{t('app_maintenance.crashes.clear')}</button>
    {/if}
  {:else}
    <p class="muted">{t('common.loading')}</p>
  {/if}
</section>

<style>
  h2 {
    margin-top: var(--fc-space-4);
  }
  .notes {
    white-space: pre-wrap;
    font: inherit;
  }
  .crashes {
    padding-left: var(--fc-space-3);
  }
  .error {
    color: var(--fc-danger);
  }
</style>
