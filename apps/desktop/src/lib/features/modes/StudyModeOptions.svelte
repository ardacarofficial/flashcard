<script lang="ts">
  /** Start-form options of a study session: time limit (`timed`) and rating scale (`confidence`). */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { RatingScale, StartModes, TimeoutAction } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Input, Select, Switch } from '$lib/ui';
  import { moduleOn } from '../cards/modules.svelte';
  import { startModes } from './modes';

  let { value = $bindable(null) }: { value?: StartModes | null } = $props();

  let timed = $state(false);
  let seconds = $state(10);
  let onTimeout = $state<TimeoutAction>('again');
  let scale = $state<RatingScale | ''>('');

  onMount(() => {
    if (!moduleOn('timed')) return;
    void api.call('settings.get', { scope: { kind: 'app' }, keys: ['timed.seconds', 'timed.on_timeout'] }).then(
      (r) => {
        for (const v of r.values) {
          if (v.key === 'timed.seconds' && typeof v.value === 'number') seconds = v.value;
          if (v.key === 'timed.on_timeout' && (v.value === 'again' || v.value === 'skip')) onTimeout = v.value;
        }
      },
      () => {},
    );
  });

  $effect(() => {
    value = startModes(
      timed && moduleOn('timed') ? { seconds: Number(seconds) || 10, on_timeout: onTimeout } : null,
      scale !== '' && moduleOn('confidence') ? scale : null,
    );
  });

  const timeouts = $derived([
    { value: 'again' as TimeoutAction, label: t('modes.options.again') },
    { value: 'skip' as TimeoutAction, label: t('modes.options.skip') },
  ]);
  const scales = $derived([
    { value: '' as RatingScale | '', label: t('modes.options.scale_default') },
    { value: 'buttons' as RatingScale | '', label: t('modes.options.scale_buttons') },
    { value: 'confidence' as RatingScale | '', label: t('modes.options.scale_confidence') },
  ]);
</script>

{#if moduleOn('timed') || moduleOn('confidence')}
  <div class="opts" data-testid="mode-options">
    {#if moduleOn('timed')}
      <Switch label={t('modes.options.timed')} description={t('modes.options.timed_hint')} bind:checked={timed} />
      {#if timed}
        <div class="row">
          <Input class="secs" type="number" min="3" max="600" label={t('modes.options.seconds')} bind:value={seconds} />
          <Select label={t('modes.options.on_timeout')} options={timeouts} bind:value={onTimeout} />
        </div>
      {/if}
    {/if}
    {#if moduleOn('confidence')}
      <div class="scale"><Select label={t('modes.options.scale')} options={scales} bind:value={scale} /></div>
    {/if}
  </div>
{/if}

<style>
  .opts {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .row {
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .scale {
    max-width: 260px;
  }
  .row :global(.secs) {
    width: 120px;
  }
</style>
