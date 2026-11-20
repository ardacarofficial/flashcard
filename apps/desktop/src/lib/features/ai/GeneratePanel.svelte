<script lang="ts">
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type {
    AiNoteDraft,
    AiPreview,
    AiTask,
    AttributeAssignment,
    EffectiveAttributeDto,
    JobDto,
    NoteTypeDto,
  } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { t, td } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import JobProgress from '../io/JobProgress.svelte';
  import { aiResult, previewTask, sendPreviewed, usd } from './ai';
  import PreviewPanel from './PreviewPanel.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let notetypes = $state<NoteTypeDto[]>([]);
  let notetypeId = $state<string>(CLASSIC_NOTETYPE_ID);
  let defs = $state<EffectiveAttributeDto[]>([]);
  let attrs = $state<Record<string, FieldValue>>({});
  let source = $state('');
  let count = $state(10);
  let instructions = $state('');
  let preview = $state.raw<AiPreview | null>(null);
  let task = $state.raw<AiTask | null>(null);
  let job = $state<JobDto | null>(null);
  let drafts = $state<{ keep: boolean; draft: AiNoteDraft }[]>([]);
  let attributes = $state.raw<AttributeAssignment[]>([]);
  let spent = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const notetype = $derived(notetypes.find((n) => n.id === notetypeId) ?? null);
  const fieldName = (key: string) => notetype?.fields.find((f) => f.key === key)?.name ?? key;
  const kept = $derived(drafts.filter((d) => d.keep).length);

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  $effect(() => {
    void api.call('notetypes.list', {}).then((r) => (notetypes = r.notetypes), fail);
  });

  $effect(() => {
    const deck = ctx.selectedDeck;
    defs = [];
    if (!deck) return;
    void api.attribute.effective(deck).then((r) => {
      defs = r.attributes;
      attrs = Object.fromEntries(r.attributes.map((a) => [a.def.id, a.suggested_value ?? initialValue(a.def)]));
    }, fail);
  });

  async function runPreview() {
    error = null;
    notice = null;
    drafts = [];
    const deck = ctx.selectedDeck;
    if (!deck) return;
    const next: AiTask = {
      feature: 'generate_cards',
      deck_id: deck,
      notetype_id: notetypeId,
      source,
      count,
      instructions: instructions.trim() || null,
      images: [],
      attributes: toAssignments($state.snapshot(attrs)),
    };
    try {
      preview = await previewTask(next);
      task = next;
    } catch (e) {
      fail(e);
    }
  }

  async function send() {
    if (!preview || !task) return;
    busy = true;
    error = null;
    try {
      const done = await sendPreviewed(task, preview, undefined, (j) => (job = j));
      preview = null;
      const r = aiResult(done);
      if (!r) {
        error = td(done.error?.message_key ?? 'errors.internal.unexpected');
      } else if (r.output.feature === 'generate_cards') {
        drafts = r.output.notes.map((draft) => ({ keep: true, draft }));
        attributes = r.output.attributes;
        spent = usd(r.cost_usd);
        if (r.truncated) notice = t('ai.generate.truncated');
      }
    } catch (e) {
      fail(e);
    } finally {
      job = null;
      busy = false;
    }
  }

  async function add() {
    const deck = ctx.selectedDeck;
    if (!deck) return;
    const notes = drafts.filter((d) => d.keep).map((d) => $state.snapshot(d.draft));
    const ok = await ctx.guarded(async () => {
      const r = await api.call('ai.notes.add', {
        deck_id: deck,
        notetype_id: notetypeId,
        notes,
        attributes,
        tags: [],
      });
      notice = t('ai.generate.added', { count: r.note_ids.length, skipped: r.skipped });
    });
    if (ok) drafts = [];
  }
</script>

<section>
  {#if !ctx.selectedDeck}
    <p>{t('ai.generate.pick_deck')}</p>
  {:else}
    <div class="form">
      <label>
        {t('ai.generate.notetype')}
        <select bind:value={notetypeId}>
          {#each notetypes as nt (nt.id)}
            <option value={nt.id}>{nt.name}</option>
          {/each}
        </select>
      </label>
      {#each defs as a (a.def.id)}
        <AttributeField def={a.def} bind:value={attrs[a.def.id]!} />
      {/each}
      <label>
        {t('ai.generate.source')}
        <textarea rows="8" bind:value={source} placeholder={t('ai.generate.source_hint')}></textarea>
      </label>
      <label>
        {t('ai.generate.count')}
        <input type="number" min="1" max="50" bind:value={count} />
      </label>
      <label>
        {t('ai.generate.instructions')}
        <input type="text" bind:value={instructions} placeholder={t('ai.generate.instructions_hint')} />
      </label>
      <button disabled={busy || !source.trim()} onclick={() => void runPreview()}>
        {t('ai.generate.preview')}
      </button>
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if notice}
    <p role="status">{notice}</p>
  {/if}

  {#if preview}
    <PreviewPanel {preview} {busy} onsend={() => void send()} oncancel={() => (preview = null)} />
  {/if}
  {#if job}
    <JobProgress {job} />
  {/if}

  {#if drafts.length > 0}
    <h3>{t('ai.generate.drafts', { count: drafts.length })}</h3>
    {#if spent}
      <p class="muted">{t('ai.generate.spent', { cost: spent })}</p>
    {/if}
    <ol class="drafts">
      {#each drafts as d, i (i)}
        <li>
          <label class="keep">
            <input type="checkbox" bind:checked={d.keep} />
            {t('ai.generate.keep')}
          </label>
          {#each Object.keys(d.draft.fields) as key (key)}
            <label>
              {fieldName(key)}
              <textarea rows="2" bind:value={d.draft.fields[key]}></textarea>
            </label>
          {/each}
        </li>
      {/each}
    </ol>
    <button class="primary" disabled={kept === 0} onclick={() => void add()}>
      {t('ai.generate.add', { count: kept })}
    </button>
  {/if}
</section>

<style>
  .form,
  .drafts li {
    display: grid;
    gap: var(--fc-space-2);
    max-width: 48rem;
  }
  label {
    display: grid;
    gap: var(--fc-space-1);
  }
  .keep {
    display: flex;
    align-items: center;
  }
  .drafts li {
    border-bottom: var(--fc-border-width) solid var(--fc-border);
    padding: var(--fc-space-2) 0;
  }
  .error {
    color: var(--fc-danger);
  }
  .muted {
    color: var(--fc-text-muted);
  }
</style>
