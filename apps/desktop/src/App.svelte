<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { DeckTreeNode, ModuleDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { SCREENS, type ScreenCtx, type ScreenDef } from '$lib/features';
  import { deckPaths } from '$lib/components/deck-paths';
  import ShortcutHost from '$lib/features/browse/ShortcutHost.svelte';
  import PluginPanels from '$lib/features/plugins/PluginPanels.svelte';

  interface MenuItem {
    id: string;
    label_key: string;
    screen: ScreenDef;
    order: number;
    route: string | null;
  }

  const routeMatches = (pattern: string, route: string) =>
    pattern.endsWith('/*') ? route.startsWith(pattern.slice(0, -1)) : pattern === route;

  /** Screen for a `main_menu` route, from the registered features. */
  const screenOfRoute = (route: string | null): ScreenDef | null =>
    SCREENS.find((s) => route !== null && s.routes?.some((p) => routeMatches(p, route))) ?? null;

  let modules = $state<ModuleDto[]>([]);
  let decks = $state<DeckTreeNode[]>([]);
  let screen = $state.raw<ScreenDef | null>(null);
  /** Menu entry shown (several entries may share a screen: plugin views). */
  let active = $state<string | null>(null);
  let selectedDeck = $state<string | null>(null);
  let error = $state<string | null>(null);

  const menu = $derived.by((): MenuItem[] => {
    const ui = modules.filter((m) => m.enabled).flatMap((m) => m.ui);
    const items = ui.flatMap((c): MenuItem[] => {
      const s = c.slot === 'main_menu' ? screenOfRoute(c.route) : null;
      return s ? [{ id: c.id, label_key: c.label_key, screen: s, order: c.order, route: c.route }] : [];
    });
    for (const s of SCREENS) {
      if (s.slot && ui.some((c) => c.slot === s.slot?.name)) {
        items.push({ id: s.id, label_key: s.slot.label_key, screen: s, order: s.slot.order, route: null });
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
    route: menu.find((m) => m.id === active)?.route ?? null,
  });

  function show(item: MenuItem) {
    screen = item.screen;
    active = item.id;
  }

  async function loadModules() {
    try {
      modules = (await api.modules.list()).modules;
      const first = menu[0];
      if (!screen && first) show(first);
    } catch (e) {
      fail(e);
    }
  }

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
    void loadModules();
    void loadDecks();
    // Plugins installed/removed/switched change the menu (v0.7).
    const unsubModules = api.on(['plugins_changed', 'module_toggled'], () => void loadModules());
    // Live updates: any deck/study event (or undo/redo, import, restore) re-reads the tree.
    const unsub = api.on(
      ['deck_', 'study_', 'undone', 'redone', 'imported', 'collection_'],
      () => void loadDecks(),
    );
    return () => {
      void unsub.then((u) => u());
      void unsubModules.then((u) => u());
    };
  });
</script>

<div class="shell">
  <nav>
    <div class="brand">{t('app.title')}</div>
    <ul>
      {#each menu as item (item.id)}
        <li>
          <button class:active={active === item.id} aria-current={active === item.id ? 'page' : undefined} onclick={() => show(item)}>
            {td(item.label_key)}
          </button>
        </li>
      {/each}
    </ul>
    <PluginPanels {modules} />
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
      {#key active}<Screen {ctx} />{/key}
    {:else}
      <p class="muted">{t('common.loading')}</p>
    {/if}
  </main>
</div>

<!-- Keyboard shortcuts + command palette (v0.4); inert when the palette module is off. -->
<ShortcutHost
  screens={menu.map((m) => ({ id: m.screen.id, label: td(m.label_key), open: () => show(m) }))}
  activeScreen={screen?.id ?? null}
  {guarded}
/>

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
