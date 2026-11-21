<!--
  Settings → Study → Deck options: preset list on the left, the chosen preset's options on
  the right (limits, memory, advanced, decks). The backend validates and stores everything.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, PresetConfig, PresetDto, PresetList } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    ChevronDown,
    ChevronUp,
    Copy,
    EmptyState,
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
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<PresetList | null>(null);
  let failed = $state(false);
  let selected = $state<string | null>(null);
  let newName = $state('');
  let saved = $state(false);
  let deckToAssign = $state<string | null>(null);
  let advanced = $state(false);

  // Edit form (strings for list inputs; parsed on save, validated by the backend).
  let name = $state('');
  let cfg = $state<PresetConfig | null>(null);
  let learning = $state('');
  let relearning = $state('');
  let weights = $state('');

  const current = $derived(list?.items.find((p) => p.id === selected) ?? null);

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));
  const pathOf = (id: string) => decks.find((d) => d.id === id)?.path ?? id;
  const assignable = $derived(
    current ? decks.filter((d) => !current.deck_ids.includes(d.id)).map((d) => ({ value: d.id, label: d.path })) : [],
  );

  const NEW_ORDER = ['added', 'random'] as const;
  const REVIEW_ORDER = ['due', 'random', 'retrievability'] as const;
  const newOrders = $derived(NEW_ORDER.map((v) => ({ value: v, label: t(`presets.new_order.${v}`) })));
  const reviewOrders = $derived(REVIEW_ORDER.map((v) => ({ value: v, label: t(`presets.review_order.${v}`) })));

  const nums = (s: string) =>
    s
      .split(/[\s,;]+/)
      .filter(Boolean)
      .map(Number);

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
  }

  async function load() {
    const ok = await ctx.guarded(async () => {
      list = await api.call('presets.list', {});
      const p = list.items.find((x) => x.id === selected) ?? null;
      if (!p) edit(list.items[0] ?? null);
    });
    failed = !ok && !list;
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
    if (!current || !cfg) return;
    const config: PresetConfig = {
      ...$state.snapshot(cfg),
      learning_steps_mins: nums(learning),
      relearning_steps_mins: nums(relearning),
      fsrs_weights: nums(weights),
    };
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

<SettingsPage title={t('presets.heading')} subtitle={t('presets.intro')} width="wide">
  {#if !list}
    <SettingsSection>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} lines={5} />
    </SettingsSection>
  {:else if list.items.length === 0}
    <SettingsSection>
      <EmptyState icon={Layers} title={t('presets.none')} description={t('presets.none_hint')}>
        <div class="new-empty">{@render newForm()}</div>
      </EmptyState>
    </SettingsSection>
  {:else}
  <div class="layout">
    <aside aria-label={t('presets.list')}>
      <h2 class="side-title">{t('presets.list')}</h2>
      <ul class="presets">
        {#each list.items as p (p.id)}
          <li>
            <button type="button" class="preset" class:active={p.id === selected} aria-pressed={p.id === selected} onclick={() => edit(p)}>
              <span class="pname">{p.name}</span>
              <span class="count num" title={t('presets.deck_count', { n: p.deck_ids.length })}>
                <span class="sr-only">{t('presets.deck_count', { n: p.deck_ids.length })}</span>
                <span aria-hidden="true">{formatCount(p.deck_ids.length)}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      <div class="new-side">{@render newForm()}</div>
    </aside>

    <div class="editor">
      {#if current && cfg}
        <form class="stack" aria-label={current.name} onsubmit={(e) => { e.preventDefault(); void save(); }}>
          <SettingsSection title={current.name} body="rows">
            {#snippet actions()}
              <Button variant="ghost" size="sm" icon={Copy} onclick={() => void copy()}>{t('presets.clone')}</Button>
            {/snippet}
            <FormRow label={t('presets.field.name')}>
              <Input size="sm" class="txt" bind:value={name} label={t('presets.field.name')} hideLabel />
            </FormRow>
            <FormRow label={t('presets.field.new_per_day')} description={t('presets.desc.new_per_day')}>
              <Input size="sm" class="num-in" type="number" min="0" bind:value={cfg.new_per_day} label={t('presets.field.new_per_day')} hideLabel />
            </FormRow>
            <FormRow label={t('presets.field.reviews_per_day')} description={t('presets.desc.reviews_per_day')}>
              <Input size="sm" class="num-in" type="number" min="0" bind:value={cfg.reviews_per_day} label={t('presets.field.reviews_per_day')} hideLabel />
            </FormRow>
            <FormRow label={t('presets.field.desired_retention')} description={t('presets.desc.desired_retention')}>
              <Input size="sm" class="num-in" type="number" min="0.7" max="0.99" step="0.01" bind:value={cfg.desired_retention} label={t('presets.field.desired_retention')} hideLabel />
            </FormRow>
            <FormRow label={t('presets.field.learning_steps_mins')} description={t('presets.field.steps_hint')}>
              <Input size="sm" class="txt" bind:value={learning} label={t('presets.field.learning_steps_mins')} hideLabel placeholder="1 10" />
            </FormRow>
            <FormRow label={t('presets.field.relearning_steps_mins')} description={t('presets.field.steps_hint')}>
              <Input size="sm" class="txt" bind:value={relearning} label={t('presets.field.relearning_steps_mins')} hideLabel placeholder="10" />
            </FormRow>
            <div class="adv-toggle">
              <Button variant="ghost" size="sm" iconEnd={advanced ? ChevronUp : ChevronDown} aria-expanded={advanced} aria-controls="fc-preset-advanced" onclick={() => (advanced = !advanced)}>
                {t('presets.advanced')}
              </Button>
            </div>
            {#if advanced}
              <div id="fc-preset-advanced" class="advanced">
                <FormRow label={t('presets.field.bury_siblings')}>
                  <Switch bind:checked={cfg.bury_siblings} ariaLabel={t('presets.field.bury_siblings')} />
                </FormRow>
                <FormRow label={t('presets.field.maximum_interval_days')}>
                  <Input size="sm" class="num-in" type="number" min="1" max="36500" bind:value={cfg.maximum_interval_days} label={t('presets.field.maximum_interval_days')} hideLabel />
                </FormRow>
                <FormRow label={t('presets.field.new_order')}>
                  <Select size="sm" options={newOrders} bind:value={cfg.new_order} label={t('presets.field.new_order')} hideLabel />
                </FormRow>
                <FormRow label={t('presets.field.review_order')}>
                  <Select size="sm" options={reviewOrders} bind:value={cfg.review_order} label={t('presets.field.review_order')} hideLabel />
                </FormRow>
                <FormRow label={t('presets.field.fsrs_weights')} description={t('presets.field.fsrs_hint')} stack>
                  <Textarea rows={2} bind:value={weights} label={t('presets.field.fsrs_weights')} hideLabel />
                </FormRow>
              </div>
            {/if}
            <div class="save">
              {#if saved}<span role="status" class="saved">{t('presets.saved')}</span>{/if}
              <Button type="submit" variant="primary" size="sm" icon={Save}>{t('presets.save')}</Button>
            </div>
          </SettingsSection>
        </form>

        <SettingsSection title={t('presets.decks')} description={t('presets.decks_intro')} body="rows">
          {#if current.deck_ids.length === 0}
            <p class="muted">{t('presets.decks_none')}</p>
          {:else}
            <ul class="decks">
              {#each current.deck_ids as d (d)}
                <li>
                  <span class="path">{pathOf(d)}</span>
                  <IconButton size="sm" icon={X} label={t('presets.unassign', { path: pathOf(d) })} onclick={() => void assign([d], null)} />
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
  .adv-toggle {
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
  }
  .adv-toggle :global(.fc-btn) {
    margin-inline-start: -8px;
  }
  .save {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-3);
    padding-top: var(--space-3);
  }
  .saved {
    color: var(--success);
    font-size: var(--text-sm);
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
    height: var(--row-h);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
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
