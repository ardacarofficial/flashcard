<script lang="ts">
  /**
   * Things that arrive from outside the window (v0.9): a program asking to connect to the
   * local API (approval dialog), copied text worth a card (clipboard suggestion), and
   * handled flashcard:// links (navigate, or "proposed to the inbox"). Mounted once by the
   * shell; draws nothing until something arrives.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { ApiClientDto, DeckId } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { CLIPBOARD_EVENT, DEEP_LINK_EVENT, defaultShell, linkTarget, type Shell } from './shell';

  interface Props {
    /** Opens the menu entry with this route. */
    open: (route: string) => void;
    selectDeck: (id: DeckId | null) => void;
    shell?: Shell;
  }
  let { open, selectDeck, shell = defaultShell() }: Props = $props();

  let pairing = $state<ApiClientDto | null>(null);
  let suggestion = $state<string | null>(null);
  let notice = $state<string | null>(null);

  async function decide(approve: boolean) {
    const c = pairing;
    pairing = null;
    if (!c) return;
    try {
      await api.call(approve ? 'local_api.clients.approve' : 'local_api.clients.deny', approve ? { id: c.id, scopes: null } : { id: c.id });
    } catch (e) {
      notice = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }

  async function addSuggestion() {
    const text = suggestion;
    suggestion = null;
    if (!text) return;
    try {
      await api.call('inbox.add', { front: text, fields: {}, tags: [], attributes: {}, via: 'clipboard' });
      notice = t('clipboard.added');
    } catch (e) {
      notice = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }

  async function drainLinks() {
    for (const r of await shell.takeDeepLinks()) {
      if (r.error || !r.ok) {
        notice = r.error ? `${t('deep_links.failed')} ${td(r.error.message_key)}` : t('deep_links.failed');
        continue;
      }
      const target = linkTarget(r.ok);
      if (target.deck) selectDeck(target.deck);
      open(target.route);
      if (target.proposed) notice = t('deep_links.added');
    }
  }

  onMount(() => {
    const unsubPairing = api.on('local_api_pairing', (env) => {
      if (env.event.type === 'local_api_pairing') pairing = env.event.data.client;
    });
    const unsubClip = shell.listen(CLIPBOARD_EVENT, (p) => {
      const text = (p as { text?: unknown } | null)?.text;
      if (typeof text === 'string' && text) suggestion = text;
    });
    const unsubLinks = shell.listen(DEEP_LINK_EVENT, () => void drainLinks());
    // Links handled before this window listened (cold start).
    void drainLinks();
    return () => {
      void unsubPairing.then((u) => u());
      void unsubClip.then((u) => u());
      void unsubLinks.then((u) => u());
    };
  });
</script>

{#if pairing}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="pairing-title">
      <h2 id="pairing-title">{t('local_api.pairing.title')}</h2>
      <p>{t('local_api.pairing.body', { name: pairing.name })}</p>
      {#if pairing.origin}<p class="muted">{t('local_api.origin', { origin: pairing.origin })}</p>{/if}
      <p>{t('local_api.pairing.asks')}</p>
      <ul>
        {#each pairing.requested_scopes as s (s)}<li>{td(`local_api.scope.${s}`)}</li>{/each}
      </ul>
      <p class="muted">{t('local_api.pairing.hint')}</p>
      <div class="actions">
        <button onclick={() => decide(false)}>{t('local_api.deny')}</button>
        <button onclick={() => decide(true)}>{t('local_api.approve')}</button>
      </div>
    </div>
  </div>
{/if}

{#if suggestion}
  <div class="toast" role="status">
    <span>{t('clipboard.suggest', { text: suggestion })}</span>
    <button onclick={addSuggestion}>{t('clipboard.add')}</button>
    <button onclick={() => (suggestion = null)}>{t('clipboard.dismiss')}</button>
  </div>
{/if}

{#if notice}
  <div class="toast notice" role="status">
    <span>{notice}</span>
    <button onclick={() => (notice = null)}>{t('clipboard.dismiss')}</button>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--fc-text) 40%, transparent);
  }
  .dialog {
    background: var(--fc-bg);
    padding: var(--fc-space-6);
    border-radius: var(--fc-radius, 6px);
    max-width: 28rem;
  }
  .actions {
    display: flex;
    gap: var(--fc-space-2);
    justify-content: flex-end;
  }
  .toast {
    position: fixed;
    right: var(--fc-space-4);
    bottom: var(--fc-space-4);
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    background: var(--fc-bg);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius, 6px);
    padding: var(--fc-space-2) var(--fc-space-3);
  }
  .toast.notice {
    bottom: calc(var(--fc-space-4) * 4);
  }
</style>
