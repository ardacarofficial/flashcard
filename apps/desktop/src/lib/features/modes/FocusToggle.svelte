<script lang="ts">
  /**
   * Focus mode (docs/02 §6.2): full screen and the counters hidden (both by setting). Since
   * v1.2 a running session hides the shell on its own; focus mode also hides the sidebar the
   * session bar may have brought back. Presentation only; the switch is the `focus` module,
   * the options its settings. Key: F.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { t } from '$lib/i18n/index.svelte';
  import { Button } from '$lib/ui';
  import { moduleOn } from '../cards/modules.svelte';
  import { plainKey } from './modes';

  let { active = $bindable(false) }: { active?: boolean } = $props();

  let fullscreen = true;
  let hideCounts = true;

  onMount(() => {
    void api
      .call('settings.get', { scope: { kind: 'app' }, keys: ['focus.fullscreen', 'focus.hide_counts', 'focus.auto'] })
      .then(
        (r) => {
          for (const v of r.values) {
            if (v.key === 'focus.fullscreen' && typeof v.value === 'boolean') fullscreen = v.value;
            if (v.key === 'focus.hide_counts' && typeof v.value === 'boolean') hideCounts = v.value;
            if (v.key === 'focus.auto' && v.value === true && moduleOn('focus')) active = true;
          }
        },
        () => {},
      );
    return () => apply(false);
  });

  function apply(on: boolean) {
    const root = document.documentElement;
    root.classList.toggle('fc-focus', on);
    root.classList.toggle('fc-focus-hide-counts', on && hideCounts);
    try {
      if (on && fullscreen && !document.fullscreenElement) void root.requestFullscreen?.().catch(() => {});
      if (!on && document.fullscreenElement) void document.exitFullscreen?.().catch(() => {});
    } catch {
      /* full screen is best effort */
    }
  }

  $effect(() => apply(active && moduleOn('focus')));

  function onKey(e: KeyboardEvent) {
    if (!moduleOn('focus') || !plainKey(e) || e.key.toLowerCase() !== 'f') return;
    e.preventDefault();
    active = !active;
  }
</script>

<svelte:window onkeydown={onKey} />

{#if moduleOn('focus')}
  <Button variant="ghost" size="sm" kbd="F" aria-keyshortcuts="F" onclick={() => (active = !active)} aria-pressed={active} data-testid="focus-toggle">
    {active ? t('modes.focus.exit') : t('modes.focus.enter')}
  </Button>
{/if}

<style>
  :global(html.fc-focus nav),
  :global(html.fc-focus .deck-pick),
  :global(html.fc-focus .section-tabs),
  :global(html.fc-focus-hide-counts [data-focus-hide]) {
    display: none !important;
  }
  :global(html.fc-focus .shell) {
    grid-template-columns: 1fr !important;
  }
</style>
