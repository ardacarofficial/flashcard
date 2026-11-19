<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import BackupPanel from './BackupPanel.svelte';
  import ExportPanel from './ExportPanel.svelte';
  import ImportPanel from './ImportPanel.svelte';
  import PrintPanel from './PrintPanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const TABS = ['import', 'export', 'backup', 'print'] as const;
  type Tab = (typeof TABS)[number];
  let tab = $state<Tab>('import');
</script>

<section>
  <div class="tabs" role="tablist">
    {#each TABS as id (id)}
      <button role="tab" aria-selected={tab === id} class:active={tab === id} onclick={() => (tab = id)}>
        {t(`io.tabs.${id}`)}
      </button>
    {/each}
  </div>
  <div role="tabpanel">
    {#if tab === 'import'}
      <ImportPanel {ctx} />
    {:else if tab === 'export'}
      <ExportPanel {ctx} />
    {:else if tab === 'backup'}
      <BackupPanel />
    {:else}
      <PrintPanel {ctx} />
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
