<script lang="ts">
  /**
   * Inline card creation (docs/02 §14): "question :: answer" lines become cards. The backend
   * parses and previews (`modes.inline.preview`, duplicates marked) and creates them as one
   * undo step (`modes.inline.create`). Used during study; any screen (reading) can mount it.
   */
  import { api } from '$lib/api';
  import type { InlineCardDto } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';

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

<div class="composer" role="dialog" aria-label={t('modes.inline.heading')}>
  <h3>{t('modes.inline.heading')}</h3>
  <p class="muted">{t('modes.inline.hint')}</p>
  <textarea
    bind:this={area}
    bind:value={text}
    rows="4"
    placeholder={t('modes.inline.placeholder')}
    aria-label={t('modes.inline.heading')}
    onkeydown={onKey}
  ></textarea>
  {#if cards.length > 0}
    <ul data-testid="inline-preview">
      {#each cards as c (c.line)}
        <li class:duplicate={c.duplicate}>
          <span class="muted">{t('modes.inline.line', { n: c.line })}</span>
          <strong>{c.front}</strong> → {c.back}
          {#if c.duplicate}<em class="muted">({t('modes.inline.duplicate')})</em>{/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if truncated}<p class="muted">{t('modes.inline.truncated')}</p>{/if}
  {#if notice}<p class="muted" role="status">{notice}</p>{/if}
  <div class="row">
    <button class="primary" disabled={fresh === 0} onclick={() => void create()}>
      {t('modes.inline.create', { count: fresh })} <kbd>{'Ctrl+Enter'}</kbd>
    </button>
    <button onclick={onclose}>{t('modes.inline.close')} <kbd>{'Esc'}</kbd></button>
  </div>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
    margin-top: var(--fc-space-3);
  }
  h3,
  p {
    margin: 0;
  }
  ul {
    margin: 0;
    padding-left: var(--fc-space-4);
  }
  .duplicate {
    opacity: 0.6;
  }
  .row {
    display: flex;
    gap: var(--fc-space-2);
  }
  kbd {
    color: var(--fc-text-muted);
    font-size: 0.8em;
    margin-left: var(--fc-space-1);
  }
</style>
