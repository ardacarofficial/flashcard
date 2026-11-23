<script lang="ts">
  /**
   * Maintenance (v1.0): version and folders, log export for a bug report, database check,
   * crash reports, updates. Every decision is the backend's; this screen shows and asks.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { IntegrityDto, MaintenanceStatus, ModuleDto, RescueChoice, UpdateStatus } from '$lib/api/types';
  import { formatDateTime } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { pickSavePath } from '../io/files';
  import type { ScreenCtx } from '../types';
  import { bundleName, formatSize } from './format';
  import { Button, CircleAlert, CircleCheck, Chip, Database, Download, Icon, LifeBuoy, RefreshCw, ScrollText, Trash2, TriangleAlert } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let status = $state<MaintenanceStatus | null>(null);
  let integrity = $state<IntegrityDto | null>(null);
  let update = $state<UpdateStatus | null>(null);
  let updatesOn = $state(false);
  let notice = $state<string | null>(null);
  let checking = $state(false);
  let upToDate = $state(false);
  let failed = $state(false);
  let checkingUpdate = $state(false);
  /** v1.4.1: the rescue choice waiting for confirmation, and whether it was sent. */
  let rescueAsk = $state<RescueChoice | null>(null);
  let rescueSent = $state(false);

  /** Crash time (RFC 3339 from the backend) in the user's locale; the raw text if unparsable. */
  const when = (iso: string) => {
    const ms = Date.parse(iso);
    return Number.isNaN(ms) ? iso : formatDateTime(ms);
  };
  const logBytes = $derived(status ? status.logs.reduce((n, f) => n + f.bytes, 0) : 0);

  async function load() {
    failed = false;
    const ok = await ctx.guarded(async () => {
      status = await api.call('maintenance.status', {});
      integrity = status.integrity;
      const modules: ModuleDto[] = (await api.call('modules.list', {})).modules;
      updatesOn = modules.some((m) => m.id === 'updates' && m.enabled);
      update = updatesOn ? await api.call('updates.status', {}) : null;
    });
    failed = !ok && !status;
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

  /** The app restarts and the shell rescues before the collection opens again. */
  async function requestRescue(mode: RescueChoice) {
    const ok = await ctx.guarded(() => api.call('maintenance.rescue.request', { mode }));
    rescueAsk = null;
    rescueSent = ok;
  }

  async function clearCrashes() {
    await ctx.guarded(async () => {
      const r = await api.call('maintenance.crash_reports.clear', {});
      notice = t('app_maintenance.crashes.cleared', { count: r.removed });
    });
    await load();
  }

  async function checkUpdate() {
    checkingUpdate = true;
    await ctx.guarded(async () => {
      update = await api.call('updates.check', {});
      upToDate = !update.available;
    });
    checkingUpdate = false;
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

<SettingsPage title={t('app_maintenance.heading')} subtitle={status ? t('app_maintenance.version', { version: status.version }) : undefined}>
  {#snippet meta()}
    {#if status?.previous_run}
      <Chip size="sm" tone={status.previous_run.clean ? 'success' : 'warning'} icon={status.previous_run.clean ? CircleCheck : TriangleAlert}
        label={status.previous_run.clean ? t('app_maintenance.previous_run.clean') : t('app_maintenance.previous_run.unclean')} />
    {/if}
  {/snippet}

  {#if !status}
    <SettingsSection>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} />
    </SettingsSection>
  {:else}
    {#if notice}<p class="notice" role="status"><Icon icon={CircleCheck} size={16} /><span>{notice}</span></p>{/if}

    {#if updatesOn && update}
      <SettingsSection title={t('app_maintenance.updates.heading')} icon={RefreshCw} body="rows">
        <FormRow label={t('app_maintenance.updates.current', { version: update.current_version })}
          description={!update.supported ? t('app_maintenance.updates.unsupported') : upToDate && !update.available ? t('app_maintenance.updates.up_to_date') : undefined}>
          {#if update.supported && !update.available}
            <Button size="sm" icon={RefreshCw} loading={checkingUpdate} onclick={checkUpdate}>{t('app_maintenance.updates.check')}</Button>
          {/if}
        </FormRow>
        {#if update.supported && update.last_error}
          <FormRow label={td(update.last_error.message_key)} class="row-error" />
        {/if}
        {#if update.supported && update.available}
          <FormRow label={t('app_maintenance.updates.available', { version: update.available.version })}
            description={update.phase === 'downloading' ? t('app_maintenance.updates.downloading') : update.phase === 'idle' ? undefined : t('app_maintenance.updates.ready')}>
            {#if update.phase === 'idle'}
              <Button size="sm" variant="primary" icon={Download} onclick={download}>{t('app_maintenance.updates.install')}</Button>
            {:else if update.phase === 'downloading'}
              <Button size="sm" variant="primary" loading disabled>{t('app_maintenance.updates.install')}</Button>
            {:else}
              <Button size="sm" variant="primary" onclick={apply}>{t('app_maintenance.updates.apply')}</Button>
            {/if}
          </FormRow>
          {#if update.available.notes}
            <FormRow label={t('app_maintenance.updates.notes')} stack>
              <pre class="notes">{update.available.notes}</pre>
            </FormRow>
          {/if}
        {/if}
      </SettingsSection>
    {/if}

    <SettingsSection title={t('app_maintenance.logs.heading')} icon={ScrollText} description={t('app_maintenance.logs.intro')} body="rows">
      {#if status.log_dir}
        <FormRow label={t('app_maintenance.log_dir')} description={t('app_maintenance.logs.files', { count: status.logs.length, size: formatSize(logBytes) })}>
          <code class="path">{status.log_dir}</code>
        </FormRow>
      {:else}
        <FormRow label={t('app_maintenance.no_log')} />
      {/if}
      <FormRow label={t('app_maintenance.logs.bundle')}>
        <Button size="sm" icon={Download} onclick={exportLogs}>{t('app_maintenance.logs.export')}</Button>
      </FormRow>
    </SettingsSection>

    <SettingsSection title={t('app_maintenance.integrity.heading')} icon={Database} body="rows">
      {#if status.data_dir}
        <FormRow label={t('app_maintenance.data_dir')}><code class="path">{status.data_dir}</code></FormRow>
      {/if}
      <FormRow label={t('app_maintenance.integrity.status')}
        description={!integrity ? t('app_maintenance.integrity.never') : undefined}>
        <Button size="sm" loading={checking} onclick={checkDb}>{t('app_maintenance.integrity.check')}</Button>
      </FormRow>
      {#if integrity}
        <FormRow label={t('maintenance.results')} stack>
          {#if integrity.ok}
            <p class="ok"><Icon icon={CircleCheck} size={16} /><span>{t('app_maintenance.integrity.ok', { ms: integrity.duration_ms })}</span></p>
          {:else}
            <p class="bad"><Icon icon={CircleAlert} size={16} /><span>{t('app_maintenance.integrity.problems', { count: integrity.problems.length })}</span></p>
            <ul class="mono">
              {#each integrity.problems as p, i (i)}<li><code>{p}</code></li>{/each}
            </ul>
          {/if}
        </FormRow>
        {#if !integrity.ok}
          <FormRow label={t('app_maintenance.rescue.label')} description={t('app_maintenance.rescue.intro')} stack>
            {#if rescueSent}
              <p class="ok" role="status"><Icon icon={RefreshCw} size={16} /><span>{t('app_maintenance.rescue.restarting')}</span></p>
            {:else if rescueAsk}
              <p class="ask" role="alert"><Icon icon={TriangleAlert} size={16} /><span>{rescueAsk === 'backup' ? t('app_maintenance.rescue.confirm_backup') : t('app_maintenance.rescue.confirm_readable')}</span></p>
              <div class="actions">
                <Button size="sm" variant="primary" onclick={() => void requestRescue(rescueAsk!)}>{t('app_maintenance.rescue.restart')}</Button>
                <Button size="sm" variant="ghost" onclick={() => (rescueAsk = null)}>{t('app_maintenance.rescue.cancel')}</Button>
              </div>
            {:else}
              <div class="actions">
                <Button size="sm" icon={LifeBuoy} onclick={() => (rescueAsk = 'backup')}>{t('app_maintenance.rescue.backup')}</Button>
                <Button size="sm" onclick={() => (rescueAsk = 'readable')}>{t('app_maintenance.rescue.readable')}</Button>
              </div>
            {/if}
          </FormRow>
        {/if}
      {/if}
      {#if status.rescue}
        <FormRow label={t('app_maintenance.rescue.last')} stack>
          {#if status.rescue.ok}
            <p class="ok"><Icon icon={CircleCheck} size={16} /><span>{status.rescue.mode === 'backup' ? t('app_maintenance.rescue.done_backup') : t('app_maintenance.rescue.done_readable')}</span></p>
            {#if status.rescue.kept_in}<code class="path kept">{status.rescue.kept_in}</code>{/if}
          {:else}
            <p class="bad"><Icon icon={CircleAlert} size={16} /><span>{t('app_maintenance.rescue.failed')}{#if status.rescue.error} {td(status.rescue.error.message_key)}{/if}</span></p>
          {/if}
        </FormRow>
      {/if}
    </SettingsSection>

    <SettingsSection title={t('app_maintenance.crashes.heading')} icon={TriangleAlert} body="rows">
      {#snippet actions()}
        {#if status && status.crash_reports.length > 0}
          <Button size="sm" variant="danger" icon={Trash2} onclick={clearCrashes}>{t('app_maintenance.crashes.clear')}</Button>
        {/if}
      {/snippet}
      {#if status.crash_reports.length === 0}
        <p class="muted">{t('app_maintenance.crashes.none')}</p>
      {:else}
        <ul class="crashes">
          {#each status.crash_reports as c (c.file)}
            <li>
              <span class="msg">{c.message}</span>
              <span class="meta num" title={c.time}>{when(c.time)} · {c.version}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </SettingsSection>
  {/if}
</SettingsPage>

<style>
  .notice {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid color-mix(in srgb, var(--success) 35%, var(--border));
    border-radius: var(--radius-control);
    background: var(--success-soft);
    color: var(--success);
    font-size: var(--text-sm);
  }
  .notice :global(svg),
  .ok :global(svg),
  .bad :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .path {
    display: inline-block;
    max-width: 24rem;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
    overflow-wrap: anywhere;
    text-align: end;
    user-select: all;
  }
  .notes {
    margin: 0;
    width: 100%;
    white-space: pre-wrap;
    font: inherit;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .ok,
  .bad {
    display: flex;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
  }
  .ok {
    color: var(--success);
  }
  .ask {
    display: flex;
    gap: var(--space-2);
    margin: 0;
    color: var(--warning);
    font-size: var(--text-sm);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .kept {
    display: block;
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--danger);
  }
  .mono {
    margin: var(--space-2) 0 0;
    padding-left: var(--space-6);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .muted {
    margin: 0;
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .crashes {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .crashes li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-4);
    min-height: var(--row-h);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .msg {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .meta {
    flex: none;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  :global(.row-error) :global(.label) {
    color: var(--danger);
  }
  @media (max-width: 639.98px) {
    .path {
      text-align: start;
    }
  }
</style>
