<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import GeneratePanel from './GeneratePanel.svelte';
  import ProvidersPanel from './ProvidersPanel.svelte';
  import UsagePanel from './UsagePanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const TABS = ['generate', 'providers', 'usage'] as const;
  type Tab = (typeof TABS)[number];
  let tab = $state<Tab>('generate');
</script>

<section>
  <div class="tabs" role="tablist">
    {#each TABS as id (id)}
      <button role="tab" aria-selected={tab === id} class:active={tab === id} onclick={() => (tab = id)}>
        {t(`ai.tabs.${id}`)}
      </button>
    {/each}
  </div>
  <div role="tabpanel">
    {#if tab === 'generate'}
      <GeneratePanel {ctx} />
    {:else if tab === 'providers'}
      <ProvidersPanel {ctx} />
    {:else}
      <UsagePanel />
    {/if}
  </div>
</section>

<style>
  .tabs {
    display: flex;
    gap: var(--fc-space-1);
    margin-bottom: var(--fc-space-3);
  }
  .tabs .active {
    background: var(--fc-accent);
    border-color: var(--fc-accent);
    color: var(--fc-accent-text);
  }
</style>
