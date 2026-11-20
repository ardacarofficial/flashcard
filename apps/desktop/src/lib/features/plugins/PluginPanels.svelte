<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ModuleDto } from '$lib/api/types';
  import { td } from '$lib/i18n/index.svelte';
  import PluginView from './PluginView.svelte';
  import { contributions, loadPluginTexts, parseViewRoute } from './texts.svelte';

  /** Sidebar panels contributed by plugins (`sidebar_panel` slot); also keeps plugin texts loaded. */
  let { modules }: { modules: ModuleDto[] } = $props();
  const panels = $derived(
    contributions(modules, 'sidebar_panel').flatMap((c) => {
      const target = parseViewRoute(c.route);
      return target ? [{ ...target, id: c.id, label_key: c.label_key }] : [];
    }),
  );

  onMount(() => {
    const enabled = modules.length === 0 || modules.some((m) => m.id === 'plugins' && m.enabled);
    if (enabled) void loadPluginTexts().catch(() => {});
    const unsub = api.on('plugins_changed', () => void loadPluginTexts().catch(() => {}));
    return () => void unsub.then((u) => u());
  });
</script>

{#each panels as p (p.id)}
  <details class="panel">
    <summary>{td(p.label_key)}</summary>
    <PluginView pluginId={p.pluginId} view={p.view} />
  </details>
{/each}

<style>
  .panel {
    margin-top: var(--fc-space-4);
    font-size: 0.9em;
  }
</style>
