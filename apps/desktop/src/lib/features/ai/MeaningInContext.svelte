<!--
  "Meaning in this sentence" (v1.4): which meaning of the word fits the note's sentence. Shows
  what is stored (`ai.meaning.get`: applied, or a suggestion made without a preview), asks the
  AI ("Suggest": `ai.preview` → `ai.run`, or `ai.meaning.suggest` when the user opted in to
  asking without a preview, `ai.meaning.auto_suggest`) and applies the answer the user accepts,
  maybe edited (`ai.meaning.apply`: one undo step, Ctrl+Z). Nothing is written before "Apply";
  the card's meaning field changes only when the box is ticked. Every rule lives in fc-app
  (`modules/ai/meaning.rs`); this only shows. One component for the note editor and the study
  menu (`bare` inside a dialog). The caller shows it only when `aiReady()` (ready.svelte.ts).
-->
<script lang="ts">
  import { api, ApiError } from '$lib/api';
  import type { AiNoteMeaning, AiNoteMeaningState, AiPreview, AiTask } from '$lib/api/types';
  import { formatPercent } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Badge, Button, Card, Checkbox, Input, Skeleton, Sparkles, toast } from '$lib/ui';
  import { followJob, isFinal } from '../io/job';
  import { aiResult, jobErrorText, previewTask, sendPreviewed } from './ai';
  import PreviewPanel from './PreviewPanel.svelte';

  interface Props {
    noteId: string;
    /** Inside a titled dialog: no heading of its own. */
    bare?: boolean;
    /** After an apply (the note's meaning field may have changed in the core). */
    onapplied?: () => void;
  }
  let { noteId, bare = false, onapplied }: Props = $props();

  /** What the AI proposed, from a job's output or a stored suggestion. */
  interface Suggestion {
    meaning: string;
    phrase: string;
    expression: boolean;
    also_correct: string[];
    not_this: string[];
    confidence: number | null;
    model: string;
    original: string;
  }

  const AUTO_SUGGEST = 'ai.meaning.auto_suggest';

  let stored = $state.raw<AiNoteMeaningState | null>(null);
  let loaded = $state(false);
  let suggestion = $state.raw<Suggestion | null>(null);
  /** The suggestion's wording as the user may edit it before applying. */
  let wording = $state('');
  let replaceField = $state(false);
  /** A stored suggestion the user dismissed (shown again only after a new one). */
  let dismissedAt = $state<number | null>(null);
  let preview = $state.raw<AiPreview | null>(null);
  let task = $state.raw<AiTask | null>(null);
  let asking = $state(false);
  let applying = $state(false);
  let error = $state<string | null>(null);

  const applied = $derived(stored?.meaning?.status === 'applied' ? stored.meaning : null);

  function fromStored(m: AiNoteMeaning): Suggestion {
    return {
      meaning: m.meaning,
      phrase: m.phrase,
      expression: m.expression,
      also_correct: m.also_correct,
      not_this: m.not_this,
      confidence: m.confidence,
      model: m.model,
      original: m.original,
    };
  }

  function show(s: Suggestion | null) {
    suggestion = s;
    wording = s?.meaning ?? '';
    replaceField = false;
  }

  /** An error in words; a request the core cannot build says why (no sentence, no meaning field…). */
  function fail(e: unknown) {
    const reason = e instanceof ApiError && e.code === 'VALIDATION_AI_TASK' ? e.details.reason : undefined;
    const known = {
      no_sentence: t('ai.meaning.reason.no_sentence'),
      no_word: t('ai.meaning.reason.no_word'),
      no_meaning_field: t('ai.meaning.reason.no_meaning_field'),
      no_cards: t('ai.meaning.reason.no_cards'),
    } as Record<string, string>;
    if (typeof reason === 'string' && known[reason]) {
      error = known[reason];
    } else {
      error = td((e as ApiError)?.messageKey ?? 'errors.internal.unexpected');
    }
  }

  async function load() {
    const id = noteId;
    try {
      const s = await api.call('ai.meaning.get', { note_id: id });
      if (id !== noteId) return;
      stored = s;
      // A stored suggestion (asked without a preview) shows until applied or dismissed.
      const m = s.meaning;
      if (m && m.status === 'suggested' && !suggestion && !asking && dismissedAt !== m.updated_at) show(fromStored(m));
    } catch (e) {
      if (id === noteId) fail(e);
    } finally {
      if (id === noteId) loaded = true;
    }
  }

  // A new note starts clean.
  $effect(() => {
    void noteId;
    stored = null;
    loaded = false;
    show(null);
    preview = null;
    task = null;
    error = null;
    dismissedAt = null;
    void load();
  });
  $effect(() => {
    let off: (() => void) | null = null;
    let gone = false;
    void api.on(['undone', 'redone'], () => void load()).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  });

  async function autoSuggestOn(): Promise<boolean> {
    try {
      const r = await api.call('settings.get', { scope: { kind: 'app' }, keys: [AUTO_SUGGEST] });
      return r.values[0]?.value === true;
    } catch {
      return false;
    }
  }

  async function ask() {
    error = null;
    show(null);
    preview = null;
    const id = noteId;
    if (await autoSuggestOn()) {
      // Opted in: no preview; the job stores its answer as a suggestion.
      asking = true;
      try {
        const started = await api.call('ai.meaning.suggest', { note_id: id });
        const job = isFinal(started) ? started : await followJob(started, () => {});
        if (id !== noteId) return;
        if (job.status !== 'succeeded') {
          error = jobErrorText(job, t('ai.meaning.failed'));
          return;
        }
        asking = false;
        dismissedAt = null;
        await load();
      } catch (e) {
        fail(e);
      } finally {
        asking = false;
      }
      return;
    }
    const next: AiTask = {
      feature: 'meaning_in_context',
      note_id: id,
      sentence: null,
      lemma: null,
      surface: null,
      position: null,
      target_language: null,
      candidates: [],
    };
    try {
      const p = await previewTask(next);
      if (id !== noteId) return;
      preview = p;
      task = next;
    } catch (e) {
      fail(e);
    }
  }

  async function send() {
    const shown = preview;
    const sent = task;
    if (!shown || !sent) return;
    asking = true;
    preview = null;
    try {
      const done = await sendPreviewed(sent, shown);
      if (task !== sent) return;
      const r = aiResult(done);
      const out = r?.output;
      if (!r || out?.feature !== 'meaning_in_context') {
        error = jobErrorText(done, t('ai.meaning.failed'));
        return;
      }
      show({
        meaning: out.meaning,
        phrase: out.phrase,
        expression: out.expression,
        also_correct: out.also_correct,
        not_this: out.not_this,
        confidence: out.confidence,
        model: r.model,
        original: out.original,
      });
    } catch (e) {
      fail(e);
    } finally {
      asking = false;
    }
  }

  function dismiss() {
    if (stored?.meaning?.status === 'suggested') dismissedAt = stored.meaning.updated_at;
    show(null);
  }

  async function apply() {
    const s = suggestion;
    if (!s || !wording.trim()) return;
    applying = true;
    error = null;
    const edited = wording.trim() !== s.meaning.trim();
    try {
      stored = await api.call('ai.meaning.apply', {
        note_id: noteId,
        meaning: wording.trim(),
        phrase: s.phrase,
        expression: s.expression,
        also_correct: s.also_correct,
        not_this: s.not_this,
        confidence: edited ? null : s.confidence,
        model: s.model || null,
        edited,
        accepted: s.also_correct,
        replace_field: replaceField,
      });
      show(null);
      toast.show(t('ai.meaning.applied'), {
        tone: 'success',
        action: { label: t('ai.meaning.undo'), run: () => void api.call('oplog.undo', {}).catch(fail) },
      });
      onapplied?.();
    } catch (e) {
      fail(e);
    } finally {
      applying = false;
    }
  }

  const otherAnswers = $derived(
    (stored?.accepted ?? []).flatMap((a) => a.answers).filter((a) => a.trim() && a !== applied?.meaning),
  );
