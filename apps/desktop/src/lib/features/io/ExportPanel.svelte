<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { ExportFormat, ExportReport, JobDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { pickSavePath, safeName } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';
  import { Button, CircleAlert, CircleCheck, Download, Icon, Select, Switch } from '$lib/ui';
  import { FormRow, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const FORMATS: ExportFormat[] = ['fcpack', 'apkg', 'colpkg', 'text', 'json'];
  const EXT: Record<ExportFormat, string> = {
    fcpack: 'fcpack',
    apkg: 'apkg',
    colpkg: 'colpkg',
    text: 'txt',
    json: 'json',
  };

  let format = $state<ExportFormat>('fcpack');
  let includeHistory = $state(true);
  let job = $state<JobDto | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  const wholeCollection = $derived(format === 'colpkg' || !ctx.selectedDeck);
  const formats = $derived(FORMATS.map((f) => ({ value: f, label: t(`io.export_formats.${f}`) })));
  const hasHistory = $derived(format === 'fcpack' || format === 'apkg' || format === 'colpkg');

  async function run() {
    error = null;
    message = null;
    const name = wholeCollection ? 'collection' : (ctx.selectedPath ?? 'deck');
    const path = await pickSavePath(
      `${safeName(name)}.${EXT[format]}`,
      [{ name: t(`io.export_formats.${format}`), extensions: [EXT[format]] }],
      t('io.path_prompt'),
    );
    if (!path) return;
    try {
      const started = await api.call('io.export.start', {
        format,
        path,
        deck_id: wholeCollection ? null : ctx.selectedDeck,
        include_history: hasHistory ? includeHistory : null,
        text_options: null,
      });
      job = started;
      const done = await followJob(started, (j) => (job = j));
      job = null;
      const r = resultOf<ExportReport>(done);
      if (r) message = t('io.export.done', { notes: r.notes, media: r.media, path: r.path });
      else if (done.status === 'cancelled') message = t('io.job.cancelled');
      else if (done.error) error = td(done.error.message_key);
    } catch (e) {
      job = null;
      error = td((e as ApiError).messageKey);
    }
  }
</script>

<SettingsSection title={t('io.tabs.export')} description={wholeCollection ? t('io.export.scope_all') : t('io.export.scope', { deck: ctx.selectedPath ?? '' })} icon={Download} body="rows">
  <FormRow label={t('io.export.format')} description={t('io.export.format_hint')} labelFor="fc-export-format">
    <Select id="fc-export-format" hideLabel label={t('io.export.format')} options={formats} bind:value={format} />
  </FormRow>
  {#if hasHistory}
    <FormRow label={t('io.export.include_history')} description={t('io.export.include_history_hint')}>
      <Switch ariaLabel={t('io.export.include_history')} bind:checked={includeHistory} />
    </FormRow>
  {/if}
  <div class="foot">
    {#if job}
      <JobProgress {job} />
    {:else}
      <Button variant="primary" icon={Download} onclick={run}>{t('io.export.start')}</Button>
    {/if}
    {#if message}<p class="note ok" role="status"><Icon icon={CircleCheck} size={16} /><span>{message}</span></p>{/if}
    {#if error}<p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}
  </div>
</SettingsSection>

<style>
  .foot {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-6) var(--space-4);
    border-top: 1px solid var(--border);
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
  @media (max-width: 1023.98px) {
    .foot {
      padding-inline: var(--space-4);
    }
  }
</style>
