<!--
  Settings → Study → Deck options (v1.3): what a preset is (one package of study options that
  many decks can share), the list of presets on the left, the chosen preset's options on the
  right — everyday rows first (daily limits, steps, retention), the rest under "Advanced" —
  and which decks use it (asked from the backend: `presets.for_deck` per deck, so inherited
  use is shown exactly as the scheduler sees it). The backend validates and stores everything.
  Each row is marked "default" when it equals the built-in defaults (`presets.list.defaults`);
  "Reset to default" puts the default back into the form, Save stores it.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, EffectivePreset, PresetConfig, PresetDto, PresetList } from '$lib/api/types';
  import { formatCount, formatPercent100 } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Brain,
    Button,
    CalendarDays,
    Copy,
    EmptyState,
    ExternalLink,
    IconButton,
    Input,
    Layers,
    Plus,
    Save,
    Select,
    Switch,
    Textarea,
    Trash2,
    X,
  } from '$lib/ui';
  import { FormRow, LoadState, SettingsAdvanced, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<PresetList | null>(null);
  let failed = $state(false);
  let selected = $state<string | null>(null);
  let newName = $state('');
  let saved = $state(false);
  let deckToAssign = $state<string | null>(null);
  /** Effective preset of every deck (`presets.for_deck`): who uses what, inherited or not. */
  let usage = $state<Record<string, EffectivePreset>>({});

  // Edit form (strings for list inputs; parsed on save, validated by the backend).
  let name = $state('');
  let cfg = $state<PresetConfig | null>(null);
  let learning = $state('');
  let relearning = $state('');
  let weights = $state('');
  /** Desired retention as a percentage (70–99); stored as a ratio. */
  let retention = $state<number | null>(90);

  const current = $derived(list?.items.find((p) => p.id === selected) ?? null);
  const defaults = $derived(list?.defaults ?? null);

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const pathOf = (id: string) => decks.find((d) => d.id === id)?.path ?? id;
  const assignable = $derived(
    current ? decks.filter((d) => !current.deck_ids.includes(d.id)).map((d) => ({ value: d.id, label: d.path })) : [],
  );

  /** Decks that get `presetId` from a parent deck (not assigned on themselves). */
  const inheritedOf = (presetId: string) =>
    decks.filter((d) => {
      const u = usage[d.id];
      return u?.preset_id === presetId && u.assigned_on !== d.id;
    });
  /** Every deck that studies with `p` (assigned or inherited). */
  const usersOf = (p: PresetDto) => p.deck_ids.length + inheritedOf(p.id).length;
  const inherited = $derived(current ? inheritedOf(current.id) : []);
  const users = $derived(current ? current.deck_ids.length + inherited.length : 0);
  const onDefaults = $derived(decks.filter((d) => usage[d.id] && usage[d.id]!.preset_id === null).length);

  const NEW_ORDER = ['added', 'random'] as const;
  const REVIEW_ORDER = ['due', 'random', 'retrievability'] as const;
  const newOrders = $derived(NEW_ORDER.map((v) => ({ value: v, label: t(`presets.new_order.${v}`) })));
  const reviewOrders = $derived(REVIEW_ORDER.map((v) => ({ value: v, label: t(`presets.review_order.${v}`) })));

  const nums = (s: string) =>
    s
      .split(/[\s,;]+/)
      .filter(Boolean)
      .map(Number);

  /** The form as the config `presets.update` would get. */
  function formConfig(): PresetConfig | null {
    if (!cfg) return null;
    return {
      ...$state.snapshot(cfg),
      learning_steps_mins: nums(learning),
      relearning_steps_mins: nums(relearning),
      fsrs_weights: nums(weights),
      desired_retention: retention === null ? Number.NaN : Math.round(Number(retention)) / 100,
    };
  }
  const form = $derived(formConfig());

  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  type Field = keyof PresetConfig;
  /** `true` when the form's `field` equals the built-in default (FormRow marker). */
  const isDef = (field: Field): boolean | undefined => (form && defaults ? same(form[field], defaults[field]) : undefined);
  /** Puts the default of `field` back into the form (Save stores it). */
  function reset(field: Field) {
    if (!cfg || !defaults) return;
    saved = false;
    const d = structuredClone($state.snapshot(defaults));
    if (field === 'learning_steps_mins') learning = d.learning_steps_mins.join(' ');
    else if (field === 'relearning_steps_mins') relearning = d.relearning_steps_mins.join(' ');
    else if (field === 'fsrs_weights') weights = d.fsrs_weights.join(' ');
    else if (field === 'desired_retention') retention = Math.round(d.desired_retention * 100);
    else (cfg as Record<string, unknown>)[field] = d[field];
  }

  const dirty = $derived.by(() => {
    if (!current || !form) return false;
    if (name !== current.name) return true;
    return (Object.keys(current.config) as Field[]).some((k) => !same(form[k], current.config[k]));
  });

  /** Minutes as words in the largest whole unit: `10 dk`, `1 sa`, `2 gün`. */
  function duration(mins: number): string {
    if (mins >= 1440 && mins % 1440 === 0) return t('presets.unit.days', { n: mins / 1440 });
    if (mins >= 60 && mins % 60 === 0) return t('presets.unit.hours', { n: mins / 60 });
    return t('presets.unit.minutes', { n: mins });
  }
  function stepsExample(raw: string, key: 'learning' | 'relearning'): string {
    const steps = nums(raw).filter((n) => Number.isFinite(n) && n > 0);
    if (steps.length === 0) return t(`presets.example.${key}_none`);
    return t(`presets.example.${key}`, { steps: steps.map(duration).join(' → ') });
  }
  const exampleNew = $derived(cfg ? t('presets.example.new_per_day', { n: formatCount(Number(cfg.new_per_day) || 0) }) : '');
  const exampleReviews = $derived(cfg ? t('presets.example.reviews_per_day', { n: formatCount(Number(cfg.reviews_per_day) || 0) }) : '');
  const exampleRetention = $derived(
    retention !== null && Number.isFinite(Number(retention))
      ? t('presets.example.desired_retention', { pct: formatPercent100(Number(retention)), n: formatCount(Math.round(Number(retention))) })
      : '',
  );
  const exampleInterval = $derived(
    cfg ? t('presets.example.maximum_interval_days', { span: duration((Number(cfg.maximum_interval_days) || 0) * 1440) }) : '',
  );

  /** Opens `p` in the form (`force`: also when it is already open, e.g. after saving). */
  function edit(p: PresetDto | null, force = false) {
    if (!force && p && p.id === selected && cfg) return;
    selected = p?.id ?? null;
    saved = false;
    if (!p) {
      cfg = null;
      return;
    }
    name = p.name;
    cfg = structuredClone($state.snapshot(p.config));
    learning = p.config.learning_steps_mins.join(' ');
    relearning = p.config.relearning_steps_mins.join(' ');
    weights = p.config.fsrs_weights.join(' ');
    retention = Math.round(p.config.desired_retention * 100);
  }

  async function loadUsage() {
    const ids = decks.map((d) => d.id);
    const effective = await Promise.all(ids.map((deck_id) => api.call('presets.for_deck', { deck_id }).catch(() => null)));
    usage = Object.fromEntries(effective.filter((e): e is EffectivePreset => e !== null).map((e) => [e.deck_id, e]));
  }

  async function load() {
    const ok = await ctx.guarded(async () => {
      list = await api.call('presets.list', {});
      const p = list.items.find((x) => x.id === selected) ?? null;
      if (!p) edit(list.items[0] ?? null);
    });
    failed = !ok && !list;
    await loadUsage();
  }

  async function create() {
    await ctx.guarded(async () => {
      const p = await api.call('presets.create', { name: newName, config: null });
      newName = '';
      await load();
      edit(list?.items.find((x) => x.id === p.id) ?? null);
    });
  }

  async function save() {
    const config = formConfig();
    if (!current || !config) return;
    saved = await ctx.guarded(async () => {
      const p = await api.call('presets.update', { id: current.id, name, config });
      await load();
      edit(list?.items.find((x) => x.id === p.id) ?? null, true);
    });
  }

  async function copy() {
    if (!current) return;
    await ctx.guarded(async () => {
      const p = await api.call('presets.clone', { id: current.id, name: t('presets.clone_suffix', { name: current.name }) });
      await load();
      edit(list?.items.find((x) => x.id === p.id) ?? null);
    });
  }

  async function remove() {
    if (!current) return;
    await ctx.guarded(() => api.call('presets.delete', { id: current.id }));
    await load();
  }

  async function assign(deckIds: string[], presetId: string | null) {
    await ctx.guarded(() => api.call('presets.assign', { deck_ids: deckIds, preset_id: presetId }));
    deckToAssign = null;
    await load();
  }

  // A deck added or removed elsewhere: ask again who uses what.
  let deckKey = '';
  $effect(() => {
    const k = decks.map((d) => d.id).join(',');
    if (k !== deckKey && list) {
      deckKey = k;
      void loadUsage();
    }
  });

  onMount(() => {
    void load();
    const unsub = api.on(['presets_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

{#snippet newForm()}
  <form class="new" onsubmit={(e) => { e.preventDefault(); void create(); }}>
    <Input size="sm" bind:value={newName} label={t('presets.new_name')} hideLabel placeholder={t('presets.new_placeholder')} />
    <Button size="sm" type="submit" icon={Plus} disabled={!newName.trim()}>{t('presets.new')}</Button>
  </form>
{/snippet}

{#snippet explainer()}
  <ul class="explain" aria-label={t('presets.how.title')}>
    <li>{t('presets.how.what')}</li>
    <li>{t('presets.how.inherit')}</li>
    <li>{t('presets.how.shared')}</li>
  </ul>
{/snippet}

<SettingsPage title={t('presets.heading')} subtitle={t('presets.intro')} width="wide">
  {#if !list}
    <SettingsSection>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={5} />
    </SettingsSection>
  {:else if list.items.length === 0}
    <SettingsSection title={t('presets.how.title')}>
      {@render explainer()}
    </SettingsSection>
    <SettingsSection>
      <EmptyState icon={Layers} title={t('presets.none')} description={t('presets.none_hint')}>
        <div class="new-empty">{@render newForm()}</div>
      </EmptyState>
    </SettingsSection>
  {:else}
  <SettingsSection title={t('presets.how.title')}>
    {@render explainer()}
  </SettingsSection>
  <div class="layout">
    <aside aria-label={t('presets.list')}>
      <h2 class="side-title">{t('presets.list')}</h2>
      <ul class="presets">
        {#each list.items as p (p.id)}
          {@const n = usersOf(p)}
          <li>
            <button type="button" class="preset" class:active={p.id === selected} aria-pressed={p.id === selected} onclick={() => edit(p)}>
              <span class="pname">{p.name}</span>
              <span class="count num" title={t('presets.deck_count', { n })}>
                <span class="sr-only">{t('presets.deck_count', { n })}</span>
                <span aria-hidden="true">{formatCount(n)}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if onDefaults > 0}
        <p class="side-note" data-testid="presets-on-defaults">{t('presets.on_defaults', { n: onDefaults })}</p>
      {/if}
      <div class="new-side">{@render newForm()}</div>
    </aside>

    <div class="editor">
      {#if current && cfg}
        <form class="stack" aria-label={current.name} onsubmit={(e) => { e.preventDefault(); void save(); }}>
          <SettingsSection
            title={current.name}
            description={users === 0 ? t('presets.used_by_none') : t('presets.used_by', { n: users })}
            body="rows"
          >
            {#snippet actions()}
              <Button variant="ghost" size="sm" icon={Copy} onclick={() => void copy()}>{t('presets.clone')}</Button>
            {/snippet}
            <FormRow label={t('presets.field.name')} description={t('presets.desc.name')}>
              <Input size="sm" class="txt" bind:value={name} label={t('presets.field.name')} hideLabel />
            </FormRow>
          </SettingsSection>

          <SettingsSection title={t('presets.section.daily')} description={t('presets.section.daily_desc')} icon={CalendarDays} body="rows">
            <FormRow
              label={t('presets.field.new_per_day')}
              description={t('presets.desc.new_per_day')}
              example={exampleNew}
              isDefault={isDef('new_per_day')}
              onReset={() => reset('new_per_day')}
            >
              <Input size="sm" class="num-in" type="number" min="0" bind:value={cfg.new_per_day} label={t('presets.field.new_per_day')} hideLabel />
            </FormRow>
            <FormRow
              label={t('presets.field.reviews_per_day')}
              description={t('presets.desc.reviews_per_day')}
              example={exampleReviews}
              isDefault={isDef('reviews_per_day')}
              onReset={() => reset('reviews_per_day')}
            >
              <Input size="sm" class="num-in" type="number" min="0" bind:value={cfg.reviews_per_day} label={t('presets.field.reviews_per_day')} hideLabel />
            </FormRow>
          </SettingsSection>

          <SettingsSection title={t('presets.section.memory')} description={t('presets.section.memory_desc')} icon={Brain} body="rows">
            <FormRow
              label={t('presets.field.desired_retention')}
              description={t('presets.desc.desired_retention')}
              example={exampleRetention}
              isDefault={isDef('desired_retention')}
              onReset={() => reset('desired_retention')}
            >
              <span class="pct">
                <Input size="sm" class="num-in" type="number" min="70" max="99" step="1" bind:value={retention} label={t('presets.field.desired_retention')} hideLabel />
                <span class="unit" aria-hidden="true">%</span>
              </span>
            </FormRow>
            <FormRow
              label={t('presets.field.learning_steps_mins')}
              description={t('presets.desc.learning_steps')}
              example={stepsExample(learning, 'learning')}
              isDefault={isDef('learning_steps_mins')}
              onReset={() => reset('learning_steps_mins')}
            >
              <Input size="sm" class="txt" bind:value={learning} label={t('presets.field.learning_steps_mins')} hideLabel placeholder="1 10" />
            </FormRow>
            <FormRow
              label={t('presets.field.relearning_steps_mins')}
              description={t('presets.desc.relearning_steps')}
              example={stepsExample(relearning, 'relearning')}
              isDefault={isDef('relearning_steps_mins')}
              onReset={() => reset('relearning_steps_mins')}
            >
              <Input size="sm" class="txt" bind:value={relearning} label={t('presets.field.relearning_steps_mins')} hideLabel placeholder="10" />
            </FormRow>
          </SettingsSection>

          <SettingsAdvanced id="presets" summary={t('presets.advanced_summary')}>
            <SettingsSection title={t('presets.section.order')} description={t('presets.section.order_desc')} body="rows">
              <FormRow label={t('presets.field.new_order')} description={t('presets.desc.new_order')} isDefault={isDef('new_order')} onReset={() => reset('new_order')}>
                <Select size="sm" options={newOrders} bind:value={cfg.new_order} label={t('presets.field.new_order')} hideLabel />
              </FormRow>
              <FormRow label={t('presets.field.review_order')} description={t('presets.desc.review_order')} isDefault={isDef('review_order')} onReset={() => reset('review_order')}>
                <Select size="sm" options={reviewOrders} bind:value={cfg.review_order} label={t('presets.field.review_order')} hideLabel />
              </FormRow>
              <FormRow label={t('presets.field.bury_siblings')} description={t('presets.desc.bury_siblings')} example={t('presets.example.bury_siblings')} isDefault={isDef('bury_siblings')} onReset={() => reset('bury_siblings')}>
                <Switch bind:checked={cfg.bury_siblings} ariaLabel={t('presets.field.bury_siblings')} />
              </FormRow>
              <FormRow
                label={t('presets.field.maximum_interval_days')}
                description={t('presets.desc.maximum_interval_days')}
                example={exampleInterval}
                isDefault={isDef('maximum_interval_days')}
                onReset={() => reset('maximum_interval_days')}
              >
                <Input size="sm" class="num-in" type="number" min="1" max="36500" bind:value={cfg.maximum_interval_days} label={t('presets.field.maximum_interval_days')} hideLabel />
              </FormRow>
            </SettingsSection>
            <SettingsSection title={t('presets.section.fsrs')} description={t('presets.section.fsrs_desc')} body="rows">
              <FormRow label={t('presets.field.fsrs_weights')} description={t('presets.field.fsrs_hint')} isDefault={isDef('fsrs_weights')} onReset={() => reset('fsrs_weights')} stack>
                <Textarea rows={2} bind:value={weights} label={t('presets.field.fsrs_weights')} hideLabel placeholder={t('presets.fsrs_placeholder')} />
                {#if ctx.navigate}
                  <Button variant="ghost" size="sm" iconEnd={ExternalLink} onclick={() => ctx.navigate?.('/planning')}>{t('presets.open_optimizer')}</Button>
                {/if}
              </FormRow>
            </SettingsSection>
          </SettingsAdvanced>

          <div class="save" class:dirty>
            <p class="save-note" role="status">
              {#if saved && !dirty}
                <span class="saved">{t('presets.saved')}</span>
              {:else if dirty}
                {users > 1 ? t('presets.unsaved_shared', { n: users }) : t('presets.unsaved')}
              {/if}
            </p>
            <Button type="submit" variant="primary" size="sm" icon={Save}>{t('presets.save')}</Button>
          </div>
        </form>

        <SettingsSection title={t('presets.decks')} description={t('presets.decks_intro')} icon={Layers} body="rows">
          {#if current.deck_ids.length === 0 && inherited.length === 0}
            <p class="muted">{t('presets.decks_none')}</p>
          {:else}
            <ul class="decks" data-testid="preset-decks">
              {#each current.deck_ids as d (d)}
                <li>
                  <span class="path">{pathOf(d)}</span>
                  <IconButton size="sm" icon={X} label={t('presets.unassign', { path: pathOf(d) })} onclick={() => void assign([d], null)} />
                </li>
              {/each}
              {#each inherited as d (d.id)}
                {@const from = usage[d.id]?.assigned_on}
                <li class="inherited">
                  <span class="path">{d.path}</span>
                  <span class="via">{t('presets.inherits_from', { path: from ? pathOf(from) : '' })}</span>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="assign">
            <Select size="sm" options={assignable} bind:value={deckToAssign} label={t('presets.assign_pick')} hideLabel placeholder={t('presets.assign_pick')} disabled={assignable.length === 0} />
            <Button size="sm" icon={Plus} disabled={!deckToAssign} onclick={() => deckToAssign && void assign([deckToAssign], current.id)}>{t('presets.assign')}</Button>
          </div>
        </SettingsSection>

        <SettingsSection title={t('presets.danger')} tone="danger" body="rows">
          <FormRow label={current.name} description={t('presets.delete_desc')}>
            <Button variant="danger" size="sm" icon={Trash2} onclick={() => void remove()}>{t('presets.delete')}</Button>
          </FormRow>
        </SettingsSection>
      {:else}
        <SettingsSection>
          <EmptyState size="sm" icon={Layers} title={t('presets.pick')} />
        </SettingsSection>
      {/if}
    </div>
  </div>
  {/if}
</SettingsPage>

<style>
  .layout {
    display: grid;
    grid-template-columns: 13.5rem minmax(0, 1fr);
    gap: var(--space-8);
    align-items: start;
  }
  aside {
    position: sticky;
    top: 0;
  }
  .side-title {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .presets {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  .presets li {
    border-bottom: 1px solid var(--border);
  }
  .preset {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    height: var(--row-h);
    padding: 0 var(--space-2);
    border: none;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  .preset:hover {
    background: var(--surface-2);
  }
  .preset.active {
    background: var(--surface-3);
    font-weight: var(--weight-medium);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .preset:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .pname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    flex: none;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .side-note {
    margin: var(--space-2) 0 0;
    color: var(--text-2);
    font-size: var(--text-xs);
    line-height: var(--leading);
  }
  .explain {
    margin: 0;
    padding-inline-start: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--text-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
    max-width: 68ch;
  }
  .new {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .new :global(.fc-field) {
    flex: 1 1 10rem;
    min-width: 0;
  }
  .new-side {
    margin-top: var(--space-3);
  }
  @media (min-width: 1024px) {
    .new-side .new {
      flex-direction: column;
      align-items: stretch;
    }
    .new-side .new :global(.fc-field) {
      flex: none;
    }
  }
  .new-empty {
    width: min(100%, 26rem);
    margin-top: var(--space-2);
  }
  .editor,
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    min-width: 0;
    max-width: 720px;
  }
  .editor :global(.num-in) {
    min-width: 0;
    width: 7rem;
  }
  .editor :global(.txt) {
    min-width: 0;
    width: 12rem;
  }
  .pct {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .unit {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .save {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) 0;
    margin-top: calc(-1 * var(--space-4));
    border-top: 1px solid var(--border);
    background: var(--surface-0);
  }
  .save-note {
    flex: 1 1 auto;
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    text-align: end;
  }
  .save.dirty .save-note {
    color: var(--warning);
  }
  .saved {
    color: var(--success);
  }
  .muted {
    margin: 0;
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .decks {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .decks li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--row-h);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .decks li.inherited {
    color: var(--text-2);
  }
  .via {
    flex: none;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .assign {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
    padding-top: var(--space-3);
  }
  .assign :global(.fc-field) {
    flex: 1 1 14rem;
    min-width: 0;
  }
  @media (max-width: 1023.98px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-6);
    }
    aside {
      position: static;
      max-width: 720px;
    }
  }
</style>
