<!--
  Settings → General (docs/11 §2): language and appearance. Other groups' pages are their
  modules' own screens (navigation.ts).
-->
<script lang="ts">
  import { i18n, LOCALES, setLocale, t, type Locale } from '$lib/i18n/index.svelte';
  import { Languages, Palette, SegmentedControl } from '$lib/ui';
  import ThemeSettings from '../themes/ThemeSettings.svelte';
  import { FormRow, SettingsPage, SettingsSection } from './kit';
  import type { ScreenCtx } from '../types';

  let { ctx: _ctx }: { ctx: ScreenCtx } = $props();

  const languages = $derived(LOCALES.map((l) => ({ value: l, label: t(`lang.${l}`) })));
</script>

<SettingsPage title={t('settings.pages.general')} subtitle={t('settings.general_intro')}>
  <SettingsSection title={t('settings.language')} icon={Languages} body="rows">
    <FormRow label={t('settings.app.locale.label')} description={t('settings.app.locale.description')}>
      <SegmentedControl label={t('settings.language')} options={languages} value={i18n.locale} onchange={(l: Locale) => void setLocale(l)} />
    </FormRow>
  </SettingsSection>
  <SettingsSection title={t('themes.heading')} icon={Palette} body="rows">
    <ThemeSettings />
  </SettingsSection>
</SettingsPage>
