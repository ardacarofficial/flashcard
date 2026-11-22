<!--
  Rule editor (v1.3): the rule reads as one sentence — "<when>, for <which cards>, <do>." —
  and each underlined part opens its own small control right under the sentence. A live
  preview (`automation.preview`, a dry run that writes nothing) says how many cards the rule
  would change right now. The full trigger / query / actions editor stays under
  "Advanced: write the rule by hand" and edits the same draft.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { toApiError } from '$lib/api/client';
  import type { ActionInfo, NoteTypeDto, Rating, RulePreview } from '$lib/api/types';
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Checkbox,
    Icon,
    IconButton,
    Input,
    Pencil,
    Plus,
    Select,
    Spinner,
    Switch,
    TriangleAlert,
    X,
  } from '$lib/ui';
  import { SettingsAdvanced, SettingsSection } from '../core/kit';
  import { TRIGGER_KINDS, emptyAction, fitsTrigger } from './actions';
  import {
    COND_KINDS,
    actionsText,
    buildCondition,
    cardsText,
    draftOf,
    missing,
    parseCondition,
    switchKind,
    whenText,
    type CondKind,
    type RuleForm,
    type SentenceEnv,
  } from './recipes';

  interface Props {
    form: RuleForm;
    /** Saved rule being edited (`null`: a new one). */
    ruleId: string | null;
    actionInfos: ActionInfo[];
    notetypes: NoteTypeDto[];
    env: SentenceEnv;
    /** Part to open at first (a recipe that still needs a choice). */
    startPart?: 'when' | 'cards' | 'do' | null;
    onsave: () => void;
    oncancel: () => void;
  }

  let { form = $bindable(), ruleId, actionInfos, notetypes, env, startPart = null, onsave, oncancel }: Props = $props();

  const RATINGS: Rating[] = ['again', 'hard', 'good', 'easy'];
  const FLAGS = [1, 2, 3, 4, 5, 6, 7];
  const UNITS = [
    { value: 1, key: 'minutes' },
    { value: 60, key: 'hours' },
    { value: 1440, key: 'days' },
  ] as const;
  const uid = $props.id();

  type Part = 'when' | 'cards' | 'do';
  let open = $state<Part | null>(null);
  onMount(() => {
    open = startPart;
  });

  // ---------------------------------------------------------------- choices
  const inbox = $derived(form.trigger.kind === 'inbox_added');
  const fitting = $derived(actionInfos.filter((i) => fitsTrigger(i.selection, form.trigger.kind)));
  $effect(() => {
    const ok = new Set(fitting.map((i) => i.method));
    if (fitting.length > 0 && form.actions.some((a) => !ok.has(a.method))) {
      form.actions = form.actions.map((a) => (ok.has(a.method) ? a : emptyAction(fitting[0]!.method)));
    }
  });
  const choiceAttrs = $derived(env.attrs);
  const triggerOptions = $derived(TRIGGER_KINDS.map((k) => ({ value: k, label: t(`automation.trigger.${k}`) })));
  const actionOptions = $derived(fitting.map((info) => ({ value: info.method, label: td(info.label_key) })));
  const flagOptions = $derived(FLAGS.map((f) => ({ value: f, label: td(`browse.flags.${f}`) })));
  const deckOptions = $derived(env.decks.map((d) => ({ value: d.id, label: d.path })));
  const optionalDeckOptions = $derived([{ value: '', label: '—' }, ...deckOptions]);
  const attrOptions = $derived(choiceAttrs.map((d) => ({ value: d.id, label: d.name })));
  const valueOptions = (attrId: string) => [
    { value: '', label: t('automation.clear') },
    ...(choiceAttrs.find((d) => d.id === attrId)?.options ?? []).map((o) => ({ value: o.id, label: o.label })),
  ];
  const unitOptions = $derived(UNITS.map((u) => ({ value: u.value, label: t(`automation.schedule.${u.key}`) })));

  // ---------------------------------------------------------------- which cards
  const cond = $derived(parseCondition(form.condition));
  const kindOptions = $derived(
    COND_KINDS.map((k) => ({
      value: k,
      label: k === 'all' && form.trigger.kind !== 'schedule' ? t('automation.kind.event') : td(`automation.kind.${k}`),
    })),
  );
  const notetypeOptions = $derived(notetypes.map((n) => ({ value: n.name, label: n.name })));
  const deckPathOptions = $derived(env.decks.map((d) => ({ value: d.path, label: d.path })));
  const condDefaults = $derived({
    tag: form.actions.find((a) => a.tags.trim())?.tags.split(',')[0]?.trim() ?? '',
    notetype: env.subtitleType,
    deck: env.decks[0]?.path ?? '',
  });
  function setKind(kind: CondKind) {
    form.condition = buildCondition(switchKind(cond, kind, condDefaults));
  }
  function setCond(patch: Partial<typeof cond>) {
    form.condition = buildCondition({ ...cond, ...patch });
  }
  function setN(raw: string) {
    const n = Math.round(Number(raw));
    if (raw.trim() !== '' && Number.isFinite(n) && n >= 1) setCond({ n });
  }

  // ---------------------------------------------------------------- schedule
  const unit = $derived([...UNITS].reverse().find((u) => form.trigger.everyMins % u.value === 0)?.value ?? 1);
  function setEvery(count: number, u: number) {
    if (Number.isFinite(count) && count >= 1) form.trigger.everyMins = Math.round(count) * u;
  }

  function toggleIn<T>(list: T[], v: T): T[] {
    return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  }

  // ---------------------------------------------------------------- preview (dry run)
  const blockers = $derived(missing(form).filter((k) => k !== 'missing_name'));
  const draftJson = $derived(JSON.stringify(draftOf(form)));
  let preview = $state<RulePreview | null>(null);
  let matchedOnly = $state<number | null>(null);
  let previewError = $state<string | null>(null);
  let counting = $state(false);
  let seq = 0;
  $effect(() => {
    const json = draftJson;
    const blocked = blockers.length > 0;
    const condition = form.condition;
    const isInbox = inbox;
    const mine = ++seq;
    counting = true;
    const timer = setTimeout(async () => {
      try {
        if (blocked) {
          // Not runnable yet (e.g. no deck chosen): still say how many cards the condition finds.
          const n = isInbox ? null : (await api.call('search.ids', { query: condition })).card_ids.length;
          if (mine !== seq) return;
          preview = null;
          matchedOnly = n;
        } else {
          const p = await api.call('automation.preview', { rule_id: null, draft: JSON.parse(json) });
          if (mine !== seq) return;
          preview = p;
          matchedOnly = null;
        }
        previewError = null;
      } catch (e) {
        if (mine !== seq) return;
        preview = null;
        matchedOnly = null;
        previewError = td(toApiError(e).messageKey);
      } finally {
        if (mine === seq) counting = false;
      }
    }, 300);
    return () => clearTimeout(timer);
  });
  const SAMPLES = 5;
  const samples = $derived(preview ? preview.cards.slice(0, SAMPLES) : []);
  const editingAutoAccept = $derived(inbox && form.actions.some((a) => a.method === 'inbox.accept'));

  // ---------------------------------------------------------------- the sentence
  const pattern = $derived(t('automation.sentence.pattern').split(/(\{when\}|\{cards\}|\{action\})/));
  const partText = $derived({
    when: whenText(form.trigger),
    cards: cardsText(form.condition, form.trigger, env),
    do: actionsText(form.actions, env),
  });
  const PART_OF: Record<string, Part> = { '{when}': 'when', '{cards}': 'cards', '{action}': 'do' };
  const PART_TITLE = {
    when: () => t('automation.editor.part_when'),
    cards: () => t('automation.editor.part_cards'),
    do: () => t('automation.editor.part_do'),
  };
  const canSave = $derived(missing(form).length === 0);
