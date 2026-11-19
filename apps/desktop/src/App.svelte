<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { DeckTreeNode, ModuleDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { SCREENS, type ScreenCtx, type ScreenDef } from '$lib/features';
  import { deckPaths } from '$lib/components/deck-paths';

  interface MenuItem {
    id: string;
    label_key: string;
    screen: ScreenDef;
    order: number;
  }

  /** Screen for a `main_menu` route, from the registered features. */
  const screenOfRoute = (route: string | null): ScreenDef | null =>
    SCREENS.find((s) => route !== null && s.routes?.includes(route)) ?? null;

  let modules = $state<ModuleDto[]>([]);
  let decks = $state<DeckTreeNode[]>([]);
  let screen = $state.raw<ScreenDef | null>(null);
  let selectedDeck = $state<string | null>(null);
  let error = $state<string | null>(null);

  const menu = $derived.by((): MenuItem[] => {
    const ui = modules.filter((m) => m.enabled).flatMap((m) => m.ui);
    const items = ui.flatMap((c): MenuItem[] => {
      const s = c.slot === 'main_menu' ? screenOfRoute(c.route) : null;
      return s ? [{ id: c.id, label_key: c.label_key, screen: s, order: c.order }] : [];
    });
    for (const s of SCREENS) {
      if (s.slot && ui.some((c) => c.slot === s.slot?.name)) {
        items.push({ id: s.id, label_key: s.slot.label_key, screen: s, order: s.slot.order });
      }
    }
    return items.sort((a, b) => a.order - b.order);
  });
  const paths = $derived(deckPaths(decks));
  const selectedPath = $derived(paths.find((p) => p.id === selectedDeck)?.path ?? null);
  const ctx = $derived<ScreenCtx>({
    decks,
    selectedDeck,
    selectedPath,
    selectDeck: (id) => (selectedDeck = id),
    guarded: (fn) => guarded(fn),
  });

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
          <button class:active={screen?.id === item.screen.id} aria-current={screen?.id === item.screen.id ? 'page' : undefined} onclick={() => (screen = item.screen)}>
            {td(item.label_key)}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <main>
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    {#if screen}
      {#if screen.deckPicker}
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
      {@const Screen = screen.component}
      <Screen {ctx} />
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
