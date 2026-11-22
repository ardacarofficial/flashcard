<!--
  Settings → Automation (v1.3). Starts from ready rules (recipes) that say how many cards they
  would change right now; a rule reads as one sentence with editable parts (RuleEditor); the
  raw trigger / query / actions editor stays under "Advanced". Rules list, readable run
  history and the loop limits (`automation.max_depth`, `automation.max_runs_per_event`) under
  the page's advanced disclosure. Everything a rule is and does is decided by fc-app.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ActionInfo, AttributeDefDto, DeckTreeNode, NoteTypeDto, RuleDto, RunDto, SettingDef } from '$lib/api/types';
  import { formatDateTime } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { fromAction, fromTrigger } from './actions';
  import RuleEditor from './RuleEditor.svelte';
  import {
    RECIPES,
    draftOf,
    missing,
    recipeEnv,
    sentence,
    type Recipe,
    type RecipeId,
    type RuleForm,
    type SentenceEnv,
  } from './recipes';
  import {
    Button,
    ChevronDown,
    ChevronUp,
    Chip,
    CircleCheck,
    Dialog,
    EmptyState,
    History,
    Icon,
    IconButton,
    Input,
    Pencil,
    Play,
    Plus,
    Skeleton,
    Sparkles,
    Switch,
    Trash2,
    TriangleAlert,
    Zap,
    toast,
  } from '$lib/ui';
  import type { Tone } from '$lib/ui';
  import { FormRow, SettingsAdvanced, SettingsPage, SettingsSection, isDefault, loadDefaults, resetSetting } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let rules = $state<RuleDto[]>([]);
  let loaded = $state(false);
  let runs = $state<RunDto[]>([]);
  let actionInfos = $state<ActionInfo[]>([]);
  let attrs = $state<AttributeDefDto[]>([]);
  let notetypes = $state<NoteTypeDto[]>([]);

  // Editor (null = closed).
  let editing = $state<{ id: string | null; start?: 'do' | null } | null>(null);
  let form = $state<RuleForm>({ name: '', enabled: true, trigger: fromTrigger({ kind: 'note_added' }), condition: '', actions: [] });

  let confirmDelete = $state<RuleDto | null>(null);

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
  const choiceAttrs = $derived(
    attrs
      .filter((a) => a.kind === 'ordered_enum' || a.kind === 'enum')
      .map((a) => ({ id: a.id, name: a.name, options: a.options.map((o) => ({ id: o.id, label: o.label })) })),
  );
  const rEnv = $derived(recipeEnv(notetypes, decks));
  const env = $derived<SentenceEnv>({ decks, attrs: choiceAttrs, subtitleType: rEnv.subtitleType });

  // Rules that accept inbox proposals without the user's review (integration round 1): a
  // program paired with the local API (or a plugin) then writes notes straight into the
  // collection, bounded only by its daily proposal limit.
  const autoAccepting = $derived(
    rules.filter((r) => r.enabled && r.trigger.kind === 'inbox_added' && r.actions.some((a) => a.method === 'inbox.accept')),
  );

  async function load() {
    await ctx.guarded(async () => {
      rules = (await api.call('automation.rules.list', {})).rules;
      runs = (await api.call('automation.runs.list', { rule_id: null, limit: 20 })).runs;
    });
    loaded = true;
  }

  // ---------------------------------------------------------------- recipes
  type Count = { kind: 'affected' | 'matching'; n: number } | 'error';
  let counts = $state<Partial<Record<RecipeId, Count>>>({});
  let countRound = 0;

  /** How many cards each recipe would change right now (dry runs, one after another). */
  async function countRecipes() {
    const round = ++countRound;
    for (const r of RECIPES) {
      const f = r.form(rEnv);
      let c: Count;
      try {
        if (missing(f).length) {
          c = { kind: 'matching', n: (await api.call('search.ids', { query: f.condition })).card_ids.length };
        } else {
          const p = await api.call('automation.preview', { rule_id: null, draft: $state.snapshot(draftOf(f)) });
          c = { kind: 'affected', n: p.changed };
        }
      } catch {
        c = 'error';
      }
      if (round !== countRound) return;
      counts[r.id] = c;
    }
  }

  /** Key-order independent JSON (rules come back from Rust, drafts from the form). */
  const stable = (v: unknown): string =>
    Array.isArray(v)
      ? `[${v.map(stable).join(',')}]`
      : v && typeof v === 'object'
        ? `{${Object.keys(v)
            .sort()
            .map((k) => `${JSON.stringify(k)}:${stable((v as Record<string, unknown>)[k])}`)
            .join(',')}}`
        : JSON.stringify(v);
  const saved = $derived(new Set(rules.map((r) => stable({ trigger: r.trigger, condition: r.condition, actions: r.actions }))));
  /** A recipe the user already saved as it is. */
  const inRules = (r: Recipe) => saved.has(stable($state.snapshot(draftOf(r.form(rEnv)))));

  function countText(id: RecipeId): string {
    const c = counts[id];
    if (c === undefined) return t('automation.recipes.counting');
    if (c === 'error') return t('automation.recipes.no_count');
    return c.kind === 'affected' ? t('automation.preview.affected', { n: c.n }) : t('automation.recipes.matching', { n: c.n });
  }

  // ---------------------------------------------------------------- editor
  function openRecipe(r: Recipe) {
    form = r.form(rEnv);
    // A recipe that still needs a choice (the archive deck) opens where it is made.
    editing = { id: null, start: missing(form).some((k) => k !== 'missing_name') ? 'do' : null };
  }

  function openBlank() {
    form = {
      name: '',
      enabled: true,
      trigger: fromTrigger({ kind: 'card_answered', ratings: [] }),
      condition: '',
      actions: [fromAction({ method: actionInfos[0]?.method ?? 'bulk.add_tags', params: {} })],
    };
    editing = { id: null };
  }

  function openRule(rule: RuleDto) {
    form = {
      name: rule.name,
      enabled: rule.enabled,
      trigger: fromTrigger(rule.trigger),
      condition: rule.condition,
      actions: rule.actions.map(fromAction),
    };
    editing = { id: rule.id };
  }

  async function save() {
    if (!editing) return;
    const id = editing.id;
    const d = $state.snapshot(draftOf(form));
    const name = form.name.trim();
    const enabled = form.enabled;
    const ok = await ctx.guarded(() =>
      id
        ? api.call('automation.rules.update', { id, name, enabled, ...d })
        : api.call('automation.rules.create', { name, enabled, ...d }),
    );
    if (ok) editing = null;
    await load();
  }

  // ---------------------------------------------------------------- rules
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
    await ctx.guarded(async () => {
      const res = await api.call('automation.rules.run', { id: rule.id });
      const own = res.runs[0];
      if (own) toast.show(t('automation.rules.ran', { n: own.matched }), { tone: own.status === 'failed' ? 'danger' : 'success' });
    });
    await load();
    void countRecipes();
  }

  async function remove(rule: RuleDto) {
    confirmDelete = null;
    await ctx.guarded(() => api.call('automation.rules.delete', { id: rule.id }));
    await load();
  }

  const ruleSentence = (r: RuleDto) =>
    sentence({ trigger: fromTrigger(r.trigger), condition: r.condition, actions: r.actions.map(fromAction) }, env);

  // ---------------------------------------------------------------- run history
  const RUN_TONE: Record<string, Tone> = { applied: 'success', no_match: 'neutral', failed: 'danger' };
  const ACTION_KEYS = new Set(['add_tags', 'remove_tags', 'set_suspended', 'set_buried', 'set_flag', 'move_cards', 'set_attribute', 'reset_progress', 'accept']);

  function runWhen(at: number): string {
    const d = new Date(at);
    const today = new Date();
    const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
    const time = formatDateTime(at, { hour: '2-digit', minute: '2-digit' });
    if (d.toDateString() === today.toDateString()) return t('automation.runs.today', { time });
    if (d.toDateString() === yesterday.toDateString()) return t('automation.runs.yesterday', { time });
    return formatDateTime(at);
  }

  function errorText(r: RunDto): string {
    if (!r.error) return '';
    const s = td(r.error.message_key);
    return s === r.error.message_key ? r.error.message : s;
  }

  function runOutcome(r: RunDto): string {
    const n = r.matched;
    switch (r.status) {
      case 'applied': {
        const rule = rules.find((x) => x.id === r.rule_id);
        const key = rule?.actions.length === 1 ? rule.actions[0]!.method.split('.').pop()! : '';
        return ACTION_KEYS.has(key) ? td(`automation.runs.applied_action.${key}`, { n }) : t('automation.runs.applied', { n });
      }
      case 'failed':
        return t('automation.runs.failed', { error: errorText(r) });
      case 'no_match':
      case 'skipped_cycle':
      case 'skipped_depth':
      case 'skipped_budget':
        return td(`automation.runs.${r.status}`);
      default:
        return td(`automation.status.${r.status}`);
    }
  }

  const ruleName = (id: string) => rules.find((r) => r.id === id)?.name ?? t('automation.runs.deleted_rule');

  // ---------------------------------------------------------------- loop limits (settings)
  const SETTING_KEYS = ['automation.max_depth', 'automation.max_runs_per_event'];
  let settingDefs = $state<SettingDef[]>([]);
  let values = $state<Record<string, number>>({});

  async function loadSettings() {
    try {
      const schema = (await api.call('settings.schema', {})).settings.map((e) => e.def);
      const defs = SETTING_KEYS.map((k) => schema.find((d) => d.key === k)).filter((d): d is SettingDef => !!d);
      const got = defs.length ? (await api.call('settings.get', { scope: { kind: 'app' }, keys: defs.map((d) => d.key) })).values : [];
      const next: Record<string, number> = {};
      for (const d of defs) next[d.key] = Number(d.default);
      for (const v of got) if (typeof v.value === 'number') next[v.key] = v.value;
      values = next;
      settingDefs = defs;
    } catch {
      settingDefs = [];
    }
  }

  async function commitSetting(def: SettingDef, raw: string) {
    const n = Math.round(Number(raw));
    if (raw.trim() === '' || !Number.isFinite(n) || n === values[def.key]) return;
    const before = values[def.key];
    values[def.key] = n;
    const ok = await ctx.guarded(() => api.call('settings.set', { scope: { kind: 'app' }, key: def.key, value: n }));
    if (!ok && before !== undefined) values[def.key] = before;
  }

  async function resetValue(def: SettingDef) {
    await ctx.guarded(async () => {
      const v = await resetSetting(def.key);
      values[def.key] = typeof v === 'number' ? v : Number(def.default);
    });
  }

  const bounds = (d: SettingDef) => (d.kind.type === 'integer' ? { min: d.kind.min ?? undefined, max: d.kind.max ?? undefined } : {});
  const exampleKey = (d: SettingDef) => d.label_key.replace(/\.label$/, '.example');

  onMount(() => {
    void load();
    void loadDefaults();
    void loadSettings();
    // Recipes are counted once the collection's note types are known (the Subtitle recipe
    // names the built-in Subtitle note type). Note types are optional: a module may be off.
    void Promise.all([
      ctx.guarded(async () => (actionInfos = (await api.call('automation.actions', {})).actions)),
      api.call('notetypes.list', {}).then(
        (r) => (notetypes = r.notetypes),
        () => (notetypes = []),
      ),
    ]).then(() => countRecipes());
    // Classifications are optional (module may be off): no error if unavailable.
    api.call('attributes.list', { scope_deck_id: null, include_deleted: false }).then(
      (r) => (attrs = r.defs),
      () => (attrs = []),
    );
    const unsub = api.on(['automation_changed', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

{#snippet headerActions()}
  {#if !editing}
    <Button variant="secondary" icon={Plus} onclick={openBlank}>{t('automation.new')}</Button>
  {/if}
{/snippet}

{#snippet recipesSection()}
  <SettingsSection title={t('automation.recipes.title')} description={t('automation.recipes.description')} icon={Sparkles}>
    <ul class="recipes">
      {#each RECIPES as r (r.id)}
        <li>
          <button type="button" class="recipe" data-testid="recipe-{r.id}" onclick={() => openRecipe(r)}>
            <span class="r-icon"><Icon icon={r.icon} size={18} /></span>
            <span class="r-body">
              <span class="r-name">{r.name()}</span>
              <span class="r-desc">{r.description()}</span>
              {#if inRules(r)}<span class="r-exists"><Icon icon={CircleCheck} size={14} />{t('automation.recipes.exists')}</span>{/if}
              <span class="r-count" class:zero={counts[r.id] !== undefined && counts[r.id] !== 'error' && (counts[r.id] as { n: number }).n === 0}>
                {countText(r.id)}
              </span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  </SettingsSection>
{/snippet}

{#snippet rulesSection()}
  <SettingsSection title={t('automation.rules.title')} description={rules.length ? t('automation.rules.description') : undefined} icon={Zap} body="rows">
    {#if !loaded}
      <div class="pad"><Skeleton lines={3} /></div>
    {:else if rules.length === 0}
      <div class="empty-rows">
        <EmptyState size="sm" icon={Zap} title={t('automation.rules.none')} description={t('automation.rules.none_hint')} />
      </div>
    {:else}
      <ol class="rules">
        {#each rules as rule, i (rule.id)}
          <li class:off={!rule.enabled} data-testid="rule-row">
            <div class="rule-text">
              <span class="rule-head">
                <span class="rule-name">{rule.name}</span>
                {#if !rule.enabled}<Chip size="sm" label={t('automation.rules.off')} />{/if}
              </span>
              <span class="rule-sentence">{ruleSentence(rule)}</span>
            </div>
            <div class="rule-actions">
              {#if rules.length > 1}
                <IconButton size="sm" icon={ChevronUp} label={t('automation.up')} disabled={i === 0} onclick={() => move(i, -1)} />
                <IconButton size="sm" icon={ChevronDown} label={t('automation.down')} disabled={i === rules.length - 1} onclick={() => move(i, 1)} />
              {/if}
              <IconButton size="sm" icon={Play} label={t('automation.run_now')} onclick={() => runNow(rule)} />
              <IconButton size="sm" icon={Pencil} label={t('automation.edit')} onclick={() => openRule(rule)} />
              <IconButton size="sm" icon={Trash2} label={t('automation.delete')} onclick={() => (confirmDelete = rule)} />
              <Switch ariaLabel={rule.name} checked={rule.enabled} onchange={() => toggle(rule)} />
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  </SettingsSection>
{/snippet}

<SettingsPage title={t('automation.heading')} subtitle={t('automation.intro')} actions={headerActions}>
  {#if autoAccepting.length > 0}
    <p class="banner" role="note">
      <Icon icon={TriangleAlert} size={16} />
      <span>{t('automation.auto_accept_active', { names: autoAccepting.map((r) => r.name).join(', ') })}</span>
    </p>
  {/if}

  {#if editing}
    <RuleEditor
      bind:form
      ruleId={editing.id}
      startPart={editing.start ?? null}
      {actionInfos}
      {notetypes}
      {env}
      onsave={() => void save()}
      oncancel={() => (editing = null)}
    />
    {@render rulesSection()}
  {:else if !loaded}
    <div class="pad"><Skeleton lines={4} /></div>
  {:else if rules.length > 0}
    {@render rulesSection()}
    {@render recipesSection()}
  {:else}
    {@render recipesSection()}
    {@render rulesSection()}
  {/if}

  <SettingsSection title={t('automation.runs.title')} description={t('automation.runs.description')} icon={History} body="rows">
    {#if !loaded}
      <div class="pad"><Skeleton lines={2} /></div>
    {:else if runs.length === 0}
      <p class="muted row">{t('automation.runs.none')}</p>
    {:else}
      <ul class="runs">
        {#each runs as r (r.seq)}
          <li data-testid="run-row">
            <span class="dot {RUN_TONE[r.status] ?? 'warning'}" aria-hidden="true"></span>
            <span class="run-text">
              <span class="when">{runWhen(r.at)}</span>
              · <strong>“{ruleName(r.rule_id)}”</strong>
              · <span class:err={r.status === 'failed'}>{runOutcome(r)}</span>
              {#if r.trigger}<span class="via"> · {td(`automation.runs.via.${r.trigger}`)}</span>{/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>

  {#if settingDefs.length}
    <SettingsAdvanced id="automation" title={t('automation.advanced.title')} summary={t('automation.advanced.summary')}>
      <SettingsSection body="rows">
        {#each settingDefs as d (d.key)}
          {@const id = `automation-setting-${d.key.replaceAll('.', '-')}`}
          <FormRow
            label={td(d.label_key)}
            description={d.description_key ? td(d.description_key) : undefined}
            example={td(exampleKey(d))}
            labelFor={id}
            isDefault={isDefault(d.key, values[d.key])}
            onReset={() => void resetValue(d)}
          >
            <Input
              {id}
              type="number"
              size="sm"
              hideLabel
              label={td(d.label_key)}
              {...bounds(d)}
              value={values[d.key]}
              onchange={(e) => void commitSetting(d, e.currentTarget.value)}
            />
          </FormRow>
        {/each}
      </SettingsSection>
    </SettingsAdvanced>
  {/if}
</SettingsPage>

<Dialog
  open={confirmDelete !== null}
  role="alertdialog"
  size="sm"
  title={t('automation.rules.delete_title', { name: confirmDelete?.name ?? '' })}
  description={t('automation.rules.delete_body')}
  onclose={() => (confirmDelete = null)}
>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (confirmDelete = null)}>{t('automation.editor.cancel')}</Button>
    <Button variant="danger" onclick={() => confirmDelete && void remove(confirmDelete)}>{t('automation.delete')}</Button>
  {/snippet}
</Dialog>

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
  /* Recipes: a two-column grid of cards (one column when narrow). */
  .recipes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr));
    gap: var(--space-3);
  }
  .recipe {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    width: 100%;
    height: 100%;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background-color var(--motion-fast) var(--ease);
  }
  .recipe:hover {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }
  .recipe:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .r-icon {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--accent-text);
  }
  .r-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .r-name {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }
  .r-desc {
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .r-exists {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .r-exists :global(svg) {
    color: var(--success);
  }
  .r-count {
    margin-top: auto;
    padding-top: var(--space-1);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    color: var(--accent-text);
  }
  .r-count.zero {
    color: var(--text-2);
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
    min-height: 56px;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
  }
  .rule-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 16rem;
    min-width: 0;
  }
  .rule-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .rule-name {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .rules li.off .rule-name,
  .rules li.off .rule-sentence {
    color: var(--text-2);
  }
  .rule-sentence {
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
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
    align-items: baseline;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .run-text {
    flex: 1 1 14rem;
    min-width: 0;
    line-height: var(--leading);
  }
  .run-text strong {
    font-weight: var(--weight-medium);
  }
  .when {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .via {
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--warning-solid);
    flex: none;
    align-self: center;
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
  @media (max-width: 639.98px) {
    .rules li {
      flex-wrap: wrap;
    }
  }
</style>
