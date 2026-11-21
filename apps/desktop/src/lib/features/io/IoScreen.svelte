<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { deckPaths } from '$lib/components/deck-paths';
  import { Download, HardDrive, Printer, Select, Tabs, Upload } from '$lib/ui';
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
  // The deck import / export / print work on (the shell's selected deck), picked here.
  const deckOptions = $derived([
    { value: null as string | null, label: t('deck.select_prompt') },
    ...deckPaths(ctx.decks).map((p) => ({ value: p.id as string | null, label: p.path })),
  ]);
  const tabs = $derived(TABS.map((id) => ({ id, label: t(`io.tabs.${id}`), icon: ICONS[id] })));
</script>

<SettingsPage title={t('nav.io')} subtitle={t('io.intro')} width="wide">
  {#snippet actions()}
    <div class="deck"><Select label={t('deck.current')} hideLabel options={deckOptions} value={ctx.selectedDeck} onchange={(id) => ctx.selectDeck(id)} /></div>
  {/snippet}
  <Tabs variant="pill" label={t('nav.io')} {tabs} value={tab} onchange={(id) => (tab = id as Tab)}>
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
  .deck {
    min-width: 16rem;
  }
  .panel {
    padding-top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
</style>
