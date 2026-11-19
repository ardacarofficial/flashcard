<script lang="ts">
  import { i18n, LOCALES, t, type Locale } from '$lib/i18n/index.svelte';
  import { theme, THEME_MODES, type ThemeMode } from '$lib/theme/theme.svelte';

  const THEME_LABEL = {
    system: 'settings.theme_system',
    light: 'settings.theme_light',
    dark: 'settings.theme_dark',
  } as const satisfies Record<ThemeMode, string>;
</script>

<section>
  <h2>{t('settings.heading')}</h2>
  <div class="form">
    <label>
      {t('settings.language')}
      <select value={i18n.locale} onchange={(e) => (i18n.locale = e.currentTarget.value as Locale)}>
        {#each LOCALES as l (l)}
          <option value={l}>{t(`lang.${l}`)}</option>
        {/each}
      </select>
    </label>
    <label>
      {t('settings.theme')}
      <select value={theme.mode} onchange={(e) => (theme.mode = e.currentTarget.value as ThemeMode)}>
        {#each THEME_MODES as m (m)}
          <option value={m}>{t(THEME_LABEL[m])}</option>
        {/each}
      </select>
    </label>
  </div>
</section>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
    max-width: 320px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
</style>
