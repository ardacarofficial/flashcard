<!--
  Settings → "Reading and modes" (docs/11 §2): the study modes' display options, learn /
  test lengths, timed and focus mode, and reading. Drawn from `settings.schema` (settings of
  enabled modules only), so a switched-off mode's options disappear; validation is Rust's.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SettingDef } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { BookOpen, GraduationCap, Input, Layers, Select, SegmentedControl, Switch } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';
  import { modeSettingSections } from './mode-settings';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const ICONS = { study: GraduationCap, practice: Layers, reading: BookOpen } as const;

  let sections = $state<ReturnType<typeof modeSettingSections>>([]);
  let values = $state<Record<string, unknown>>({});
  let phase = $state<'loading' | 'ready' | 'error'>('loading');

  async function load() {
    phase = 'loading';
    try {
      const schema = (await api.call('settings.schema', {})).settings;
      sections = modeSettingSections(schema);
      const keys = sections.flatMap((s) => s.defs.map((d) => d.key));
      const got = keys.length ? (await api.call('settings.get', { scope: { kind: 'app' }, keys })).values : [];
      const next: Record<string, unknown> = {};
      for (const s of sections) for (const d of s.defs) next[d.key] = d.default;
      for (const v of got) if (v.value !== null && v.value !== undefined) next[v.key] = v.value;
      values = next;
      phase = 'ready';
    } catch {
      phase = 'error';
    }
  }
  onMount(() => void load());

  async function save(def: SettingDef, value: unknown) {
    const before = values[def.key];
    values = { ...values, [def.key]: value };
    const ok = await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key: def.key, value }));
    if (!ok) values = { ...values, [def.key]: before };
  }

  const label = (d: SettingDef) => td(d.label_key);
  const description = (d: SettingDef) => (d.description_key ? td(d.description_key) : undefined);
  const choices = (d: SettingDef) =>
    d.kind.type === 'choice' ? d.kind.options.map((o) => ({ value: o, label: td(`${d.label_key.replace(/\.label$/, '')}.${o}`) })) : [];

  /** Text and number fields save when they lose focus (Enter too), not on every key. */
  function commit(def: SettingDef, raw: string) {
    if (def.kind.type === 'integer' || def.kind.type === 'float') {
      const n = Number(raw);
      if (raw.trim() === '' || !Number.isFinite(n)) return;
      if (n !== values[def.key]) void save(def, def.kind.type === 'integer' ? Math.round(n) : n);
    } else if (raw !== values[def.key]) void save(def, raw);
  }
</script>

<SettingsPage title={t('mode_settings.title')} subtitle={t('mode_settings.subtitle')}>
  {#if phase !== 'ready'}
    <LoadState state={phase} retry={load} />
  {:else}
    {#each sections as s (s.id)}
      <SettingsSection title={t(s.title_key)} icon={ICONS[s.id]} body="rows">
        {#each s.defs as d (d.key)}
          {@const id = `mode-setting-${d.key.replaceAll('.', '-')}`}
          {#if d.kind.type === 'bool'}
            <FormRow label={label(d)} description={description(d)}>
              <Switch ariaLabel={label(d)} checked={values[d.key] === true} onchange={(v) => void save(d, v)} />
            </FormRow>
          {:else if d.kind.type === 'choice'}
            <FormRow label={label(d)} description={description(d)}>
              {#if d.kind.options.length <= 3}
                <SegmentedControl size="sm" label={label(d)} options={choices(d)} value={values[d.key] as string} onchange={(v: string) => void save(d, v)} />
              {:else}
                <Select label={label(d)} hideLabel options={choices(d)} value={values[d.key] as string} onchange={(v: string) => void save(d, v)} />
              {/if}
            </FormRow>
          {:else}
            <FormRow label={label(d)} description={description(d)} labelFor={id}>
              <Input
                {id}
                size="sm"
                label={label(d)}
                hideLabel
                type={d.kind.type === 'text' ? 'text' : 'number'}
                min={d.kind.type === 'integer' || d.kind.type === 'float' ? (d.kind.min ?? undefined) : undefined}
                max={d.kind.type === 'integer' || d.kind.type === 'float' ? (d.kind.max ?? undefined) : undefined}
                value={String(values[d.key] ?? '')}
                class={d.kind.type === 'text' ? 'mode-text' : 'mode-number'}
                onchange={(e: Event) => commit(d, (e.currentTarget as HTMLInputElement).value)}
              />
            </FormRow>
          {/if}
        {/each}
      </SettingsSection>
    {:else}
      <p class="empty">{t('mode_settings.empty')}</p>
    {/each}
  {/if}
</SettingsPage>

<style>
  .empty {
    color: var(--text-2);
  }
  :global(.mode-number) {
    width: 120px;
  }
  :global(.mode-text) {
    width: 260px;
    max-width: 100%;
  }
</style>
