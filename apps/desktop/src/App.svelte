<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { DeckTreeNode, ModuleDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { SCREENS, resolveScreen, type ScreenCtx, type ScreenDef } from '$lib/features';
  import { deckPaths } from '$lib/components/deck-paths';
  import ShortcutHost from '$lib/features/browse/ShortcutHost.svelte';
  import PluginPanels from '$lib/features/plugins/PluginPanels.svelte';
  import OutsideHost from '$lib/features/inbox/OutsideHost.svelte';
  import LockHost from '$lib/features/profiles/LockHost.svelte';
  import { buildNavigation, samePlace, type NavEntry, type SectionId } from '$lib/shell/navigation';
  import AppSidebar from '$lib/shell/AppSidebar.svelte';
  import DeckPicker from '$lib/shell/DeckPicker.svelte';
  import ProfileMenu from '$lib/shell/ProfileMenu.svelte';
  import SettingsLayout from '$lib/shell/SettingsLayout.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import ToastHost from '$lib/ui/ToastHost.svelte';

  /** The shell's own first page (not a module; docs/11 §2). Its own chunk, like every screen. */
  const HOME: ScreenDef = { id: 'home', load: () => import('$lib/features/home/HomeScreen.svelte') };

  let modules = $state<ModuleDto[]>([]);
  let decks = $state<DeckTreeNode[]>([]);
  /** Entry shown (several entries may share a screen: plugin views, extra places). */
  let active = $state<string | null>('home');
  let selectedDeck = $state<string | null>(null);
  let error = $state<string | null>(null);
  /** App lock engaged (LockHost): no screen is mounted behind the lock screen. */
  let locked = $state(false);
  /** A plugin the app switched off (repeated failures), or plugins whose hooks were skipped. */
  let pluginNotice = $state<string | null>(null);
  let inboxCount = $state(0);
  /** Last page opened in each section / in Settings (the sidebar returns to it). */
  const lastIn = new Map<string, string>();

  /** A plugin's display name (its locale's `name`), else its id. */
  function pluginName(id: string): string {
    const label = td(`plugin.${id}.name`);
    return label === `plugin.${id}.name` ? id : label;
  }

  const nav = $derived(buildNavigation(modules, SCREENS, HOME));
  /** Every reachable entry, extra places included. */
  const entries = $derived([
    ...nav.sections.flatMap((s) => s.entries),
    ...nav.settings.flatMap((g) => g.entries),
    ...(nav.inbox ? [nav.inbox] : []),
  ]);
  const current = $derived(entries.find((e) => e.id === active) ?? null);
  const screen = $derived(current?.screen ?? null);
  const where = $derived.by((): SectionId | 'inbox' | 'settings' | null => {
    const p = current?.place;
    if (!p) return null;
    return p.kind === 'section' ? p.id : p.kind;
  });
  const sectionNow = $derived(current?.place.kind === 'section' ? (nav.sections.find((s) => samePlace({ kind: 'section', id: s.id }, current.place)) ?? null) : null);

  const paths = $derived(deckPaths(decks));
  const selectedPath = $derived(paths.find((p) => p.id === selectedDeck)?.path ?? null);
  const ctx = $derived<ScreenCtx>({
    decks,
    selectedDeck,
    selectedPath,
    selectDeck: (id) => (selectedDeck = id),
    guarded: (fn) => guarded(fn),
    route: current?.route ?? null,
    navigate: (route) => openRoute(route),
  });

  const placeKey = (e: NavEntry) => (e.place.kind === 'section' ? `section.${e.place.id}` : e.place.kind);

  function show(entry: NavEntry) {
    active = entry.id;
    lastIn.set(placeKey(entry), entry.id);
  }

  function reopen(key: string, list: NavEntry[]) {
    const last = lastIn.get(key);
    const e = list.find((x) => x.id === last) ?? list[0];
    if (e) show(e);
  }

  const openSection = (id: SectionId) => reopen(`section.${id}`, nav.sections.find((s) => s.id === id)?.entries ?? []);
  const openSettings = () => reopen('settings', nav.settings.flatMap((g) => g.entries));
  const openInbox = () => nav.inbox && show(nav.inbox);

  /** Opens the entry with `route` (flashcard:// links, v0.9; `ctx.navigate`). */
  function openRoute(route: string) {
    const item = nav.all.find((m) => m.route === route);
    if (item) show(item);
  }

  async function loadModules() {
    try {
      modules = (await api.modules.list()).modules;
      // A switched-off module's page is gone: back home.
      if (!current) show(nav.all[0]!);
      void loadInbox();
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

  async function loadInbox() {
    if (!modules.some((m) => m.id === 'inbox' && m.enabled)) {
      inboxCount = 0;
      return;
    }
    try {
      inboxCount = (await api.call('inbox.count', {})).pending;
    } catch {
      inboxCount = 0;
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
    const unsubNotice = api.on('plugins_changed', (env) => {
      if (env.event.type !== 'plugins_changed' || !env.event.data.auto_disabled) return;
      const { plugin_id, reason } = env.event.data;
      const why = reason ? td(reason.key, reason.params) : '';
      pluginNotice = [t('plugins.notice.auto_disabled', { name: pluginName(plugin_id) }), why]
        .filter(Boolean)
        .join(' ');
    });
    // One notice per action whose plugins' hook budgets ran out (nobody is switched off).
    const unsubSkipped = api.on('plugin_hooks_skipped', (env) => {
      if (env.event.type !== 'plugin_hooks_skipped') return;
      const { plugin_ids, notice } = env.event.data;
      pluginNotice = td(notice.key, { ...notice.params, names: plugin_ids.map(pluginName).join(', ') });
    });
    const unsubInbox = api.on('inbox_changed', (env) => {
      if (env.event.type === 'inbox_changed') inboxCount = env.event.data.pending;
    });
    // Live updates: any deck/study event (or undo/redo, import, restore) re-reads the tree.
    const unsub = api.on(
      ['deck_', 'study_', 'undone', 'redone', 'data_changed', 'imported', 'collection_'],
      () => void loadDecks(),
    );
    return () => {
      void unsub.then((u) => u());
      void unsubModules.then((u) => u());
      void unsubNotice.then((u) => u());
      void unsubSkipped.then((u) => u());
      void unsubInbox.then((u) => u());
    };
  });
</script>

<div class="shell">
  <AppSidebar {nav} current={where} {inboxCount} {openSection} {openInbox} {openSettings}>
    {#snippet profile()}
      <ProfileMenu
        openProfiles={nav.all.some((e) => e.route === '/profiles') ? () => openRoute('/profiles') : null}
        {guarded}
      />
    {/snippet}
    {#snippet panels()}<PluginPanels {modules} />{/snippet}
  </AppSidebar>

  <main>
    <div class="page">
      {#if error}<p class="banner error" role="alert">{error}</p>{/if}
      {#if pluginNotice}
        <p class="banner notice" role="status">
          <span>{pluginNotice}</span>
          <Button size="sm" variant="ghost" onclick={() => (pluginNotice = null)}>{t('plugins.notice.dismiss')}</Button>
        </p>
      {/if}

      {#snippet body()}
        {#if locked}
          <!-- Behind the lock screen nothing mounts or loads (lazy chunks included). -->
        {:else if screen}
          {#if screen.deckPicker}
            <DeckPicker {paths} value={selectedDeck} onchange={(id) => (selectedDeck = id)} />
          {/if}
          <!-- Lazy screens (v1.0 code splitting) load on first open; bundled ones render at once. -->
          {#key active}
            {#await resolveScreen(screen)}
              <p class="loading" role="status"><Spinner /> {t('common.loading')}</p>
            {:then Screen}
              <Screen {ctx} />
            {:catch}
              <p class="banner error" role="alert">{t('common.load_failed')}</p>
            {/await}
          {/key}
        {:else}
          <p class="loading" role="status"><Spinner /> {t('common.loading')}</p>
        {/if}
      {/snippet}

      {#if where === 'settings' && !locked}
        <SettingsLayout groups={nav.settings} {active} open={show}>{@render body()}</SettingsLayout>
      {:else}
        {#if sectionNow && sectionNow.entries.length > 1 && !locked}
          <div class="section-tabs">
            <Tabs
              label={t('shell.section_nav', { section: t(sectionNow.label_key) })}
              tabs={sectionNow.entries.map((e) => ({ id: e.id, label: td(e.label_key) }))}
              value={active ?? ''}
              onchange={(id) => {
                const e = sectionNow.entries.find((x) => x.id === id);
                if (e) show(e);
              }}
            />
          </div>
        {/if}
        {@render body()}
      {/if}
    </div>
  </main>
</div>

<ToastHost />

<!-- First-run guide on a new, empty collection (v1.0). -->
<!-- Loaded lazily (v1.1 bundle budget): they only matter once their check says so. -->
{#await import('$lib/features/onboarding/OnboardingHost.svelte') then { default: OnboardingHost }}
  <OnboardingHost ondone={() => void loadDecks()} />
{/await}

<!-- App lock screen (v1.0); drawn above everything while locked. -->
<LockHost
  onlockchange={(l) => (locked = l)}
  onunlock={() => {
    // Calls made while locked failed with "the app is locked": start over.
    error = null;
    void loadModules();
    void loadDecks();
  }}
/>

<!-- Pairing requests, clipboard suggestions, flashcard:// links (v0.9). -->
<OutsideHost open={openRoute} selectDeck={(id) => (selectedDeck = id)} />

<!-- Recovery after a crash, update notice (v1.0). -->
{#await import('$lib/features/maintenance/RecoveryHost.svelte') then { default: RecoveryHost }}
  <RecoveryHost open={openRoute} />
{/await}

<!-- Keyboard shortcuts + command palette (v0.4); inert when the palette module is off. -->
<ShortcutHost
  screens={nav.all.map((m) => ({ id: m.screen.id, label: td(m.label_key), open: () => show(m) }))}
  activeScreen={screen?.id ?? null}
  {guarded}
/>

<style>
  .shell {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    height: 100%;
  }
  main {
    overflow: auto;
    background: var(--surface-0);
  }
  .page {
    max-width: 1200px;
    margin: 0 auto;
    padding: var(--space-8) var(--space-8) var(--space-12);
  }
  .section-tabs {
    margin-bottom: var(--space-6);
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin: 0 0 var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
  }
  .banner.error {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .banner.notice {
    background: var(--warning-soft);
    color: var(--text-1);
  }
  .loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-2);
  }
  @media (max-width: 1023.98px) {
    .page {
      padding: var(--space-6) var(--space-4) var(--space-8);
    }
  }
</style>
