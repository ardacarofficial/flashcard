<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ThemePresetDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { theme, THEME_MODES, type ThemeMode } from '$lib/theme/theme.svelte';

  /** Appearance settings (docs/02 §31): scheme, preset, accent. Every change is `themes.set`. */
  const MODE_LABEL = {
    system: 'settings.theme_system',
    light: 'settings.theme_light',
    dark: 'settings.theme_dark',
  } as const satisfies Record<ThemeMode, string>;

  let presets = $state<ThemePresetDto[]>([]);
  let error = $state<string | null>(null);
  const current = $derived(theme.current);

  onMount(() => {
    theme.start();
    void api
      .call('themes.presets', {})
      .then((r) => (presets = r.presets))
      .catch(() => (presets = []));
  });

  async function change(c: { mode?: ThemeMode; preset?: string; accent?: string }) {
    error = null;
    try {
      await theme.set({ mode: c.mode ?? null, preset: c.preset ?? null, accent: c.accent ?? null });
    } catch (e) {
      error = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }
</script>

<div class="appearance">
  <label>
    {t('themes.mode_label')}
    <select value={theme.mode} onchange={(e) => void change({ mode: e.currentTarget.value as ThemeMode })}>
      {#each THEME_MODES as m (m)}
        <option value={m}>{t(MODE_LABEL[m])}</option>
      {/each}
    </select>
  </label>

  {#if current && presets.length > 0}
    <fieldset>
      <legend>{t('themes.preset_label')}</legend>
      <div class="presets">
        {#each presets as p (p.id)}
          <label class="preset" class:chosen={p.id === current.preset}>
            <input
              type="radio"
              name="theme-preset"
              value={p.id}
              checked={p.id === current.preset}
              onchange={() => void change({ preset: p.id })}
            />
            <span class="swatch" aria-hidden="true">
              <span style:background={p.light['bg']} style:color={p.light['accent']}>●</span>
              <span style:background={p.dark['bg']} style:color={p.dark['accent']}>●</span>
            </span>
            {td(p.name_key)}
          </label>
        {/each}
      </div>
    </fieldset>

    <fieldset>
      <legend>{t('themes.accent_label')}</legend>
      <div class="accent">
        <input
          type="color"
          aria-label={t('themes.accent_custom')}
          value={current.accent ?? current.light['accent']}
          onchange={(e) => void change({ accent: e.currentTarget.value })}
        />
        {#if current.accent}
          <button type="button" onclick={() => void change({ accent: '' })}>{t('themes.accent_reset')}</button>
        {/if}
      </div>
    </fieldset>

    <div class="preview">
      <p>{t('themes.preview_text')}</p>
      <button type="button" class="primary" tabindex="-1">{t('themes.preview_button')}</button>
    </div>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .appearance {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-3);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  fieldset {
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-2) var(--fc-space-3);
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-2);
  }
  .preset {
    flex-direction: row;
    align-items: center;
    gap: var(--fc-space-2);
    padding: var(--fc-space-1) var(--fc-space-2);
    border-radius: var(--fc-radius);
  }
  .preset.chosen {
    background: var(--fc-selected);
  }
  .swatch {
    display: inline-flex;
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    overflow: hidden;
  }
  .swatch span {
    padding: 0 var(--fc-space-1);
  }
  .accent {
    display: flex;
    align-items: center;
    gap: var(--fc-space-2);
  }
  .preview {
    background: var(--fc-surface);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
  }
  .primary {
    background: var(--fc-accent);
    color: var(--fc-accent-text);
    border: none;
  }
</style>
