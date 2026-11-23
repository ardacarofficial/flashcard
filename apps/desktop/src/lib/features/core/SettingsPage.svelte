<!--
  Settings → General (docs/11 §2, v1.3): where a first-time user lands in Settings. It starts
  with "What do you want to change?" — the everyday questions, each opening the page that
  answers it (only pages whose module is on) — then language and appearance. Other groups'
  pages are their modules' own screens (navigation.ts).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { i18n, LOCALES, setLocale, t, type Locale, type MessageKey } from '$lib/i18n/index.svelte';
  import { ChevronRight, Icon, Languages, LifeBuoy, Palette, SegmentedControl } from '$lib/ui';
  import ThemeSettings from '../themes/ThemeSettings.svelte';
  import { FormRow, SettingsPage, SettingsSection } from './kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const languages = $derived(LOCALES.map((l) => ({ value: l, label: t(`lang.${l}`) })));

  /** Everyday questions → the page that answers them (shown when that page exists). */
  const QUESTIONS: { id: string; route: string; q: MessageKey; where: MessageKey }[] = [
    { id: 'new_per_day', route: '/presets', q: 'settings.guide.new_per_day', where: 'settings.guide.where.presets' },
    { id: 'reminder', route: '/reminders', q: 'settings.guide.reminder', where: 'settings.guide.where.reminders' },
    { id: 'card_back', route: '/notetypes', q: 'settings.guide.card_back', where: 'settings.guide.where.notetypes' },
    { id: 'keys', route: '/shortcuts', q: 'settings.guide.keys', where: 'settings.guide.where.shortcuts' },
    { id: 'ai', route: '/ai', q: 'settings.guide.ai', where: 'settings.guide.where.ai' },
    { id: 'subtitle', route: '/subtitle', q: 'settings.guide.subtitle', where: 'settings.guide.where.subtitle' },
    { id: 'backup', route: '/io', q: 'settings.guide.backup', where: 'settings.guide.where.io' },
  ];

  let routes = $state<Set<string> | null>(null);
  const questions = $derived(routes && ctx.navigate ? QUESTIONS.filter((x) => routes!.has(x.route)) : []);

  onMount(() => {
    void api
      .call('modules.list', {})
      .then((r) => (routes = new Set(r.modules.filter((m) => m.enabled).flatMap((m) => m.ui.flatMap((c) => (c.route ? [c.route] : []))))))
      .catch(() => (routes = new Set()));
  });
</script>

<SettingsPage title={t('shell.page.look')} subtitle={t('settings.general_intro')}>
  {#if questions.length > 0}
    <SettingsSection title={t('settings.guide.title')} description={t('settings.guide.description')} icon={LifeBuoy}>
      <ul class="guide" data-testid="settings-guide">
        {#each questions as x (x.id)}
          <li>
            <button type="button" onclick={() => ctx.navigate?.(x.route)}>
              <span class="q">{t(x.q)}</span>
              <span class="where">{t(x.where)}</span>
              <Icon icon={ChevronRight} size={14} />
            </button>
          </li>
        {/each}
      </ul>
    </SettingsSection>
  {/if}
  <SettingsSection title={t('settings.language')} icon={Languages} body="rows">
    <FormRow label={t('settings.app.locale.label')} description={t('settings.app.locale.description')}>
      <SegmentedControl label={t('settings.language')} options={languages} value={i18n.locale} onchange={(l: Locale) => void setLocale(l)} />
    </FormRow>
  </SettingsSection>
  <SettingsSection title={t('themes.heading')} icon={Palette} body="rows">
    <ThemeSettings />
  </SettingsSection>
</SettingsPage>

<style>
  .guide {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--border);
  }
  .guide button {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 40px;
    padding: var(--space-2) var(--space-2);
    border: none;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-md);
    text-align: start;
    cursor: pointer;
  }
  .guide button:hover {
    background: var(--surface-2);
  }
  .guide button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .guide :global(svg) {
    color: var(--text-3);
  }
  .where {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  @media (max-width: 640px) {
    .guide button {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .where {
      grid-column: 1;
      grid-row: 2;
    }
  }
</style>
