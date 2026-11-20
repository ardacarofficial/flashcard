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
  // Eager (main.ts mounts it): lib/ui file by file, never the barrel.
  import Button from '$lib/ui/Button.svelte';
  import Select from '$lib/ui/Select.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';
  import { QUICK_ADD_EVENT, defaultShell, type Shell } from './shell';

  let { shell = defaultShell() }: { shell?: Shell } = $props();

  let decks = $state<DeckTreeNode[]>([]);
  let deck = $state<DeckId | null>(null);
  let front = $state('');
  let back = $state('');
  let status = $state<string | null>(null);

  const paths = $derived(deckPaths(decks));
  const deckOptions = $derived(paths.map((p) => ({ value: p.id as DeckId | null, label: p.path })));

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
    <Select label={t('inbox.deck')} options={deckOptions} bind:value={deck} />
  {/if}
  <!-- svelte-ignore a11y_autofocus -->
  <Textarea label={t('inbox.front')} bind:value={front} rows={2} autoGrow autofocus />
  <Textarea label={t('inbox.back')} bind:value={back} rows={2} autoGrow />
  <div class="actions">
    <Button variant="ghost" onclick={() => void shell.hide()}>{t('quick_add.close')}</Button>
    <Button variant="primary" type="submit" disabled={!deck || !front.trim()}>{t('quick_add.add')}</Button>
  </div>
  {#if status}<p class="status" role="status">{status}</p>{/if}
</form>

<style>
  .quick-add {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-height: 100vh;
    padding: var(--space-6);
    background: var(--surface-1);
    color: var(--text-1);
  }
  h1 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    justify-content: flex-end;
  }
  .muted,
  .status {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
