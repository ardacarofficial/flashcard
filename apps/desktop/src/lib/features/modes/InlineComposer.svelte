<script lang="ts">
  /**
   * Inline card creation (docs/02 §14): "question :: answer" lines become cards. The backend
   * parses and previews (`modes.inline.preview`, duplicates marked) and creates them as one
   * undo step (`modes.inline.create`). Used during study; any screen (reading) can mount it.
   */
  import { api } from '$lib/api';
  import type { InlineCardDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Textarea } from '$lib/ui';

  let {
    deckId,
    guarded,
    onclose,
  }: {
    deckId: string;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
    onclose: () => void;
  } = $props();

  let text = $state('');
  let cards = $state<InlineCardDto[]>([]);
  let truncated = $state(false);
  let notice = $state<string | null>(null);
  let area = $state<HTMLTextAreaElement | null>(null);

  $effect(() => {
    area?.focus();
  });

  // Live preview, debounced.
  $effect(() => {
    const value = text;
    const deck = deckId;
    if (value.trim() === '') {
      cards = [];
      return;
    }
    const timer = setTimeout(() => {
      void api.call('modes.inline.preview', { text: value, deck_id: deck, notetype_id: null }).then(
        (p) => {
          if (value === text) {
            cards = p.cards;
            truncated = p.truncated;
          }
        },
        () => (cards = []),
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  const fresh = $derived(cards.filter((c) => !c.duplicate).length);

  async function create() {
    if (fresh === 0) return;
    await guarded(async () => {
      const r = await api.call('modes.inline.create', { text, deck_id: deckId, notetype_id: null, tags: [] });
      notice = t('modes.inline.created', { count: r.note_ids.length });
      text = '';
      cards = [];
    });
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void create();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="composer" role="dialog" aria-label={t('modes.inline.heading')} tabindex="-1" onkeydown={onKey}>
  <div class="head">
    <h3>{t('modes.inline.heading')}</h3>
    <span class="hint">{t('modes.inline.hint')}</span>
  </div>
  <Textarea bind:element={area} bind:value={text} rows={3} autoGrow maxRows={10} label={t('modes.inline.heading')} hideLabel placeholder={t('modes.inline.placeholder')} class="area" />
  {#if cards.length > 0}
    <ul class="preview" data-testid="inline-preview">
      {#each cards as c (c.line)}
        <li class:duplicate={c.duplicate}>
          <span class="line num">{c.line}</span>
          <span class="front">{c.front}</span>
          <span class="back">{c.back}</span>
          {#if c.duplicate}<span class="dup">{t('modes.inline.duplicate')}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if truncated}<p class="muted">{t('modes.inline.truncated')}</p>{/if}
  <div class="row">
    {#if notice}<p class="ok" role="status">{notice}</p>{/if}
    <span class="grow"></span>
    <Button variant="ghost" size="sm" kbd="Esc" onclick={onclose}>{t('modes.inline.close')}</Button>
    <Button variant="primary" size="sm" kbd="Mod+Enter" disabled={fresh === 0} onclick={() => void create()}>
      {t('modes.inline.create', { count: fresh })}
    </Button>
  </div>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .composer:focus {
    outline: none;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .hint,
  .muted {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .composer :global(.area textarea) {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  .preview {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
  }
  .preview li {
    display: grid;
    grid-template-columns: 3ch minmax(0, 1fr) minmax(0, 1fr) auto;
    gap: var(--space-3);
    align-items: center;
    min-height: 30px;
    padding: 0 var(--space-3);
  }
  .preview li + li {
    border-top: 1px solid var(--border);
  }
  .line {
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  .front {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }
  .back {
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .duplicate .front,
  .duplicate .back {
    text-decoration: line-through;
  }
  .dup {
    font-size: var(--text-2xs);
    color: var(--warning);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .ok {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--success);
  }
</style>
