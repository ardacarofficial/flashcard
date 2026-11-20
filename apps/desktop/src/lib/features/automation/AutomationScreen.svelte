<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ActionInfo, AttributeDefDto, DeckTreeNode, Rating, RuleDto, RulePreview, RunDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import {
    TRIGGER_KINDS,
    emptyAction,
    fromAction,
    fitsTrigger,
    fromTrigger,
    toAction,
    toTrigger,
    type ActionForm,
    type TriggerForm,
  } from './actions';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const RATINGS: Rating[] = ['again', 'hard', 'good', 'easy'];
  const FLAGS = [1, 2, 3, 4, 5, 6, 7];

  let rules = $state<RuleDto[]>([]);
  let runs = $state<RunDto[]>([]);
  let actionInfos = $state<ActionInfo[]>([]);
  let attrs = $state<AttributeDefDto[]>([]);

  // Editor (null = closed; id null = new rule).
  let editing = $state<{ id: string | null } | null>(null);
  let name = $state('');
  let enabled = $state(true);
  let trigger = $state<TriggerForm>(fromTrigger({ kind: 'note_added' }));
  let condition = $state('');
  let actions = $state<ActionForm[]>([]);
  let preview = $state<RulePreview | null>(null);

  const decks = $derived.by(() => {
    const out: { id: string; path: string }[] = [];
    const walk = (nodes: DeckTreeNode[]) => {
      for (const n of nodes) {
        out.push({ id: n.deck.id, path: n.deck.path });
        walk(n.children);
      }
    };
    walk(ctx.decks);
    return out;
  });
  // Actions that fit the chosen trigger (inbox rules accept inbox items; the rest act on cards).
  const fitting = $derived(actionInfos.filter((i) => fitsTrigger(i.selection, trigger.kind)));
  $effect(() => {
    const ok = new Set(fitting.map((i) => i.method));
    if (fitting.length > 0 && actions.some((a) => !ok.has(a.method))) {
      actions = actions.map((a) => (ok.has(a.method) ? a : emptyAction(fitting[0]!.method)));
    }
  });
  const choiceAttrs = $derived(attrs.filter((a) => a.kind === 'ordered_enum' || a.kind === 'enum'));
  const ruleName = (id: string) => rules.find((r) => r.id === id)?.name ?? '';

  async function load() {
    await ctx.guarded(async () => {
      rules = (await api.call('automation.rules.list', {})).rules;
      runs = (await api.call('automation.runs.list', { rule_id: null, limit: 20 })).runs;
    });
  }

  function open(rule: RuleDto | null) {
    preview = null;
    editing = { id: rule?.id ?? null };
    name = rule?.name ?? '';
    enabled = rule?.enabled ?? true;
    trigger = fromTrigger(rule?.trigger ?? { kind: 'note_added' });
    condition = rule?.condition ?? '';
    actions = rule ? rule.actions.map(fromAction) : [emptyAction(actionInfos[0]?.method ?? 'bulk.add_tags')];
  }

  function draft() {
    // Plain data (no reactive proxies) on the wire.
    return $state.snapshot({ trigger: toTrigger(trigger), condition, actions: actions.map(toAction) });
  }

  async function save() {
    if (!editing) return;
    const id = editing.id;
    const d = draft();
    const ok = await ctx.guarded(() =>
      id
        ? api.call('automation.rules.update', { id, name, enabled, ...d })
        : api.call('automation.rules.create', { name, enabled, ...d }),
    );
    if (ok) editing = null;
    await load();
  }

  async function runPreview() {
    await ctx.guarded(async () => {
      preview = await api.call('automation.preview', { rule_id: null, draft: draft() });
    });
  }

  async function toggle(rule: RuleDto) {
    await ctx.guarded(() =>
      api.call('automation.rules.update', {
        id: rule.id,
        name: null,
        enabled: !rule.enabled,
        trigger: null,
        condition: null,
        actions: null,
      }),
    );
    await load();
  }

  async function move(index: number, delta: number) {
    const ids = rules.map((r) => r.id);
    const j = index + delta;
    if (j < 0 || j >= ids.length) return;
    [ids[index], ids[j]] = [ids[j]!, ids[index]!];
    await ctx.guarded(() => api.call('automation.rules.reorder', { ids }));
    await load();
  }

  async function runNow(rule: RuleDto) {
    await ctx.guarded(() => api.call('automation.rules.run', { id: rule.id }));
    await load();
  }

  async function remove(rule: RuleDto) {
    await ctx.guarded(() => api.call('automation.rules.delete', { id: rule.id }));
    await load();
  }

  function triggerText(rule: RuleDto): string {
    const tr = rule.trigger;
    const base = t(`automation.trigger.${tr.kind}`);
    if (tr.kind === 'schedule') return `${base} · ${t('automation.every_mins', { n: tr.every_mins })}`;
    if (tr.kind === 'card_answered' && tr.ratings.length)
      return `${base} · ${tr.ratings.map((r) => t(`automation.rating.${r}`)).join(', ')}`;
    if (tr.kind === 'card_flagged' && tr.flags.length) return `${base} · ${tr.flags.join(', ')}`;
    return base;
  }

  function toggleIn<T>(list: T[], v: T): T[] {
    return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  }

  onMount(() => {
    void load();
    void ctx.guarded(async () => (actionInfos = (await api.call('automation.actions', {})).actions));
    // Classifications are optional (module may be off): no error if unavailable.
    api.call('attributes.list', { scope_deck_id: null, include_deleted: false }).then(
      (r) => (attrs = r.defs),
      () => (attrs = []),
    );
    const unsub = api.on(['automation_changed', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('automation.heading')}</h2>
<p class="muted">{t('automation.intro')}</p>

{#if rules.length === 0}<p class="muted">{t('automation.none')}</p>{/if}
<ol class="rules">
  {#each rules as rule, i (rule.id)}
    <li>
      <label><input type="checkbox" checked={rule.enabled} onchange={() => toggle(rule)} />
        <strong>{rule.name}</strong></label>
      <span class="muted">{triggerText(rule)}</span>
      {#if rule.condition}<code>{rule.condition}</code>{/if}
      <span class="muted">{rule.actions.map((a) => td(`automation.action.${a.method}`)).join(' · ')}</span>
      <span class="buttons">
        <button aria-label={t('automation.up')} disabled={i === 0} onclick={() => move(i, -1)}>↑</button>
        <button aria-label={t('automation.down')} disabled={i === rules.length - 1} onclick={() => move(i, 1)}>↓</button>
        <button onclick={() => open(rule)}>{t('automation.edit')}</button>
        <button onclick={() => runNow(rule)}>{t('automation.run_now')}</button>
        <button onclick={() => remove(rule)}>{t('automation.delete')}</button>
      </span>
    </li>
  {/each}
</ol>

{#if !editing}
  <button onclick={() => open(null)}>{t('automation.new')}</button>
{:else}
  <form class="editor" onsubmit={(e) => { e.preventDefault(); void save(); }}>
    <h3>{editing.id ? t('automation.edit_rule') : t('automation.new')}</h3>
    <label>{t('automation.name')} <input bind:value={name} /></label>
    <label><input type="checkbox" bind:checked={enabled} /> {t('automation.enabled')}</label>

    <fieldset>
      <legend>{t('automation.when')}</legend>
      <label>{t('automation.trigger.label')}
        <select bind:value={trigger.kind}>
          {#each TRIGGER_KINDS as k (k)}<option value={k}>{t(`automation.trigger.${k}`)}</option>{/each}
        </select></label>
      {#if trigger.kind === 'card_answered'}
        {#each RATINGS as r (r)}
          <label><input type="checkbox" checked={trigger.ratings.includes(r)}
            onchange={() => (trigger.ratings = toggleIn(trigger.ratings, r))} /> {t(`automation.rating.${r}`)}</label>
        {/each}
      {:else if trigger.kind === 'card_flagged'}
        {#each FLAGS as f (f)}
          <label><input type="checkbox" checked={trigger.flags.includes(f)}
            onchange={() => (trigger.flags = toggleIn(trigger.flags, f))} /> {td(`browse.flags.${f}`)}</label>
        {/each}
      {:else if trigger.kind === 'schedule'}
        <label>{t('automation.every_label')} <input type="number" min="15" bind:value={trigger.everyMins} /></label>
      {/if}
    </fieldset>

    <fieldset>
      <legend>{t('automation.if')}</legend>
      <label>{t('automation.condition')} <input bind:value={condition} placeholder={t('automation.condition_example')} /></label>
      {#if trigger.kind === 'inbox_added'}<p class="muted">{t('automation.inbox_condition_hint')}</p>{/if}
    </fieldset>

    <fieldset>
      <legend>{t('automation.do')}</legend>
      {#each actions as a, i (i)}
        <div class="action">
          <select aria-label={t('automation.action.label')} bind:value={a.method}>
            {#each fitting as info (info.method)}<option value={info.method}>{td(info.label_key)}</option>{/each}
          </select>
          {#if a.method === 'bulk.add_tags' || a.method === 'bulk.remove_tags'}
            <label>{t('automation.tags')} <input bind:value={a.tags} /></label>
          {:else if a.method === 'bulk.set_suspended' || a.method === 'bulk.set_buried'}
            <label><input type="checkbox" bind:checked={a.on} /> {t('automation.on')}</label>
          {:else if a.method === 'bulk.set_flag'}
            <select aria-label={t('automation.flag')} bind:value={a.flag}>
              {#each FLAGS as f (f)}<option value={f}>{td(`browse.flags.${f}`)}</option>{/each}
            </select>
            <label><input type="checkbox" bind:checked={a.on} /> {t('automation.on')}</label>
          {:else if a.method === 'bulk.move_cards' || a.method === 'inbox.accept'}
            <select aria-label={t('automation.deck')} bind:value={a.deckId}>
              <option value="">—</option>
              {#each decks as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
            </select>
          {:else if a.method === 'bulk.set_attribute'}
            <select aria-label={t('automation.classification')} bind:value={a.attrId}>
              <option value="">—</option>
              {#each choiceAttrs as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
            </select>
            <select aria-label={t('automation.value')} bind:value={a.optionId}>
              <option value="">{t('automation.clear')}</option>
              {#each choiceAttrs.find((d) => d.id === a.attrId)?.options ?? [] as o (o.id)}
                <option value={o.id}>{o.label}</option>
              {/each}
            </select>
          {/if}
          <button type="button" onclick={() => (actions = actions.filter((_, j) => j !== i))}>{t('automation.remove_action')}</button>
        </div>
      {/each}
      <button type="button" onclick={() => (actions = [...actions, emptyAction(fitting[0]?.method ?? 'bulk.add_tags')])}>
        {t('automation.add_action')}</button>
    </fieldset>

    <div class="row">
      <button type="submit">{t('automation.save')}</button>
      <button type="button" onclick={runPreview}>{t('automation.preview')}</button>
      <button type="button" onclick={() => (editing = null)}>{t('automation.cancel')}</button>
    </div>

    {#if preview}
      <section aria-label={t('automation.preview')}>
        <p role="status">{t('automation.preview_summary', { matched: preview.matched, changed: preview.changed })}</p>
        {#if preview.truncated}<p class="muted">{t('automation.truncated')}</p>{/if}
        <ul>
          {#each preview.cards as c (c.card_id)}
            <li class:changes={c.changes}>{c.text} <span class="muted">{c.deck}</span>
              {#if c.changes}<em>{t('automation.would_change')}</em>{/if}</li>
          {/each}
        </ul>
      </section>
    {/if}
  </form>
{/if}

<h3>{t('automation.runs')}</h3>
{#if runs.length === 0}<p class="muted">{t('automation.no_runs')}</p>{/if}
<ul class="runs">
  {#each runs as r (r.seq)}
    <li><strong>{ruleName(r.rule_id)}</strong> · {td(`automation.status.${r.status}`)} · {td(`automation.trigger.${r.trigger}`)}
      · {t('automation.matched', { n: r.matched })}
      {#if r.error}<span class="muted">{r.error.message}</span>{/if}</li>
  {/each}
</ul>

<style>
  .rules li,
  .runs li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2, 0.5rem);
    align-items: baseline;
  }
  .buttons {
    margin-left: auto;
  }
  fieldset {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2, 0.5rem);
  }
  .action {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2, 0.5rem);
    width: 100%;
  }
  .changes {
    font-weight: 600;
  }
</style>
