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
  import HelpHost from '$lib/features/help/HelpHost.svelte';
  import { buildNavigation, samePlace, type NavEntry, type SectionId } from '$lib/shell/navigation';
  import AppSidebar from '$lib/shell/AppSidebar.svelte';
  import { openHelp, openPalette, openQuickAdd, setQuickAddFallback, shellCommands } from '$lib/shell/commands.svelte';
  import TopBar from '$lib/shell/TopBar.svelte';
  import { typingTarget } from '$lib/features/browse/palette';
  import DeckPicker from '$lib/shell/DeckPicker.svelte';
  import ProfileMenu from '$lib/shell/ProfileMenu.svelte';
  import SettingsLayout from '$lib/shell/SettingsLayout.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Spinner from '$lib/ui/Spinner.svelte';
  import Tabs from '$lib/ui/Tabs.svelte';
  import ToastHost from '$lib/ui/ToastHost.svelte';
  import { setPageChrome } from '$lib/ui/page-chrome';
  import type { Crumb } from '$lib/ui/types';

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
  /** A study session is running on the current screen (`ctx.setFocus`): focus layout. */
  let focusOn = $state(false);
  /** Crumbs the current screen appended to the top bar's trail (`PageChrome.setTrail`). */
  let screenTrail = $state<Crumb[] | null>(null);
  /** Back from the screen's sub-page: makes the page crumb a link. */
  let screenUp = $state<(() => void) | null>(null);
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
    setFocus: (on) => (focusOn = on),
  });
  /** Section pages drawn as tabs (study modes are opened from the deck page instead). */
  const sectionTabs = $derived(sectionNow?.entries.filter((e) => e.tab) ?? []);

  const showTabs = $derived(!!sectionNow && sectionTabs.length > 1 && !locked && !focusOn && !!current?.tab);
  const showPicker = $derived(!!screen?.deckPicker && !focusOn && !locked);
  // The screen's PageHeader draws these (title first, picker on the right, tabs under it).
  setPageChrome({
    get actions() {
      return showPicker ? deckPickerChrome : null;
    },
    get tabs() {
      return showTabs ? sectionTabsChrome : null;
    },
    // Write-only (called from screens' effects: reading the state here would loop them).
    setTrail: (crumbs, up) => {
      const some = !!crumbs && crumbs.length > 0;
      screenTrail = some ? crumbs : null;
      screenUp = some && up ? up : null;
    },
  });

  /** Top bar trail: the place (section › page, Settings › page, Inbox), then the screen's crumbs. */
  const trail = $derived.by((): Crumb[] => {
    const out: Crumb[] = [];
    const p = current?.place;
    if (p?.kind === 'section') {
      const s = nav.sections.find((x) => x.id === p.id);
      const label = s ? t(s.label_key) : td(current!.label_key);
      out.push({ label, onclick: s ? () => openSection(s.id) : undefined });
      const page = td(current!.label_key);
      if (current!.id !== 'home' && page !== label) out.push({ label: page, onclick: screenUp ?? undefined });
      else if (screenUp) out[0] = { label, onclick: screenUp };
    } else if (p?.kind === 'settings') {
      out.push({ label: t('shell.settings'), onclick: openSettings }, { label: td(current!.label_key), onclick: screenUp ?? undefined });
    } else if (p?.kind === 'inbox') {
      out.push({ label: t('shell.inbox'), onclick: screenUp ?? undefined });
    }
    return [...out, ...(screenTrail ?? [])];
  });

  /** "Add card" until the Quick Add dialog registers itself: the Create page's Add tab. */
  const addEntry = $derived(nav.all.find((e) => e.screen.id === 'add_note') ?? null);
  $effect(() => {
    const entry = addEntry;
    return setQuickAddFallback(entry ? () => show(entry) : null);
  });
  const searchEntry = $derived(nav.all.find((e) => e.screen.id === 'browse') ?? null);
  /** The top bar's search field: the command palette, or the Search page when it is off. */
  const onsearch = $derived(
    shellCommands.canPalette ? () => void openPalette() : searchEntry ? () => show(searchEntry) : null,
  );

  /** `N` anywhere (not while typing, not over a dialog, not in a running session) adds a card. */
  function onGlobalKey(e: KeyboardEvent) {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
    if (e.key !== 'n' && e.key !== 'N') return;
    if (locked || focusOn || typingTarget(e.target)) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    if (openQuickAdd()) e.preventDefault();
  }

  /** Where an entry lives, for the palette ("Desteler · Kütüphane", "Kısayollar · Ayarlar"). */
  function placeHint(e: NavEntry): string | null {
    const p = e.place;
    if (p.kind === 'settings') return t('shell.settings');
    if (p.kind !== 'section') return null;
    const s = nav.sections.find((x) => x.id === p.id);
    return s ? t(s.label_key) : null;
  }

  const placeKey = (e: NavEntry) => (e.place.kind === 'section' ? `section.${e.place.id}` : e.place.kind);

  function show(entry: NavEntry) {
    if (active !== entry.id) {
      focusOn = false;
      screenTrail = null;
      screenUp = null;
    }
    active = entry.id;
    if (entry.tab) lastIn.set(placeKey(entry), entry.id);
  }

  function reopen(key: string, list: NavEntry[]) {
    const last = lastIn.get(key);
    const e = list.find((x) => x.id === last) ?? list[0];
    if (e) show(e);
  }

  /** The sidebar returns to the section's last tab page (Library → the deck list, not a mode). */
  const openSection = (id: SectionId) => reopen(`section.${id}`, nav.sections.find((s) => s.id === id)?.entries.filter((e) => e.tab) ?? []);
  const openSettings = () => reopen('settings', nav.settings.flatMap((g) => g.entries));
  const openInbox = () => nav.inbox && show(nav.inbox);

  /**
   * Opens the entry with `route` (flashcard:// links, v0.9; `ctx.navigate`). v1.3: a route no
   * enabled module offers says so instead of silently staying put.
   */
  function openRoute(route: string) {
    const item = nav.all.find((m) => m.route === route);
    if (item) show(item);
    else if (modules.length > 0) error = t('shell.route_off');
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

{#snippet deckPickerChrome()}
  <DeckPicker {paths} all={screen?.deckPicker === 'all'} value={selectedDeck} onchange={(id) => (selectedDeck = id)} />
{/snippet}

{#snippet sectionTabsChrome()}
  {#if sectionNow}
    <div class="section-tabs">
      <Tabs
        label={t('shell.section_nav', { section: t(sectionNow.label_key) })}
        tabs={sectionTabs.map((e) => ({ id: e.id, label: td(e.label_key) }))}
        value={active ?? ''}
        onchange={(id) => {
          const e = sectionTabs.find((x) => x.id === id);
          if (e) show(e);
        }}
      />
    </div>
  {/if}
{/snippet}

<!-- A running session (v1.2) owns the window: no sidebar, no top bar (the session bar can bring the sidebar back). -->
<div class="shell" class:session={focusOn && !locked}>
  <div class="side">
  <AppSidebar {nav} current={where} {inboxCount} {openSection} {openInbox} {openSettings}>
    {#snippet profile()}
      <ProfileMenu
        openProfiles={nav.all.some((e) => e.route === '/profiles') ? () => openRoute('/profiles') : null}
        {guarded}
      />
    {/snippet}
    {#snippet panels()}<PluginPanels {modules} />{/snippet}
  </AppSidebar>
  </div>

  <div class="column">
  {#if !(focusOn && !locked)}
    <TopBar {trail} {onsearch} searchKeys={shellCommands.paletteKeys} onadd={shellCommands.canQuickAdd && !locked ? () => void openQuickAdd() : null} onhelp={shellCommands.canHelp && !locked ? () => void openHelp() : null} />
  {/if}
  <main id="fc-main" tabindex="-1">
    <div class="page" class:focus={focusOn && !locked} class:wide={screen?.width === 'wide' && where !== 'settings'}>
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
        {@render body()}
      {/if}
    </div>
  </main>
  </div>
</div>

<svelte:window onkeydown={onGlobalKey} />

<ToastHost />

<!-- First-run guide on a new, empty collection (v1.0). -->
<!-- Loaded lazily (v1.1 bundle budget): they only matter once their check says so. -->
{#await import('$lib/features/onboarding/OnboardingHost.svelte') then { default: OnboardingHost }}
  <OnboardingHost ondone={() => void loadDecks()} {ctx} home={() => show(nav.all[0]!)} />
{/await}

<!-- Quick Add dialog (v1.2): the shell's "add a card"; its dialog chunk loads on first open. -->
{#await import('$lib/features/quickadd/QuickAddHost.svelte') then { default: QuickAddHost }}
  <QuickAddHost {ctx} enabled={!!addEntry} />
{/await}

<!-- Study launcher (v1.3): every "study" entry opens it; its dialog chunk loads on first open. -->
{#await import('$lib/features/study/LauncherHost.svelte') then { default: LauncherHost }}
  <LauncherHost {ctx} />
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
  screens={[
    ...nav.all.map((m) => ({ id: m.screen.id, label: td(m.label_key), hint: placeHint(m), open: () => show(m) })),
    // Not a screen: the shortcuts help sheet (HelpHost), listed so the palette finds it.
    ...(shellCommands.canHelp ? [{ id: 'shell.help', label: t('help.palette'), hint: null, open: () => void openHelp() }] : []),
  ]}
  activeScreen={screen?.id ?? null}
  {guarded}
  deckId={selectedDeck}
/>

<!-- Keyboard shortcuts help (v1.3): `?`, the top bar's "?", the palette. After ShortcutHost: a key it handled stays handled. -->
<HelpHost openShortcuts={nav.all.some((e) => e.route === '/shortcuts') ? () => openRoute('/shortcuts') : null} />

<style>
  .shell {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    height: 100%;
    background: var(--surface-0);
  }
  .column {
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface-0);
  }
  /* Focused only by the sidebar's "Skip to content" (a landmark, not a control: no ring). */
  main:focus {
    outline: none;
  }
  /*
   * One content container for every page (docs/11 §4 `--content-*`): same left offset and
   * max-width everywhere; tables and charts (Search, Statistics) use the documented wide variant.
   */
  .page {
    max-width: calc(var(--content-max) + 2 * var(--content-pad-x));
    padding: var(--space-6) var(--content-pad-x) var(--space-12);
  }
  .page.wide {
    max-width: calc(var(--content-max-wide) + 2 * var(--content-pad-x));
  }
  /*
   * A running session (v1.2): no sidebar, no top bar, picker or tabs. The page spans the
   * window; the session screen draws its own bar and centres its column (modes/SessionFrame).
   * The session bar's menu button (`html.fc-session-nav`) brings the sidebar back.
   */
  .side {
    display: contents;
  }
  .shell.session {
    grid-template-columns: minmax(0, 1fr);
  }
  .shell.session .side {
    display: none;
  }
  :global(html.fc-session-nav) .shell.session {
    grid-template-columns: auto minmax(0, 1fr);
  }
  :global(html.fc-session-nav) .shell.session .side {
    display: contents;
  }
  .page.focus {
    max-width: none;
    padding: 0;
  }
  .page.focus > .banner {
    margin: var(--space-3) var(--content-pad-x) 0;
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin: 0 0 var(--space-4);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    font-size: var(--text-sm);
  }
  .banner.error {
    background: var(--danger-soft);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
    color: var(--danger);
  }
  .banner.notice {
    background: var(--warning-soft);
    border-color: color-mix(in srgb, var(--warning) 35%, transparent);
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
      padding-top: var(--space-4);
      padding-bottom: var(--space-8);
    }
  }
</style>
