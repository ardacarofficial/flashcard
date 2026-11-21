<script lang="ts">
  import { api, type ApiError } from '$lib/api';
  import type { CardMetaDto, FlagDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { defaultFlagKey, flagColor, hasFlag } from './flags';
  import { moduleOn } from './modules.svelte';

  interface Props {
    noteId: string;
  }
  let { noteId }: Props = $props();

  let cards = $state<CardMetaDto[]>([]);
  let flags = $state<FlagDto[]>([]);
  let error = $state<string | null>(null);
  let naming = $state(false);

  const flagName = (f: FlagDto) => f.name ?? td(defaultFlagKey(f.flag));

  async function load(id: string) {
    try {
      cards = (await api.call('cards.meta', { note_id: id })).items;
      if (moduleOn('flags')) flags = (await api.call('flags.list', {})).flags;
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  $effect(() => {
    const id = noteId;
    void load(id);
    const unsub = api.on(['cards_updated', 'flags_changed', 'note', 'undone', 'redone', 'data_changed'], () => void load(id));
    return () => void unsub.then((u) => u());
  });

  async function run(fn: () => Promise<unknown>) {
    error = null;
    try {
      await fn();
    } catch (e) {
      error = td((e as ApiError).messageKey);
    }
  }

  const toggle = (card: CardMetaDto, flag: number) =>
    run(() => api.call('cards.set_flag', { ids: [card.id], flag, on: !hasFlag(card.flags, flag) }));

  const saveNote = (card: CardMetaDto, text: string) =>
    run(() => api.call('cards.set_user_note', { id: card.id, text: text.trim() ? text : null }));

  const rename = (f: FlagDto, name: string) =>
    run(() => api.call('flags.update', { flag: f.flag, name: name.trim() || null, color: null }));
</script>

<section class="cards">
  <h3>{t('cards.heading')}</h3>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <ul>
    {#each cards as card (card.id)}
      <li>
        <div class="row">
          <strong>{card.template_name}</strong>
          <span class="muted">{td(`cardtypes.engine.${card.card_type}`)}</span>
          {#if moduleOn('flags')}
            <span class="flags" role="group" aria-label={t('flags.heading')}>
              {#each flags as f (f.flag)}
                <button
                  type="button"
                  class="flag small"
                  class:on={hasFlag(card.flags, f.flag)}
                  style:--flag={flagColor(f)}
                  aria-pressed={hasFlag(card.flags, f.flag)}
                  title={flagName(f)}
                  onclick={() => void toggle(card, f.flag)}
                >
                  <span class="dot"></span><span class="sr-only">{flagName(f)}</span>
                </button>
              {/each}
            </span>
          {/if}
        </div>
        {#if moduleOn('card_notes')}
          <label class="note">
            <span class="muted">{t('cards.user_note')}</span>
            <textarea
              rows="2"
              value={card.user_note ?? ''}
              onchange={(e) => void saveNote(card, e.currentTarget.value)}
              placeholder={t('cards.user_note_placeholder')}
            ></textarea>
          </label>
        {/if}
      </li>
    {/each}
  </ul>
  {#if moduleOn('flags') && flags.length > 0}
    <details bind:open={naming}>
      <summary>{t('flags.rename')}</summary>
      <div class="names">
        {#each flags as f (f.flag)}
          <label>
            <span class="dot" style:--flag={flagColor(f)}></span>
            <input value={f.name ?? ''} placeholder={td(defaultFlagKey(f.flag))} onchange={(e) => void rename(f, e.currentTarget.value)} />
          </label>
        {/each}
      </div>
    </details>
  {/if}
</section>

<style>
  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-xs);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
  }
  li {
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
    align-items: center;
    min-height: 24px;
    font-size: var(--text-sm);
  }
  .row strong {
    font-weight: var(--weight-medium);
  }
  .muted {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .flags {
    display: flex;
    gap: 2px;
    margin-inline-start: auto;
  }
  .flag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    opacity: 0.4;
    cursor: pointer;
  }
  .flag:hover {
    opacity: 0.8;
    background: var(--surface-2);
  }
  .flag.on {
    opacity: 1;
    border-color: var(--flag);
  }
  .flag:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--flag);
    flex: none;
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: var(--space-1);
  }
  textarea,
  .names input {
    box-sizing: border-box;
    width: 100%;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
  }
  textarea {
    padding: var(--space-1) var(--space-2);
    resize: vertical;
  }
  .names input {
    height: var(--control-h-sm);
    padding: 0 var(--space-2);
  }
  textarea:focus-visible,
  .names input:focus-visible {
    outline: none;
    border-color: var(--accent-text);
    box-shadow: 0 0 0 1px var(--accent-text);
  }
  textarea::placeholder,
  .names input::placeholder {
    color: var(--text-3);
  }
  /* A quiet chevron instead of the browser's ▶ marker. */
  details summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    list-style: none;
    cursor: pointer;
    color: var(--text-2);
    font-size: var(--text-xs);
    padding: 2px 0;
  }
  details summary::-webkit-details-marker {
    display: none;
  }
  details summary::before {
    content: '';
    width: 5px;
    height: 5px;
    border-right: 1.5px solid currentColor;
    border-bottom: 1.5px solid currentColor;
    transform: rotate(-45deg);
    transition: transform var(--motion-fast) var(--ease);
  }
  details[open] summary::before {
    transform: rotate(45deg);
  }
  details summary:hover {
    color: var(--text-1);
  }
  .names {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .names label {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
</style>
