<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckTreeNode, PresetConfig, PresetDto, PresetList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<PresetList | null>(null);
  let selected = $state<string | null>(null);
  let newName = $state('');
  let saved = $state(false);
  let deckToAssign = $state('');

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

  const nums = (s: string) =>
    s
      .split(/[\s,;]+/)
      .filter(Boolean)
      .map(Number);

  function edit(p: PresetDto | null) {
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
    await ctx.guarded(async () => {
      list = await api.call('presets.list', {});
      const p = list.items.find((x) => x.id === selected) ?? null;
      if (!p) edit(null);
    });
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
      edit(list?.items.find((x) => x.id === p.id) ?? null);
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
    deckToAssign = '';
    await load();
  }

  onMount(() => {
    void load();
    const unsub = api.on(['presets_changed', 'undone', 'redone', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('presets.heading')}</h2>
<p class="muted">{t('presets.intro')}</p>

<div class="layout">
  <aside>
    <h3>{t('presets.list')}</h3>
    {#if list && list.items.length === 0}<p class="muted">{t('presets.none')}</p>{/if}
    <ul>
      {#each list?.items ?? [] as p (p.id)}
        <li>
          <button class:active={p.id === selected} aria-pressed={p.id === selected} onclick={() => edit(p)}>{p.name}</button>
          <span class="muted">{p.deck_ids.length}</span>
        </li>
      {/each}
    </ul>
    <form onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <input bind:value={newName} placeholder={t('presets.new_name')} aria-label={t('presets.new_name')} />
      <button type="submit" disabled={!newName.trim()}>{t('presets.new')}</button>
    </form>
  </aside>

  {#if current && cfg}
    <section aria-label={current.name}>
      <form onsubmit={(e) => { e.preventDefault(); void save(); }}>
        <label>{t('presets.field.name')} <input bind:value={name} /></label>
        <label>{t('presets.field.new_per_day')} <input type="number" min="0" bind:value={cfg.new_per_day} /></label>
        <label>{t('presets.field.reviews_per_day')} <input type="number" min="0" bind:value={cfg.reviews_per_day} /></label>
        <label>{t('presets.field.desired_retention')}
          <input type="number" min="0.7" max="0.99" step="0.01" bind:value={cfg.desired_retention} /></label>
        <label>{t('presets.field.learning_steps_mins')}
          <input bind:value={learning} placeholder={t('presets.field.steps_hint')} /></label>
        <label>{t('presets.field.relearning_steps_mins')}
          <input bind:value={relearning} placeholder={t('presets.field.steps_hint')} /></label>
        <details>
          <summary>{t('presets.advanced')}</summary>
          <label class="check"><input type="checkbox" bind:checked={cfg.bury_siblings} /> {t('presets.field.bury_siblings')}</label>
          <label>{t('presets.field.maximum_interval_days')}
            <input type="number" min="1" max="36500" bind:value={cfg.maximum_interval_days} /></label>
          <label>{t('presets.field.new_order')}
            <select bind:value={cfg.new_order}>
              <option value="added">{t('presets.new_order.added')}</option>
              <option value="random">{t('presets.new_order.random')}</option>
            </select></label>
          <label>{t('presets.field.review_order')}
            <select bind:value={cfg.review_order}>
              <option value="due">{t('presets.review_order.due')}</option>
              <option value="random">{t('presets.review_order.random')}</option>
              <option value="retrievability">{t('presets.review_order.retrievability')}</option>
            </select></label>
          <label>{t('presets.field.fsrs_weights')}
            <textarea rows="2" bind:value={weights} placeholder={t('presets.field.fsrs_hint')}></textarea></label>
        </details>
        <div class="row">
          <button type="submit">{t('presets.save')}</button>
          <button type="button" onclick={() => void copy()}>{t('presets.clone')}</button>
          <button type="button" class="danger" onclick={() => void remove()}>{t('presets.delete')}</button>
          {#if saved}<span role="status" class="muted">{t('presets.saved')}</span>{/if}
        </div>
      </form>

      <h3>{t('presets.decks')}</h3>
      {#if current.deck_ids.length === 0}<p class="muted">{t('presets.decks_none')}</p>{/if}
      <ul>
        {#each current.deck_ids as d (d)}
          <li>{pathOf(d)}
            <button aria-label={t('presets.unassign', { path: pathOf(d) })} onclick={() => void assign([d], null)}>×</button></li>
        {/each}
      </ul>
      <div class="row">
        <select bind:value={deckToAssign} aria-label={t('presets.assign_pick')}>
          <option value="">{t('presets.assign_pick')}</option>
          {#each decks.filter((d) => !current.deck_ids.includes(d.id)) as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
        </select>
        <button disabled={!deckToAssign} onclick={() => void assign([deckToAssign], current.id)}>{t('presets.assign')}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(12rem, 16rem) 1fr;
    gap: var(--fc-space-6);
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    margin: var(--fc-space-1) 0;
  }
  .active {
    font-weight: 600;
  }
  label {
    display: block;
    margin: var(--fc-space-2) 0;
  }
  label input:not([type='checkbox']),
  label select,
  label textarea {
    display: block;
    width: 100%;
    max-width: 24rem;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    margin-top: var(--fc-space-2);
  }
</style>
