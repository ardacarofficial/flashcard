<!--
  AI (Create section and Settings → Connections): make cards, providers, usage — one page with
  tabs. Lane S3 owns the "Make cards" panel; this container and the two settings panels are S4's.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Activity, Bot, PageHeader, Sparkles, Tabs } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import GeneratePanel from './GeneratePanel.svelte';
  import ProvidersPanel from './ProvidersPanel.svelte';
  import UsagePanel from './UsagePanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let tab = $state<string>('generate');
  const tabs = $derived([
    { id: 'generate', label: t('ai.tabs.generate'), icon: Sparkles },
    { id: 'providers', label: t('ai.tabs.providers'), icon: Bot },
    { id: 'usage', label: t('ai.tabs.usage'), icon: Activity },
  ]);
</script>

<div class="ai">
  <PageHeader title={t('nav.ai')} subtitle={t('ai.subtitle')} />
  <Tabs label={t('nav.ai')} {tabs} bind:value={tab}>
    {#snippet children(id)}
      <div class="panel">
        {#if id === 'generate'}
          <GeneratePanel {ctx} />
        {:else if id === 'providers'}
          <ProvidersPanel {ctx} />
        {:else}
          <UsagePanel />
        {/if}
      </div>
    {/snippet}
  </Tabs>
</div>

<style>
  .panel {
    padding-top: var(--space-2);
  }
</style>
