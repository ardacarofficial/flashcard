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
  // Eager (App mounts it): lib/ui file by file, never the barrel.
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Dialog from '$lib/ui/Dialog.svelte';
  import { CLIPBOARD_EVENT, DEEP_LINK_EVENT, defaultShell, linkTarget, type Shell } from './shell';

  interface Props {
    /** Opens the menu entry with this route. */
    open: (route: string) => void;
    selectDeck: (id: DeckId | null) => void;
    shell?: Shell;
  }
  let { open, selectDeck, shell = defaultShell() }: Props = $props();

  let pairing = $state<ApiClientDto | null>(null);
  /** The permissions ticked in the approval dialog (all of them to begin with). */
  let granted = $state<Record<string, boolean>>({});
  const grantedScopes = $derived(pairing ? pairing.requested_scopes.filter((s) => granted[s]) : []);
  let suggestion = $state<string | null>(null);
  let notice = $state<string | null>(null);

  async function decide(approve: boolean) {
    const c = pairing;
    const scopes = grantedScopes;
    pairing = null;
    if (!c) return;
    try {
      await api.call(approve ? 'local_api.clients.approve' : 'local_api.clients.deny', approve ? { id: c.id, scopes } : { id: c.id });
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
    let links: Awaited<ReturnType<typeof shell.takeDeepLinks>>;
    try {
      links = await shell.takeDeepLinks();
    } catch {
      // Locked: the shell holds the links and announces them again after unlock.
      return;
    }
    for (const r of links) {
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
      if (env.event.type === 'local_api_pairing') {
        const c = env.event.data.client;
        granted = Object.fromEntries(c.requested_scopes.map((s) => [s, true]));
        pairing = c;
      }
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
  <Dialog
    open={true}
    role="alertdialog"
    dismissible={false}
    title={t('local_api.pairing.title')}
    description={t('local_api.pairing.body', { name: pairing.name })}
    size="sm"
  >
    {#if pairing.origin}<p class="muted">{t('local_api.origin', { origin: pairing.origin })}</p>{/if}
    <fieldset>
      <legend>{t('local_api.pairing.asks')}</legend>
      {#each pairing.requested_scopes as s (s)}
        <Checkbox label={td(`local_api.scope.${s}`)} bind:checked={granted[s]} />
      {/each}
    </fieldset>
    <p class="muted">{t('local_api.pairing.hint')}</p>
    {#snippet footer()}
      <Button variant="ghost" onclick={() => decide(false)}>{t('local_api.deny')}</Button>
      <Button variant="primary" onclick={() => decide(true)} disabled={grantedScopes.length === 0}>{t('local_api.approve')}</Button>
    {/snippet}
  </Dialog>
{/if}

<div class="toasts">
  {#if suggestion}
    <div class="toast" role="status">
      <span class="msg">{t('clipboard.suggest', { text: suggestion })}</span>
      <Button size="sm" variant="primary" onclick={addSuggestion}>{t('clipboard.add')}</Button>
      <Button size="sm" variant="ghost" onclick={() => (suggestion = null)}>{t('clipboard.dismiss')}</Button>
    </div>
  {/if}
  {#if notice}
    <div class="toast" role="status">
      <span class="msg">{notice}</span>
      <Button size="sm" variant="ghost" onclick={() => (notice = null)}>{t('clipboard.dismiss')}</Button>
    </div>
  {/if}
</div>

<style>
  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: var(--space-3) 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: var(--space-2);
    padding: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .toasts {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: min(420px, calc(100vw - 2 * var(--space-4)));
  }
  .toast {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    box-shadow: var(--shadow-2);
    color: var(--text-1);
    font-size: var(--text-sm);
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
