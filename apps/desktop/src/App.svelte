<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { DeckTreeNode, ModuleDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import DeckTree from '$lib/components/DeckTree.svelte';
  import NoteAddForm from '$lib/components/NoteAddForm.svelte';
  import NoteList from '$lib/components/NoteList.svelte';
  import SettingsScreen from '$lib/components/SettingsScreen.svelte';
  import { deckPaths } from '$lib/components/deck-paths';

  /**
   * Screens this frontend can draw, by the route a module's `main_menu` contribution points
   * at. Menu items for other routes are ignored. Settings has no module menu entry: it is
   * shown when any enabled module contributes a settings page.
   */
  const ROUTES = {
    '/decks': 'decks',
    '/notes/new': 'add_note',
    '/notes': 'notes',
  } as const;
  type Screen = (typeof ROUTES)[keyof typeof ROUTES] | 'settings';
  interface MenuItem {
    id: string;
    label_key: string;
    screen: Screen;
    order: number;
  }
  const screenOf = (route: string | null): Screen | null =>
    route !== null && route in ROUTES ? ROUTES[route as keyof typeof ROUTES] : null;

  let modules = $state<ModuleDto[]>([]);
  let decks = $state<DeckTreeNode[]>([]);
  let screen = $state<Screen | null>(null);
  let selectedDeck = $state<string | null>(null);
  let error = $state<string | null>(null);

  const menu = $derived.by((): MenuItem[] => {
    const ui = modules.filter((m) => m.enabled).flatMap((m) => m.ui);
    const items = ui.flatMap((c): MenuItem[] => {
      const s = c.slot === 'main_menu' ? screenOf(c.route) : null;
      return s ? [{ id: c.id, label_key: c.label_key, screen: s, order: c.order }] : [];
    });
    if (ui.some((c) => c.slot === 'settings_page')) {
      items.push({ id: 'settings', label_key: 'nav.settings', screen: 'settings', order: Infinity });
    }
    return items.sort((a, b) => a.order - b.order);
  });
  const paths = $derived(deckPaths(decks));
  const selectedPath = $derived(paths.find((p) => p.id === selectedDeck)?.path ?? null);

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey);
  }

  async function loadDecks() {
    try {
      decks = (await api.deck.tree()).roots;
      if (selectedDeck && !deckPaths(decks).some((p) => p.id === selectedDeck)) selectedDeck = null;
    } catch (e) {
      fail(e);
    }
  }

  async function guarded(fn: () => Promise<unknown>): Promise<boolean> {
    error = null;
    try {
      await fn();
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  onMount(() => {
    api.modules.list().then((m) => {
      modules = m.modules;
      screen ??= menu[0]?.screen ?? null;
    }, fail);
    void loadDecks();
    // Live updates: any deck event (or undo/redo) re-reads the tree from the backend.
    const unsub = api.on(['deck_', 'undone', 'redone'], () => void loadDecks());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="shell">
  <nav>
    <div class="brand">{t('app.title')}</div>
    <ul>
      {#each menu as item (item.id)}
        <li>
          <button class:active={screen === item.screen} aria-current={screen === item.screen ? 'page' : undefined} onclick={() => (screen = item.screen)}>
            {td(item.label_key)}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <main>
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    {#if screen === 'add_note' || screen === 'notes'}
      <label class="deck-pick">
        {t('deck.current')}
        <select bind:value={selectedDeck}>
          <option value={null}>{t('deck.select_prompt')}</option>
          {#each paths as p (p.id)}
            <option value={p.id}>{p.path}</option>
          {/each}
        </select>
      </label>
    {/if}

    {#if screen === 'decks'}
      <DeckTree
        {decks}
        selectedId={selectedDeck}
        onselect={(id) => (selectedDeck = id)}
        oncreate={(name, parentId) => guarded(() => api.deck.create(name, parentId))}
        onrename={(id, name) => guarded(() => api.deck.rename(id, name))}
        ondelete={(id) => guarded(() => api.deck.remove(id))}
      />
    {:else if screen === 'add_note'}
      {#if selectedDeck && selectedPath}
        <NoteAddForm deckId={selectedDeck} deckName={selectedPath} />
      {/if}
    {:else if screen === 'notes'}
      {#if selectedDeck}
        <NoteList deckId={selectedDeck} />
      {/if}
    {:else if screen === 'settings'}
      <SettingsScreen />
    {:else}
      <p class="muted">{t('common.loading')}</p>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100%;
  }
  nav {
    background: var(--fc-bg-subtle);
    border-right: var(--fc-border-width) solid var(--fc-border);
    padding: var(--fc-space-4) var(--fc-space-2);
  }
  .brand {
    font-weight: 600;
    padding: 0 var(--fc-space-2) var(--fc-space-4);
  }
  nav ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  nav button {
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
  }
  nav button.active {
    background: var(--fc-selected);
  }
  main {
    padding: var(--fc-space-6);
    overflow: auto;
  }
  .deck-pick {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
    margin-bottom: var(--fc-space-4);
  }
</style>
