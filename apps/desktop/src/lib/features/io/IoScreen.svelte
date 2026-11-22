<!--
  Settings › Data › Export & backup (v1.3): export, backup / restore and print, each with a
  sentence on what it is for. Importing cards moved to "Add cards" (paste / from a file): a
  link here takes you there.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { deckPaths } from '$lib/components/deck-paths';
  import { Button, ChevronRight, Download, HardDrive, Icon, Printer, Select, Tabs, Upload } from '$lib/ui';
  import { SettingsPage } from '../core/kit';
  import { requestAddWay } from '../notes/add-way.svelte';
  import type { ScreenCtx } from '../types';
  import BackupPanel from './BackupPanel.svelte';
  import ExportPanel from './ExportPanel.svelte';
  import PrintPanel from './PrintPanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const ICONS = { export: Download, backup: HardDrive, print: Printer } as const;
  const TABS = ['export', 'backup', 'print'] as const;
  type Tab = (typeof TABS)[number];
  let tab = $state<Tab>('export');
  // The deck export / print work on (the shell's selected deck), picked here. No deck means the
  // whole collection on Export, "choose one" on Print.
  const deckOptions = $derived([
    { value: null as string | null, label: t(tab === 'export' ? 'deck.all' : 'deck.pick') },
    ...deckPaths(ctx.decks).map((p) => ({ value: p.id as string | null, label: p.path })),
  ]);
  const tabs = $derived(TABS.map((id) => ({ id, label: t(`io.tabs.${id}`), icon: ICONS[id] })));

  function toImport() {
    requestAddWay('file', ctx.selectedDeck);
    ctx.navigate?.('/notes/new');
  }
</script>

<SettingsPage title={t('nav.io')} subtitle={t('io.intro')} width="wide">
  {#snippet actions()}
    {#if tab !== 'backup'}
      <div class="deck"><Select label={t('deck.current')} hideLabel options={deckOptions} value={ctx.selectedDeck} onchange={(id) => ctx.selectDeck(id)} /></div>
    {/if}
  {/snippet}
  {#if ctx.navigate}
    <div class="import-link">
      <span class="icon" aria-hidden="true"><Icon icon={Upload} size={16} /></span>
      <div class="text">
        <strong>{t('io.data.import_title')}</strong>
        <span>{t('io.data.import_hint')}</span>
      </div>
      <Button size="sm" iconEnd={ChevronRight} onclick={toImport}>{t('io.data.import_go')}</Button>
    </div>
  {/if}
  <Tabs variant="pill" label={t('nav.io')} {tabs} value={tab} onchange={(id) => (tab = id as Tab)}>
    {#snippet children(id)}
      <div class="panel">
        <p class="purpose">{t(`io.data.${id as Tab}_hint`)}</p>
        {#if id === 'export'}
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
  .import-link {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    margin-bottom: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .icon {
    display: inline-flex;
    color: var(--accent-text);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 18rem;
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .text strong {
    color: var(--text-1);
    font-weight: var(--weight-semibold);
  }
  .panel {
    padding-top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .purpose {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-md);
  }
</style>
