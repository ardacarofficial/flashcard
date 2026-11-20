<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Download, HardDrive, Printer, Tabs, Upload } from '$lib/ui';
  import { SettingsPage } from '../core/kit';
  import type { ScreenCtx } from '../types';
  import BackupPanel from './BackupPanel.svelte';
  import ExportPanel from './ExportPanel.svelte';
  import ImportPanel from './ImportPanel.svelte';
  import PrintPanel from './PrintPanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const ICONS = { import: Upload, export: Download, backup: HardDrive, print: Printer } as const;
  const TABS = ['import', 'export', 'backup', 'print'] as const;
  type Tab = (typeof TABS)[number];
  let tab = $state<Tab>('import');
  const tabs = $derived(TABS.map((id) => ({ id, label: t(`io.tabs.${id}`), icon: ICONS[id] })));
</script>

<SettingsPage title={t('nav.io')} subtitle={t('io.intro')} width="wide">
  <Tabs label={t('nav.io')} {tabs} value={tab} onchange={(id) => (tab = id as Tab)}>
    {#snippet children(id)}
      <div class="panel">
        {#if id === 'import'}
          <ImportPanel {ctx} />
        {:else if id === 'export'}
          <ExportPanel {ctx} />
        {:else if id === 'backup'}
          <BackupPanel />
        {:else}
          <PrintPanel {ctx} />
        {/if}
      </div>
    {/snippet}
  </Tabs>
</SettingsPage>

<style>
  .panel {
    padding-top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
</style>
