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
                  <span class="dot"></span><span class="sr">{flagName(f)}</span>
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }
  .flags {
    display: flex;
    gap: 2px;
  }
  .flag {
    padding: 2px 4px;
    opacity: 0.45;
  }
  .flag.on {
    opacity: 1;
    border-color: var(--flag);
  }
  .dot {
    display: inline-block;
    width: 0.8em;
    height: 0.8em;
    border-radius: 50%;
    background: var(--flag);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }
  .names {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .names label {
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }
</style>
