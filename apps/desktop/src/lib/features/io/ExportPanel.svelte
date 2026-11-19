<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { ExportFormat, ExportReport, JobDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { pickSavePath, safeName } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';

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

<div class="panel">
  <label>
    {t('io.export.format')}
    <select bind:value={format}>
      {#each FORMATS as f (f)}<option value={f}>{t(`io.export_formats.${f}`)}</option>{/each}
    </select>
  </label>
  <p class="muted">
    {wholeCollection ? t('io.export.scope_all') : t('io.export.scope', { deck: ctx.selectedPath ?? '' })}
  </p>
  {#if hasHistory}
    <label><input type="checkbox" bind:checked={includeHistory} /> {t('io.export.include_history')}</label>
  {/if}
  {#if job}
    <JobProgress {job} />
  {:else}
    <button class="primary" onclick={run}>{t('io.export.start')}</button>
  {/if}
  {#if message}<p role="status">{message}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    align-items: flex-start;
  }
</style>
