<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ActionInfo, AttributeDefDto, DeckTreeNode, Rating, RuleDto, RulePreview, RunDto } from '$lib/api/types';
  import { formatCount } from '$lib/format';
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
  import {
    Button,
    Checkbox,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Chip,
    EmptyState,
    History,
    Icon,
    IconButton,
    Input,
    Pencil,
    Play,
    Plus,
    Select,
    Skeleton,
    Switch,
    Trash2,
    TriangleAlert,
    X,
    Zap,
  } from '$lib/ui';
  import type { Tone } from '$lib/ui';
  import { SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const RATINGS: Rating[] = ['again', 'hard', 'good', 'easy'];
  const FLAGS = [1, 2, 3, 4, 5, 6, 7];

  let rules = $state<RuleDto[]>([]);
  let loaded = $state(false);
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
  // Rules that accept inbox proposals without the user's review (integration round 1): a
  // program paired with the local API (or a plugin) then writes notes straight into the
  // collection, bounded only by its daily proposal limit.
  const autoAccepting = $derived(
    rules.filter((r) => r.enabled && r.trigger.kind === 'inbox_added' && r.actions.some((a) => a.method === 'inbox.accept')),
  );
  const editingAutoAccept = $derived(trigger.kind === 'inbox_added' && actions.some((a) => a.method === 'inbox.accept'));

  async function load() {
    await ctx.guarded(async () => {
      rules = (await api.call('automation.rules.list', {})).rules;
      runs = (await api.call('automation.runs.list', { rule_id: null, limit: 20 })).runs;
    });
    loaded = true;
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

  const triggerOptions = $derived(TRIGGER_KINDS.map((k) => ({ value: k, label: t(`automation.trigger.${k}`) })));
  const actionOptions = $derived(fitting.map((info) => ({ value: info.method, label: td(info.label_key) })));
  const flagOptions = $derived(FLAGS.map((f) => ({ value: f, label: td(`browse.flags.${f}`) })));
  const deckOptions = $derived([{ value: '', label: '—' }, ...decks.map((d) => ({ value: d.id, label: d.path }))]);
  const attrOptions = $derived([{ value: '', label: '—' }, ...choiceAttrs.map((d) => ({ value: d.id, label: d.name }))]);
  const valueOptions = (attrId: string) => [
    { value: '', label: t('automation.clear') },
    ...(choiceAttrs.find((d) => d.id === attrId)?.options ?? []).map((o) => ({ value: o.id, label: o.label })),
  ];
  const RUN_TONE: Record<string, Tone> = { applied: 'success', no_match: 'neutral', failed: 'danger' };
</script>

{#snippet headerActions()}
  {#if !editing}
    <Button size="sm" variant="primary" icon={Plus} onclick={() => open(null)}>{t('automation.new')}</Button>
  {/if}
{/snippet}

<SettingsPage title={t('automation.heading')} subtitle={t('automation.intro')} actions={headerActions}>

  {#if autoAccepting.length > 0}
    <p class="banner" role="note">
      <Icon icon={TriangleAlert} size={16} />
      <span>{t('automation.auto_accept_active', { names: autoAccepting.map((r) => r.name).join(', ') })}</span>
    </p>
  {/if}

  {#if editing}
    <SettingsSection title={editing.id ? t('automation.edit_rule') : t('automation.new')} icon={Pencil}>
      <form class="editor" onsubmit={(e) => { e.preventDefault(); void save(); }}>
        <div class="name-row">
          <Input label={t('automation.name')} bind:value={name} />
          <Switch label={t('automation.enabled')} bind:checked={enabled} />
        </div>

        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">1</span>{t('automation.when')}</legend>
          <Select label={t('automation.trigger.label')} options={triggerOptions} bind:value={trigger.kind} />
          {#if trigger.kind === 'card_answered'}
            <div class="checks">
              {#each RATINGS as r (r)}
                <Checkbox
                  label={t(`automation.rating.${r}`)}
                  checked={trigger.ratings.includes(r)}
                  onchange={() => (trigger.ratings = toggleIn(trigger.ratings, r))}
                />
              {/each}
            </div>
          {:else if trigger.kind === 'card_flagged'}
            <div class="checks">
              {#each FLAGS as f (f)}
                <Checkbox
                  label={td(`browse.flags.${f}`)}
                  checked={trigger.flags.includes(f)}
                  onchange={() => (trigger.flags = toggleIn(trigger.flags, f))}
                />
              {/each}
            </div>
          {:else if trigger.kind === 'schedule'}
            <Input type="number" min="15" label={t('automation.every_label')} bind:value={trigger.everyMins} />
          {/if}
        </fieldset>

        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">2</span>{t('automation.if')}</legend>
          <Input
            label={t('automation.condition')}
            placeholder={t('automation.condition_example')}
            hint={trigger.kind === 'inbox_added' ? t('automation.inbox_condition_hint') : undefined}
            bind:value={condition}
          />
        </fieldset>

        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">3</span>{t('automation.do')}</legend>
          {#each actions as a, i (i)}
            <div class="action">
              <Select label={t('automation.action.label')} hideLabel options={actionOptions} bind:value={a.method} />
              {#if a.method === 'bulk.add_tags' || a.method === 'bulk.remove_tags'}
                <Input label={t('automation.tags')} hideLabel placeholder={t('automation.tags')} bind:value={a.tags} />
              {:else if a.method === 'bulk.set_suspended' || a.method === 'bulk.set_buried'}
                <Checkbox label={t('automation.on')} bind:checked={a.on} />
              {:else if a.method === 'bulk.set_flag'}
                <Select label={t('automation.flag')} hideLabel options={flagOptions} bind:value={a.flag} />
                <Checkbox label={t('automation.on')} bind:checked={a.on} />
              {:else if a.method === 'bulk.move_cards' || a.method === 'inbox.accept'}
                <Select label={t('automation.deck')} hideLabel options={deckOptions} bind:value={a.deckId} />
              {:else if a.method === 'bulk.set_attribute'}
                <Select label={t('automation.classification')} hideLabel options={attrOptions} bind:value={a.attrId} />
                <Select label={t('automation.value')} hideLabel options={valueOptions(a.attrId)} bind:value={a.optionId} />
              {/if}
              <IconButton
                icon={X}
                label={t('automation.remove_action')}
                onclick={() => (actions = actions.filter((_, j) => j !== i))}
              />
            </div>
          {/each}
          <div>
            <Button
              size="sm"
              variant="ghost"
              icon={Plus}
              onclick={() => (actions = [...actions, emptyAction(fitting[0]?.method ?? 'bulk.add_tags')])}
            >
              {t('automation.add_action')}
            </Button>
          </div>
        </fieldset>

        {#if editingAutoAccept}
          <p class="banner" role="note"><Icon icon={TriangleAlert} size={16} /><span>{t('automation.auto_accept_warning')}</span></p>
        {/if}

        {#if preview}
          <section class="preview" aria-label={t('automation.preview')}>
            <p class="summary" role="status">{t('automation.preview_summary', { matched: preview.matched, changed: preview.changed })}</p>
            {#if preview.truncated}<p class="muted">{t('automation.truncated')}</p>{/if}
            <ul>
              {#each preview.cards as c (c.card_id)}
                <li class:changes={c.changes}>
                  <span class="text">{c.text}</span>
                  <span class="muted">{c.deck}</span>
                  {#if c.changes}<Chip size="sm" tone="accent" label={t('automation.would_change')} />{/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <div class="buttons">
          <Button size="sm" variant="ghost" onclick={() => (editing = null)}>{t('automation.cancel')}</Button>
          <Button size="sm" onclick={runPreview}>{t('automation.preview')}</Button>
          <Button size="sm" type="submit" variant="primary">{t('automation.save')}</Button>
        </div>
      </form>
    </SettingsSection>
  {/if}

  <SettingsSection title={t('automation.rules')} icon={Zap} body="rows">
    {#if !loaded}
      <div class="pad"><Skeleton lines={3} /></div>
    {:else if rules.length === 0}
      <div class="empty-rows"><EmptyState size="sm" icon={Zap} title={t('automation.none')} description={t('automation.none_hint')} /></div>
    {:else}
      <ol class="rules">
        {#each rules as rule, i (rule.id)}
          <li class:off={!rule.enabled}>
            <span class="idx num" aria-hidden="true">{formatCount(i + 1)}</span>
            <div class="rule-text">
              <span class="rule-name">{rule.name}</span>
              <span class="detail">
                <span>{triggerText(rule)}</span>
                {#if rule.condition}<code>{rule.condition}</code>{/if}
                <span class="acts"><Icon icon={ChevronRight} size={12} />{rule.actions.map((a) => td(`automation.action.${a.method}`)).join(' · ')}</span>
              </span>
            </div>
            <div class="rule-actions">
              <IconButton size="sm" icon={ChevronUp} label={t('automation.up')} disabled={i === 0} onclick={() => move(i, -1)} />
              <IconButton size="sm" icon={ChevronDown} label={t('automation.down')} disabled={i === rules.length - 1} onclick={() => move(i, 1)} />
              <IconButton size="sm" icon={Play} label={t('automation.run_now')} onclick={() => runNow(rule)} />
              <IconButton size="sm" icon={Pencil} label={t('automation.edit')} onclick={() => open(rule)} />
              <IconButton size="sm" icon={Trash2} label={t('automation.delete')} onclick={() => remove(rule)} />
              <Switch ariaLabel={rule.name} checked={rule.enabled} onchange={() => toggle(rule)} />
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  </SettingsSection>

  <SettingsSection title={t('automation.runs')} icon={History} body="rows">
    {#if !loaded}
      <div class="pad"><Skeleton lines={2} /></div>
    {:else if runs.length === 0}
      <p class="muted row">{t('automation.no_runs')}</p>
    {:else}
      <ul class="runs">
        {#each runs as r (r.seq)}
          <li>
            <span class="dot {RUN_TONE[r.status] ?? 'warning'}" aria-hidden="true"></span>
            <span class="run-text">
              <strong>{ruleName(r.rule_id)}</strong> · {td(`automation.status.${r.status}`)} · {td(`automation.trigger.${r.trigger}`)}
              · {t('automation.matched', { n: r.matched })}
            </span>
            {#if r.error}<span class="muted err">{r.error.message}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>
</SettingsPage>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid color-mix(in srgb, var(--warning) 35%, var(--border));
    border-radius: var(--radius-control);
    background: var(--warning-soft);
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .banner :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .muted.row {
    display: flex;
    align-items: center;
    min-height: 44px;
    border-bottom: 1px solid var(--border);
  }
  .pad {
    padding: var(--space-3) 0;
  }
  .empty-rows {
    border-bottom: 1px solid var(--border);
  }
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .name-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-4);
    align-items: end;
  }
  .name-row :global(.fc-switch) {
    padding-bottom: var(--space-1);
  }
  .step {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: var(--space-3) 0 0 28px;
    border: none;
    border-top: 1px solid var(--border);
    min-width: 0;
  }
  .step legend {
    float: left;
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-inline-start: -28px;
    padding: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .step legend + :global(*) {
    clear: both;
  }
  .step .num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    color: var(--accent-text);
    font-size: var(--text-2xs);
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
  }
  .action {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .action :global(.fc-field) {
    flex: 1 1 11rem;
    min-width: 0;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .preview {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-1);
  }
  .summary {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .preview ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 16rem;
    overflow-y: auto;
  }
  .preview li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 28px;
    font-size: var(--text-sm);
  }
  .preview li + li {
    border-top: 1px solid var(--border);
  }
  .preview li.changes .text {
    font-weight: var(--weight-semibold);
  }
  .rules,
  .runs {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .rules li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 48px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
  }
  .rules li.off .rule-name {
    color: var(--text-2);
  }
  .idx {
    flex: none;
    width: 1.25rem;
    color: var(--text-2);
    font-size: var(--text-xs);
    text-align: end;
  }
  .rule-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 14rem;
    min-width: 0;
  }
  .rule-name {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .detail {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    flex-wrap: wrap;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .acts {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    padding: 0 var(--space-1);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text-1);
  }
  .rule-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .rule-actions :global(.fc-switch) {
    margin-inline-start: var(--space-2);
  }
  .runs li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
    font-size: var(--text-sm);
  }
  .run-text {
    flex: 1 1 14rem;
    min-width: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--warning-solid);
    flex: none;
  }
  .dot.success {
    background: var(--success-solid);
  }
  .dot.neutral {
    background: var(--border-strong);
  }
  .dot.danger {
    background: var(--danger-solid);
  }
  .err {
    flex-basis: 100%;
    padding-inline-start: calc(8px + var(--space-3));
    font-size: var(--text-xs);
  }
  @media (max-width: 639.98px) {
    .rules li {
      flex-wrap: wrap;
    }
  }
</style>
