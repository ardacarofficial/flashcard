<script lang="ts">
  /**
   * The quick-add window (v0.9, docs/06 §7): opened by the global shortcut while the app is
   * in the background. Adds a Classic note (an ordinary, undoable `notes.create`); Escape
   * hides the window.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckId, DeckTreeNode } from '$lib/api/types';
  import { deckPaths } from '$lib/components/deck-paths';
  import { t, td } from '$lib/i18n/index.svelte';
  import { QUICK_ADD_EVENT, defaultShell, type Shell } from './shell';

  let { shell = defaultShell() }: { shell?: Shell } = $props();

  let decks = $state<DeckTreeNode[]>([]);
  let deck = $state<DeckId | null>(null);
  let front = $state('');
  let back = $state('');
  let status = $state<string | null>(null);

  const paths = $derived(deckPaths(decks));

  async function refresh() {
    try {
      decks = (await api.deck.tree()).roots;
      if (!deck || !deckPaths(decks).some((p) => p.id === deck)) deck = deckPaths(decks)[0]?.id ?? null;
      const text = await shell.prefill();
      if (text && !front) front = text;
    } catch (e) {
      status = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (!deck || !front.trim()) return;
    try {
      await api.note.createClassic(deck, front, back, []);
      front = back = '';
      status = t('quick_add.added');
    } catch (err) {
      status = td((err as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') void shell.hide();
  }

  onMount(() => {
    void refresh();
    const unsub = shell.listen(QUICK_ADD_EVENT, () => {
      status = null;
      void refresh();
    });
    return () => void unsub.then((u) => u());
  });
</script>

<svelte:window {onkeydown} />

<form class="quick-add" onsubmit={add}>
  <h1>{t('quick_add.title')}</h1>
  {#if paths.length === 0}
    <p class="muted">{t('quick_add.no_deck')}</p>
  {:else}
    <label>
      {t('inbox.deck')}
      <select bind:value={deck}>
        {#each paths as p (p.id)}<option value={p.id}>{p.path}</option>{/each}
      </select>
    </label>
  {/if}
  <!-- svelte-ignore a11y_autofocus -->
  <label>{t('inbox.front')} <textarea bind:value={front} rows="2" autofocus></textarea></label>
  <label>{t('inbox.back')} <textarea bind:value={back} rows="2"></textarea></label>
  <div class="actions">
    <button type="button" onclick={() => void shell.hide()}>{t('quick_add.close')}</button>
    <button type="submit" disabled={!deck || !front.trim()}>{t('quick_add.add')}</button>
  </div>
  {#if status}<p role="status">{status}</p>{/if}
</form>

<style>
  .quick-add {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    padding: var(--fc-space-4);
  }
  .quick-add label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  .actions {
    display: flex;
    gap: var(--fc-space-2);
    justify-content: flex-end;
  }
  h1 {
    font-size: 1.1rem;
    margin: 0;
  }
</style>
