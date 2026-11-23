<!--
  Settings › Advanced › Studying › Words (v1.4): when a word counts as known, the language of
  the cards (app-wide; per deck on the deck page) with the hint for notes whose deck has no
  language, and the installed word data (frequency / level packs) with "put the shipped
  one back". Every value and count comes from the core (`settings.*`, `words.stats`,
  `words.data.*`); a switched-off module's part is not shown.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { WordDataSources } from '$lib/api/types';
  import { formatCount, formatDateTime } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import { BookOpen, Button, Database, RotateCcw } from '$lib/ui';
  import { FormRow, SchemaRows, SettingsPage, SettingsSection } from './kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let unassigned = $state<number | null>(null);
  let sources = $state<WordDataSources | null>(null);

  async function load() {
    unassigned = await api.call('words.stats', {}).then(
      (s) => s.unassigned_notes,
      () => null,
    );
    sources = await api.call('words.data.sources', {}).catch(() => null);
  }
  onMount(() => void load());

  async function reset(language: string) {
    const ok = await ctx.guarded(async () => (sources = await api.call('words.data.reset', { language })));
    if (ok) void load();
  }
</script>

<SettingsPage title={t('settings_pages.words.title')} subtitle={t('settings_pages.words.intro')}>
  <SettingsSection title={t('settings_pages.words.known')} icon={BookOpen} body="rows">
    <SchemaRows keys={['words.known_interval_days', 'words.language']} guarded={ctx.guarded} onsaved={() => void load()} />
    {#if unassigned !== null && unassigned > 0}
      <p class="hint" data-testid="words-unassigned">
        {t('settings_pages.words.unassigned', { count: unassigned, n: formatCount(unassigned) })}
        {#if ctx.navigate}
          <button type="button" class="link" onclick={() => ctx.navigate?.('/decks')}>{t('settings_pages.words.open_decks')}</button>
        {/if}
      </p>
    {/if}
  </SettingsSection>
  {#if sources}
    <SettingsSection title={t('settings_pages.words.data')} description={t('settings_pages.words.data_hint')} icon={Database} body="rows">
      {#each sources.packs as p (p.language)}
        <FormRow
          label={`${p.name} (${p.language})`}
          description={t('settings_pages.words.pack', { words: formatCount(p.words), origin: t(`settings_pages.words.origin.${p.origin === 'imported' ? 'imported' : 'bundled'}`), date: formatDateTime(p.installed_at) })}
        >
          {#if p.origin === 'imported'}
            <Button size="sm" variant="secondary" icon={RotateCcw} onclick={() => void reset(p.language)}>{t('settings_pages.words.reset')}</Button>
          {/if}
        </FormRow>
      {:else}
        <p class="hint">{t('settings_pages.words.no_packs')}</p>
      {/each}
    </SettingsSection>
  {/if}
</SettingsPage>

<style>
  .hint {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .link {
    margin-inline-start: var(--space-1);
    padding: 0;
    border: none;
    background: none;
    color: var(--accent-text, var(--accent));
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }
</style>
