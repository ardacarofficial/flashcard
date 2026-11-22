<!--
  AI, shown in two places (v1.3) — each shows the part that belongs there:
  * Create › AI: making cards from a text (an add-cards path; the deck is chosen in the form).
  * Settings › Connections › AI: setting up providers and seeing what they cost (usage).
  Every part stays reachable: Create links to the provider setup when none is set up yet, and
  the settings page says where AI is used.
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Activity, Bot, Button, PageHeader, Sparkles, Tabs } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import GeneratePanel from './GeneratePanel.svelte';
  import ProvidersPanel from './ProvidersPanel.svelte';
  import UsagePanel from './UsagePanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const inSettings = $derived(ctx.place === 'settings');
  let tab = $state<string>('providers');
  const tabs = $derived([
    { id: 'providers', label: t('ai.tabs.providers'), icon: Bot },
    { id: 'usage', label: t('ai.tabs.usage'), icon: Activity },
  ]);
</script>

<div class="ai">
  {#if inSettings}
    <PageHeader title={t('nav.ai')} subtitle={t('ai.settings_subtitle')}>
      {#snippet actions()}
        {#if ctx.navigate}
          <Button variant="ghost" icon={Sparkles} onclick={() => ctx.navigate?.('/ai')}>{t('ai.make_cards')}</Button>
        {/if}
      {/snippet}
    </PageHeader>
    <Tabs variant="pill" label={t('nav.ai')} {tabs} bind:value={tab}>
      {#snippet children(id)}
        <div class="panel">
          {#if id === 'providers'}
            <ProvidersPanel {ctx} />
          {:else}
            <UsagePanel />
          {/if}
        </div>
      {/snippet}
    </Tabs>
  {:else}
    <PageHeader title={t('ai.generate_title')} subtitle={t('ai.generate_subtitle')} />
    <GeneratePanel {ctx} />
  {/if}
</div>

<style>
  .ai {
    display: flex;
    flex-direction: column;
  }
  .panel {
    padding-top: var(--space-4);
  }
</style>
