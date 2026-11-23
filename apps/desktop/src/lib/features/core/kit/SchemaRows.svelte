<!--
  Settings rows drawn from `settings.schema` (v1.4): pass the keys a page shows, in order; a
  key whose module is off is not in the schema and leaves no row. Each row: the setting's own
  label and one-line description (Rust declares both keys), a control by kind (switch,
  segmented/select for choices, number/text field), saved at once with `settings.set` at app
  scope (validation is Rust's; a refused value is put back). Use inside a
  `SettingsSection body="rows"`. `onsaved` lets the page refresh what depends on a value.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SettingDef } from '$lib/api/types';
  import { td } from '$lib/i18n/index.svelte';
  import { Input, Select, SegmentedControl, Switch } from '$lib/ui';
  import FormRow from './FormRow.svelte';

  interface Props {
    keys: string[];
    /** Runs a call and shows its error (the screen's `ctx.guarded`); plain call otherwise. */
    guarded?: (fn: () => Promise<unknown>) => Promise<boolean>;
    onsaved?: (key: string, value: unknown) => void;
  }

  let { keys, guarded, onsaved }: Props = $props();

  let defs = $state<SettingDef[]>([]);
  let values = $state<Record<string, unknown>>({});

  async function load() {
    try {
      const schema = (await api.call('settings.schema', {})).settings;
      const found = keys.flatMap((k) => schema.filter((e) => e.def.key === k).map((e) => e.def));
      const got = found.length ? (await api.call('settings.get', { scope: { kind: 'app' }, keys: found.map((d) => d.key) })).values : [];
      const next: Record<string, unknown> = {};
      for (const d of found) next[d.key] = d.default;
      for (const v of got) if (v.value !== null && v.value !== undefined) next[v.key] = v.value;
      values = next;
      defs = found;
    } catch {
      defs = [];
    }
  }
  onMount(() => void load());

  async function save(def: SettingDef, value: unknown) {
    const before = values[def.key];
    values = { ...values, [def.key]: value };
    const call = () => api.call('settings.set', { scope: { kind: 'app' }, key: def.key, value });
    const ok = guarded
      ? await guarded(call)
      : await call().then(
          () => true,
          () => false,
        );
    if (!ok) values = { ...values, [def.key]: before };
    else onsaved?.(def.key, value);
  }

  const label = (d: SettingDef) => td(d.label_key);
  const description = (d: SettingDef) => (d.description_key ? td(d.description_key) : undefined);
  const choices = (d: SettingDef) =>
    d.kind.type === 'choice' ? d.kind.options.map((o) => ({ value: o, label: td(`${d.label_key.replace(/\.label$/, '')}.${o}`) })) : [];
  const isDef = (d: SettingDef) => JSON.stringify(values[d.key]) === JSON.stringify(d.default);

  function commit(def: SettingDef, raw: string) {
    if (def.kind.type === 'integer' || def.kind.type === 'float') {
      const n = Number(raw);
      if (raw.trim() === '' || !Number.isFinite(n)) return;
      if (n !== values[def.key]) void save(def, def.kind.type === 'integer' ? Math.round(n) : n);
    } else if (raw !== values[def.key]) void save(def, raw);
  }
</script>

{#each defs as d (d.key)}
  {@const id = `schema-setting-${d.key.replaceAll('.', '-')}`}
  <div class="schema-row" data-setting={d.key}>
    {#if d.kind.type === 'bool'}
      <FormRow label={label(d)} description={description(d)} isDefault={isDef(d)} onReset={() => void save(d, d.default)}>
        <Switch ariaLabel={label(d)} checked={values[d.key] === true} onchange={(v) => void save(d, v)} />
      </FormRow>
    {:else if d.kind.type === 'choice'}
      <FormRow label={label(d)} description={description(d)} isDefault={isDef(d)} onReset={() => void save(d, d.default)}>
        {#if d.kind.options.length <= 3}
          <SegmentedControl size="sm" label={label(d)} options={choices(d)} value={values[d.key] as string} onchange={(v: string) => void save(d, v)} />
        {:else}
          <Select label={label(d)} hideLabel options={choices(d)} value={values[d.key] as string} onchange={(v: string) => void save(d, v)} />
        {/if}
      </FormRow>
    {:else}
      <FormRow label={label(d)} description={description(d)} labelFor={id} isDefault={isDef(d)} onReset={() => void save(d, d.default)}>
        <Input
          {id}
          size="sm"
          label={label(d)}
          hideLabel
          type={d.kind.type === 'text' ? 'text' : 'number'}
          min={d.kind.type === 'integer' || d.kind.type === 'float' ? (d.kind.min ?? undefined) : undefined}
          max={d.kind.type === 'integer' || d.kind.type === 'float' ? (d.kind.max ?? undefined) : undefined}
          value={String(values[d.key] ?? '')}
          class={d.kind.type === 'text' ? 'schema-text' : 'schema-number'}
          onchange={(e: Event) => commit(d, (e.currentTarget as HTMLInputElement).value)}
        />
      </FormRow>
    {/if}
  </div>
{/each}

<style>
  .schema-row {
    display: contents;
  }
  :global(.schema-number) {
    width: 120px;
  }
  :global(.schema-text) {
    width: 260px;
    max-width: 100%;
  }
</style>