</script>

{#snippet whenControls()}
  <Select label={t('automation.trigger.label')} options={triggerOptions} bind:value={form.trigger.kind} />
  {#if form.trigger.kind === 'card_answered'}
    <div class="checks" role="group" aria-label={t('automation.trigger.card_answered')}>
      {#each RATINGS as r (r)}
        <Checkbox
          label={t(`automation.rating.${r}`)}
          checked={form.trigger.ratings.includes(r)}
          onchange={() => (form.trigger.ratings = toggleIn(form.trigger.ratings, r))}
        />
      {/each}
    </div>
  {:else if form.trigger.kind === 'card_flagged'}
    <div class="checks" role="group" aria-label={t('automation.trigger.card_flagged')}>
      {#each FLAGS as f (f)}
        <Checkbox
          label={td(`browse.flags.${f}`)}
          checked={form.trigger.flags.includes(f)}
          onchange={() => (form.trigger.flags = toggleIn(form.trigger.flags, f))}
        />
      {/each}
    </div>
  {:else if form.trigger.kind === 'schedule'}
    <div class="inline">
      <Input
        type="number"
        min="1"
        label={t('automation.schedule.every')}
        value={form.trigger.everyMins / unit}
        oninput={(e) => setEvery(Number(e.currentTarget.value), unit)}
        hint={t('automation.schedule.hint')}
      />
      <Select
        label={t('automation.schedule.unit')}
        options={unitOptions}
        value={unit}
        onchange={(u: number) => setEvery(form.trigger.everyMins / unit, u)}
      />
    </div>
  {/if}
{/snippet}

{#snippet cardsControls()}
  {#if inbox}
    <Input label={t('automation.condition')} hint={t('automation.inbox_condition_hint')} bind:value={form.condition} />
  {:else}
    <Select label={t('automation.kind.label')} options={kindOptions} value={cond.kind} onchange={(k: CondKind) => setKind(k)} />
    {#if cond.kind === 'lapses'}
      <Input type="number" min="1" label={t('automation.kind.lapses_n')} value={cond.n} oninput={(e) => setN(e.currentTarget.value)} />
    {:else if cond.kind === 'stability'}
      <Input
        type="number"
        min="1"
        label={t('automation.kind.stability_n')}
        hint={t('automation.kind.stability_hint')}
        value={cond.n}
        oninput={(e) => setN(e.currentTarget.value)}
      />
    {:else if cond.kind === 'tag_stability'}
      <div class="inline">
        <Input label={t('automation.kind.tag_name')} value={cond.tag} oninput={(e) => setCond({ tag: e.currentTarget.value })} />
        <Input type="number" min="1" label={t('automation.kind.stability_n')} value={cond.n} oninput={(e) => setN(e.currentTarget.value)} />
      </div>
    {:else if cond.kind === 'tag'}
      <Input label={t('automation.kind.tag_name')} value={cond.tag} oninput={(e) => setCond({ tag: e.currentTarget.value })} />
    {:else if cond.kind === 'notetype'}
      <Select label={t('automation.kind.notetype_name')} options={notetypeOptions} value={cond.name} onchange={(name: string) => setCond({ name })} />
    {:else if cond.kind === 'deck'}
      <Select label={t('automation.kind.deck_name')} options={deckPathOptions} value={cond.name} onchange={(name: string) => setCond({ name })} />
    {:else if cond.kind === 'flag'}
      <Select label={t('automation.kind.flag_name')} options={flagOptions} value={cond.flag} onchange={(flag: number) => setCond({ flag })} />
    {:else if cond.kind === 'query'}
      <Input label={t('automation.condition')} hint={t('automation.kind.query_hint')} bind:value={form.condition} />
    {/if}
  {/if}
{/snippet}

{#snippet actionControls()}
  {#each form.actions as a, i (i)}
    <div class="action">
      <Select label={t('automation.action.label')} hideLabel options={actionOptions} bind:value={a.method} />
      {#if a.method === 'bulk.add_tags' || a.method === 'bulk.remove_tags'}
        <Input label={t('automation.tags')} hideLabel placeholder={t('automation.tags')} bind:value={a.tags} />
      {:else if a.method === 'bulk.set_suspended' || a.method === 'bulk.set_buried'}
        <Checkbox label={t('automation.on')} bind:checked={a.on} />
      {:else if a.method === 'bulk.set_flag'}
        <Select label={t('automation.flag')} hideLabel options={flagOptions} bind:value={a.flag} />
        <Checkbox label={t('automation.on')} bind:checked={a.on} />
      {:else if a.method === 'bulk.move_cards'}
        <Select label={t('automation.deck')} hideLabel options={deckOptions} bind:value={a.deckId} />
      {:else if a.method === 'inbox.accept'}
        <Select label={t('automation.deck')} hideLabel options={optionalDeckOptions} bind:value={a.deckId} />
      {:else if a.method === 'bulk.set_attribute'}
        <Select label={t('automation.classification')} hideLabel options={attrOptions} bind:value={a.attrId} />
        <Select label={t('automation.value')} hideLabel options={valueOptions(a.attrId)} bind:value={a.optionId} />
      {/if}
      {#if form.actions.length > 1}
        <IconButton icon={X} label={t('automation.remove_action')} onclick={() => (form.actions = form.actions.filter((_, j) => j !== i))} />
      {/if}
    </div>
  {/each}
  <div>
    <Button
      size="sm"
      variant="ghost"
      icon={Plus}
      disabled={form.actions.length >= 10}
      onclick={() => (form.actions = [...form.actions, emptyAction(fitting[0]?.method ?? 'bulk.add_tags')])}
    >
      {t('automation.add_action')}
    </Button>
  </div>
{/snippet}

<SettingsSection title={ruleId ? t('automation.editor.edit_title') : t('automation.editor.new_title')} icon={Pencil}>
  <form
    class="editor"
    onsubmit={(e) => {
      e.preventDefault();
      if (canSave) onsave();
    }}
  >
    <Input label={t('automation.editor.name')} hint={t('automation.editor.name_hint')} bind:value={form.name} />

    <div class="sentence-block">
      <span class="block-label" id="{uid}-sentence">{t('automation.editor.sentence_label')}</span>
      <p class="sentence" aria-labelledby="{uid}-sentence" data-testid="rule-sentence">
        <!-- One line per piece: whitespace between the parts would show before the commas. -->
        {#each pattern as piece, i (i)}{@const part = PART_OF[piece]}{#if part}<button
              type="button"
              class="part"
              class:active={open === part}
              aria-expanded={open === part}
              aria-controls="{uid}-panel"
              title={PART_TITLE[part]()}
              onclick={() => (open = open === part ? null : part)}>{partText[part]}</button
            >{:else}{piece}{/if}{/each}
      </p>
      <p class="hint">{t('automation.editor.sentence_hint')}</p>

      {#if open}
        <div class="panel" id="{uid}-panel" role="group" aria-label={PART_TITLE[open]()}>
          <p class="panel-title">{PART_TITLE[open]()}</p>
          {#if open === 'when'}
            {@render whenControls()}
          {:else if open === 'cards'}
            {@render cardsControls()}
          {:else}
            {@render actionControls()}
          {/if}
          <div class="panel-done">
            <Button size="sm" onclick={() => (open = null)}>{t('automation.editor.done')}</Button>
          </div>
        </div>
      {/if}
    </div>

    <section class="preview" aria-label={t('automation.preview.title')} aria-live="polite">
      {#if counting && !preview && matchedOnly === null && !previewError}
        <p class="count muted"><Spinner size={14} /> {t('automation.preview.counting')}</p>
      {:else if previewError}
        <p class="count error">{t('automation.preview.error', { error: previewError })}</p>
      {:else if blockers.length}
        {#each blockers as b (b)}<p class="count warn"><Icon icon={TriangleAlert} size={14} />{td(`automation.editor.${b}`)}</p>{/each}
        {#if matchedOnly !== null}<p class="muted">{t('automation.recipes.matching', { n: matchedOnly })}</p>{/if}
      {:else if preview}
        <p class="count" role="status" data-testid="preview-count">
          {inbox ? t('automation.preview.items_affected', { n: preview.changed }) : t('automation.preview.affected', { n: preview.changed })}
        </p>
        {#if !inbox && preview.matched > preview.changed}
          <p class="muted">{t('automation.preview.matched', { matched: formatCount(preview.matched), same: formatCount(preview.matched - preview.changed) })}</p>
        {/if}
        {#if samples.length}
          <p class="samples-label">{t('automation.preview.samples')}</p>
          <ul class="samples">
            {#each samples as c (c.card_id)}
              <li class:same={!c.changes}><span class="text">{c.text}</span><span class="deck">{c.deck}</span></li>
            {/each}
          </ul>
          {#if preview.matched > samples.length}
            <p class="muted">{t('automation.preview.more', { n: preview.matched - samples.length })}</p>
          {/if}
        {/if}
        {#if preview.truncated}<p class="muted">{t('automation.preview.truncated')}</p>{/if}
        <p class="note">
          {form.trigger.kind === 'schedule' ? t('automation.preview.schedule_note') : t('automation.preview.event_note')}
          {t('automation.preview.dry_run')}
        </p>
      {/if}
    </section>

    {#if editingAutoAccept}
      <p class="banner" role="note"><Icon icon={TriangleAlert} size={16} /><span>{t('automation.auto_accept_warning')}</span></p>
    {/if}

    <Switch label={t('automation.editor.enabled')} description={t('automation.editor.enabled_hint')} bind:checked={form.enabled} />

    <SettingsAdvanced id="automation.raw" title={t('automation.raw.title')} summary={t('automation.raw.summary')}>
      <div class="raw">
        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">1</span>{t('automation.when')}</legend>
          {@render whenControls()}
        </fieldset>
        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">2</span>{t('automation.if')}</legend>
          <Input
            label={t('automation.condition')}
            placeholder={t('automation.condition_example')}
            hint={inbox ? t('automation.inbox_condition_hint') : t('automation.kind.query_hint')}
            bind:value={form.condition}
          />
        </fieldset>
        <fieldset class="step">
          <legend><span class="num" aria-hidden="true">3</span>{t('automation.do')}</legend>
          {@render actionControls()}
        </fieldset>
      </div>
    </SettingsAdvanced>

    <div class="buttons">
      {#if !form.name.trim()}<span class="muted">{t('automation.editor.missing_name')}</span>{/if}
      <Button size="sm" variant="ghost" onclick={oncancel}>{t('automation.editor.cancel')}</Button>
      <Button size="sm" type="submit" variant="primary" disabled={!canSave}>{t('automation.editor.save')}</Button>
    </div>
  </form>
</SettingsSection>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .sentence-block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .block-label {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .sentence {
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    font-size: var(--text-lg);
    line-height: 1.9;
    color: var(--text-1);
  }
  .part {
    display: inline;
    padding: 1px 2px;
    margin: 0 -2px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--accent-text);
    font: inherit;
    font-weight: var(--weight-medium);
    text-decoration: underline dashed;
    text-decoration-thickness: 1px;
    text-underline-offset: 4px;
    cursor: pointer;
    -webkit-box-decoration-break: clone;
    box-decoration-break: clone;
  }
  .part:hover,
  .part.active {
    background: var(--accent-soft);
    text-decoration-style: solid;
  }
  .part:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .hint {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-2);
  }
  .panel :global(.fc-field) {
    max-width: 32rem;
  }
  .panel .inline :global(.fc-field) {
    max-width: 16rem;
  }
  .panel-title {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .panel-done {
    display: flex;
    justify-content: flex-end;
  }
  .inline {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    align-items: flex-start;
  }
  .inline :global(.fc-field) {
    flex: 1 1 10rem;
    min-width: 0;
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
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-inline-start: 3px solid var(--accent);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    min-height: 3rem;
  }
  .count {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .count.warn :global(svg) {
    color: var(--warning);
  }
  .count.error {
    color: var(--danger);
    font-weight: var(--weight-medium);
  }
  .muted {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .count.muted {
    font-weight: var(--weight-regular);
  }
  .samples-label {
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .samples {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .samples li {
    display: flex;
    gap: var(--space-3);
    align-items: baseline;
    min-height: 28px;
    padding: 2px 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .samples li:last-child {
    border-bottom: none;
  }
  .samples .text {
    font-weight: var(--weight-medium);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .samples .deck {
    color: var(--text-2);
    font-size: var(--text-xs);
    margin-inline-start: auto;
    white-space: nowrap;
  }
  .samples li.same .text {
    color: var(--text-2);
    font-weight: var(--weight-regular);
  }
  .note {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
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
  .raw {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
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
  .buttons {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
</style>
