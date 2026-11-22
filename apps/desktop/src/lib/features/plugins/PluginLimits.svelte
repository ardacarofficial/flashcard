<!--
  Settings → Plugins → Advanced (v1.3): the sandbox limits every plugin runs under. They are
  ordinary app settings of the `plugins` module (`plugins.timeout_ms`, `plugins.memory_mb`…):
  drawn from `settings.schema` (labels, descriptions, ranges and defaults come from there),
  changed with `settings.set` and put back with `settings.reset` (both undoable).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SettingDef } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Input, Select } from '$lib/ui';
  import { FormRow, LoadState, isDefault, loadDefaults, resetSetting } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let defs = $state<SettingDef[] | null>(null);
  let values = $state<Record<string, unknown>>({});
  let failed = $state(false);

  async function load() {
    const ok = await ctx.guarded(async () => {
      const schema = (await api.call('settings.schema', {})).settings.filter((e) => e.module === 'plugins').map((e) => e.def);
      const got = schema.length
        ? (await api.call('settings.get', { scope: { kind: 'app' }, keys: schema.map((d) => d.key) })).values
        : [];
      values = Object.fromEntries(schema.map((d) => [d.key, got.find((v) => v.key === d.key)?.value ?? d.default]));
      defs = schema;
    });
    failed = !ok && defs === null;
  }

  async function set(def: SettingDef, value: unknown) {
    const ok = await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key: def.key, value }));
    if (ok) values[def.key] = value;
    else await load();
  }

  async function reset(def: SettingDef) {
    await ctx.guarded(async () => {
      values[def.key] = await resetSetting(def.key);
    });
  }

  const range = (def: SettingDef) =>
    def.kind.type === 'integer' && def.kind.min !== null && def.kind.max !== null
      ? t('plugins.limits.range', { min: formatCount(def.kind.min), max: formatCount(def.kind.max), default: formatCount(Number(def.default)) })
      : undefined;
  const describe = (def: SettingDef) => [def.description_key ? td(def.description_key) : '', range(def)].filter(Boolean).join(' ');

  onMount(() => {
    void loadDefaults();
    void load();
  });
</script>

{#if defs === null}
  <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={3} />
{:else if defs.length === 0}
  <p class="none">{t('plugins.limits.none')}</p>
{:else}
  <div data-testid="plugin-limits">
    {#each defs as def (def.key)}
      {@const label = td(def.label_key)}
      {@const value = values[def.key]}
      <FormRow
        {label}
        description={describe(def)}
        isDefault={isDefault(def.key, value)}
        onReset={() => void reset(def)}
        labelFor={`plugin-limit-${def.key}`}
      >
        {#if def.kind.type === 'choice'}
          <Select
            id={`plugin-limit-${def.key}`}
            size="sm"
            hideLabel
            {label}
            value={String(value)}
            options={def.kind.options.map((o) => ({ value: o, label: td(`settings.${def.key}.options.${o}`) }))}
            onchange={(v) => void set(def, v)}
          />
        {:else}
          <Input
            id={`plugin-limit-${def.key}`}
            size="sm"
            class="limit-in"
            type="number"
            hideLabel
            {label}
            min={def.kind.type === 'integer' ? (def.kind.min ?? undefined) : undefined}
            max={def.kind.type === 'integer' ? (def.kind.max ?? undefined) : undefined}
            value={Number(value)}
            onchange={(e: Event) => void set(def, Number((e.currentTarget as HTMLInputElement).value))}
          />
        {/if}
      </FormRow>
    {/each}
  </div>
{/if}

<style>
  .none {
    margin: 0;
    padding: var(--space-3) 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  div :global(.limit-in) {
    min-width: 0;
    width: 8rem;
  }
</style>
