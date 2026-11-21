<!--
  Appearance settings (docs/02 §31): scheme, preset, accent — rows of a Settings section.
  Every change is `themes.set` (the backend owns the theme).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ThemePresetDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { theme, THEME_MODES, type ThemeMode } from '$lib/theme/theme.svelte';
  import { Button, Check, Icon, Input, Monitor, Moon, RotateCcw, SegmentedControl, Skeleton, Sun } from '$lib/ui';
  import FormRow from '../core/kit/FormRow.svelte';

  const MODE_LABEL = {
    system: 'settings.theme_system',
    light: 'settings.theme_light',
    dark: 'settings.theme_dark',
  } as const satisfies Record<ThemeMode, string>;
  const MODE_ICON = { system: Monitor, light: Sun, dark: Moon } as const;

  let presets = $state<ThemePresetDto[] | null>(null);
  let error = $state<string | null>(null);
  const current = $derived(theme.current);
  const modes = $derived(THEME_MODES.map((m) => ({ value: m, label: t(MODE_LABEL[m]), icon: MODE_ICON[m] })));
  let radios = $state<HTMLButtonElement[]>([]);

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

  /** Arrow keys move through the preset swatches (one tab stop, like a radio group). */
  function onkey(e: KeyboardEvent, i: number) {
    const list = presets ?? [];
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!dir || list.length === 0) return;
    e.preventDefault();
    const next = (i + dir + list.length) % list.length;
    radios[next]?.focus();
    void change({ preset: list[next]!.id });
  }
</script>

<FormRow label={t('themes.mode_label')} description={t('settings.app.theme.description')}>
  <SegmentedControl label={t('themes.mode_label')} options={modes} value={theme.mode} onchange={(m) => void change({ mode: m })} />
</FormRow>

<FormRow label={t('themes.preset_label')} description={t('settings.themes.preset.description')} stack>
  {#if presets === null}
    <Skeleton shape="control" width="100%" />
  {:else if current && presets.length > 0}
    <div class="presets" role="radiogroup" aria-label={t('themes.preset_label')}>
      {#each presets as p, i (p.id)}
        {@const chosen = p.id === current.preset}
        <button
          type="button"
          role="radio"
          aria-checked={chosen}
          tabindex={chosen || (!presets.some((x) => x.id === current.preset) && i === 0) ? 0 : -1}
          class="preset"
          class:chosen
          bind:this={radios[i]}
          onclick={() => void change({ preset: p.id })}
          onkeydown={(e) => onkey(e, i)}
        >
          <span class="swatch" aria-hidden="true">
            <span class="half" style:background={p.light['bg']}><span class="dot" style:background={p.light['accent']}></span></span>
            <span class="half" style:background={p.dark['bg']}><span class="dot" style:background={p.dark['accent']}></span></span>
          </span>
          <span class="name">{td(p.name_key)}</span>
          {#if chosen}<span class="tick" aria-hidden="true"><Icon icon={Check} size={14} /></span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</FormRow>

{#if current}
  <FormRow label={t('themes.accent_label')} description={t('themes.preview_text')} labelFor="fc-accent-picker">
    <div class="accent">
      <Input
        id="fc-accent-picker"
        type="color"
        hideLabel
        label={t('themes.accent_custom')}
        value={current.accent ?? current.light['accent']}
        onchange={(e: Event) => void change({ accent: (e.currentTarget as HTMLInputElement).value })}
      />
      {#if current.accent}
        <Button variant="ghost" size="sm" icon={RotateCcw} onclick={() => void change({ accent: '' })}>{t('themes.accent_reset')}</Button>
      {/if}
    </div>
  </FormRow>
{/if}

{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  .presets {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
    gap: var(--space-2);
    width: 100%;
  }
  .preset {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-h-lg);
    padding: 0 var(--space-2) 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background var(--motion-fast) var(--ease);
  }
  .preset:hover {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }
  .preset.chosen {
    border-color: var(--accent-text);
    background: var(--accent-soft);
  }
  .preset:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .swatch {
    display: grid;
    grid-template-columns: 1fr 1fr;
    flex: none;
    width: 40px;
    height: 22px;
    border-radius: 4px;
    overflow: hidden;
    border: 1px solid var(--border);
  }
  .half {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: var(--radius-pill);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: var(--weight-medium);
  }
  .tick {
    display: inline-flex;
    flex: none;
    color: var(--accent-text);
  }
  .accent {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  /* Only the swatch: it lines up with the other rows' controls on the right edge. */
  .accent > :global(.fc-field) {
    min-width: 0;
    width: auto;
  }
  .error {
    margin: 0;
    padding: var(--space-2) 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
