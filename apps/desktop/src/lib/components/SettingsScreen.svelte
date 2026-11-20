<script lang="ts">
  import { i18n, LOCALES, t, type Locale } from '$lib/i18n/index.svelte';
  import ThemeSettings from '$lib/features/themes/ThemeSettings.svelte';
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
  </div>
  <h3>{t('themes.heading')}</h3>
  <ThemeSettings />
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