</script>

<section class="ai-meaning" class:bare aria-labelledby={bare ? undefined : 'ai-meaning-title'} data-testid="ai-meaning">
  {#if !bare}<h4 id="ai-meaning-title">{t('ai.meaning.title')}</h4>{/if}
  <p class="lead">{t('ai.meaning.lead')}</p>

  {#if !loaded}
    <Skeleton lines={1} />
  {:else if applied}
    <div class="current" data-testid="ai-meaning-current">
      <p class="line">
        <span class="k">{t('ai.meaning.current')}</span>
        <strong>{applied.meaning}</strong>
        {#if applied.expression}<Badge tone="accent" variant="soft">{t('ai.meaning.expression')}</Badge>{/if}
      </p>
      {#if applied.phrase}<p class="sub">{t('ai.meaning.phrase', { phrase: applied.phrase })}</p>{/if}
      {#if otherAnswers.length > 0}<p class="sub">{t('ai.meaning.also_accepted', { list: otherAnswers.join(', ') })}</p>{/if}
      {#if applied.original && applied.original !== applied.meaning}
        <p class="sub">{t('ai.meaning.original', { original: applied.original })}</p>
      {/if}
    </div>
  {/if}

  {#if !suggestion && !preview}
    <div class="row">
      <Button size="sm" icon={Sparkles} loading={asking} disabled={asking || !loaded} onclick={() => void ask()}>
        {applied ? t('ai.meaning.ask_again') : t('ai.meaning.ask')}
      </Button>
      {#if asking}<span class="muted" role="status">{t('ai.job.waiting')}</span>{/if}
    </div>
  {/if}

  {#if preview}
    <PreviewPanel {preview} busy={asking} onsend={() => void send()} oncancel={() => (preview = null)} />
  {/if}

  {#if suggestion}
    <Card padding="md" as="section" label={t('ai.meaning.suggestion')} class="ai-meaning-suggestion">
      <div class="suggestion" data-testid="ai-meaning-suggestion">
        <Input size="sm" label={t('ai.meaning.in_sentence')} bind:value={wording} disabled={applying} />
        <p class="line">
          {#if suggestion.phrase}<span>{t('ai.meaning.phrase', { phrase: suggestion.phrase })}</span>{/if}
          {#if suggestion.expression}<Badge tone="accent" variant="soft">{t('ai.meaning.expression')}</Badge>{/if}
          {#if suggestion.confidence !== null}
            <span class="sub">{t('ai.meaning.confidence', { value: formatPercent(suggestion.confidence) })}</span>
          {/if}
        </p>
        {#if suggestion.also_correct.length > 0}
          <p class="sub">{t('ai.meaning.also_correct', { list: suggestion.also_correct.join(', ') })}</p>
        {/if}
        {#if suggestion.not_this.length > 0}
          <p class="sub">{t('ai.meaning.not_this', { list: suggestion.not_this.join(', ') })}</p>
        {/if}
        {#if suggestion.original}
          <p class="sub">{t('ai.meaning.source', { original: suggestion.original })}</p>
        {/if}
        <Checkbox bind:checked={replaceField} label={t('ai.meaning.replace_field')} disabled={applying} />
        <div class="row">
          <Button variant="primary" size="sm" loading={applying} disabled={applying || !wording.trim()} onclick={() => void apply()}>
            {t('ai.meaning.apply')}
          </Button>
          <Button variant="ghost" size="sm" disabled={applying} onclick={dismiss}>{t('ai.meaning.dismiss')}</Button>
        </div>
      </div>
    </Card>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .ai-meaning {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h4 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .lead,
  .sub,
  .muted {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .current,
  .suggestion {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .suggestion {
    gap: var(--space-2);
  }
  .line {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .line .k {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-xs);
  }
</style>
