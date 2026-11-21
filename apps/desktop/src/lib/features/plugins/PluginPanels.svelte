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
    margin-top: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    font-size: var(--text-sm);
  }
  .panel summary {
    display: flex;
    align-items: center;
    min-height: var(--row-h);
    padding: 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    cursor: pointer;
    list-style-position: inside;
  }
  .panel summary:hover {
    color: var(--text-1);
  }
  .panel summary:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
    border-radius: var(--radius-card);
  }
  .panel[open] summary {
    border-bottom: 1px solid var(--border);
  }
  .panel :global(.plugin-view) {
    padding: var(--space-3);
  }
</style>
