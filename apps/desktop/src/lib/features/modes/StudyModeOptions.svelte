<script lang="ts">
  /** Start-form options of a study session: time limit (`timed`) and rating scale (`confidence`). */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { RatingScale, StartModes, TimeoutAction } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
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
      timed && moduleOn('timed') ? { seconds, on_timeout: onTimeout } : null,
      scale !== '' && moduleOn('confidence') ? scale : null,
    );
  });
</script>

{#if moduleOn('timed') || moduleOn('confidence')}
  <div class="row" data-testid="mode-options">
    {#if moduleOn('timed')}
      <label class="row">
        <input type="checkbox" bind:checked={timed} />
        {t('modes.options.timed')}
      </label>
      {#if timed}
        <label>
          {t('modes.options.seconds')}
          <input type="number" min="3" max="600" bind:value={seconds} />
        </label>
        <label>
          {t('modes.options.on_timeout')}
          <select bind:value={onTimeout}>
            <option value="again">{t('modes.options.again')}</option>
            <option value="skip">{t('modes.options.skip')}</option>
          </select>
        </label>
      {/if}
    {/if}
    {#if moduleOn('confidence')}
      <label>
        {t('modes.options.scale')}
        <select bind:value={scale}>
          <option value="">{t('modes.options.scale_default')}</option>
          <option value="buttons">{t('modes.options.scale_buttons')}</option>
          <option value="confidence">{t('modes.options.scale_confidence')}</option>
        </select>
      </label>
    {/if}
  </div>
{/if}

<style>
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  input[type='number'] {
    width: 5em;
  }
</style>
