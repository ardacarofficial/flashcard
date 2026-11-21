<script lang="ts">
  /**
   * Shell-level keyboard handling and the command palette (Ctrl+K by default). Bindings come
   * from `shortcuts.list`, entries from `palette.list` (every public backend method); this
   * component only maps keys to them. Screen-specific actions (`browse.*`) are forwarded as a
   * `fc:shortcut` window event to the active screen. The two lists load independently: with
   * the `palette` module off only the palette goes away, the keys keep working.
   */
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { PaletteEntry, ShortcutDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { openQuickAdd, setPaletteHandler, setPaletteKeys, shellCommands } from '$lib/shell/commands.svelte';
  // Eager (App.svelte mounts it): lib/ui file by file, never the barrel.
  import Button from '$lib/ui/Button.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import Kbd from '$lib/ui/Kbd.svelte';
  import Textarea from '$lib/ui/Textarea.svelte';
  import { LayoutGrid, Plus, Search, Terminal, Undo2 } from '$lib/ui/icons';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import { bindingMap, comboOf, filterItems, groupItems, paletteItems, typingTarget, type PaletteAction, type PaletteItem } from './palette';

  interface ScreenLink {
    id: string;
    label: string;
    open: () => void;
  }

  let {
    screens,
    activeScreen,
    guarded,
  }: {
    screens: ScreenLink[];
    activeScreen: string | null;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
  } = $props();

  let entries = $state<PaletteEntry[]>([]);
  let shortcuts = $state<ShortcutDto[]>([]);
  let open = $state(false);
  let query = $state('');
  let cursor = $state(0);
  let pending = $state<PaletteEntry | null>(null);
  let params = $state('{}');
  let paramsError = $state(false);
  let input = $state<HTMLInputElement | null>(null);
  let list = $state<HTMLUListElement | null>(null);

  const comboFor = (action: string) => shortcuts.find((s) => s.action === action)?.combo ?? null;
  /** Shell actions on top: "Add card" (N) first, then search, undo, redo. */
  const actions = $derived<PaletteAction[]>([
    ...(shellCommands.canQuickAdd ? [{ action: 'ui.new_note', label: t('palette.action_add_card'), keys: 'N' }] : []),
    { action: 'ui.search', label: t('shortcuts.ui.search'), keys: comboFor('ui.search') },
    { action: 'ui.undo', label: t('shortcuts.ui.undo'), keys: comboFor('ui.undo') },
    { action: 'ui.redo', label: t('shortcuts.ui.redo'), keys: comboFor('ui.redo') },
  ]);
  /** Screens show the key that opens them (Search: `ui.search`). */
  const SCREEN_KEYS: Record<string, string> = { browse: 'ui.search' };
  const grouped = $derived(
    groupItems(
      filterItems(
        paletteItems(
          entries,
          screens.map((s) => ({ id: s.id, label: s.label, keys: SCREEN_KEYS[s.id] ? comboFor(SCREEN_KEYS[s.id]!) : null })),
          (k) => td(k),
          actions,
        ),
        query,
      ),
      {
        actions: t('palette.group_actions'),
        screens: t('palette.group_screens'),
        groupTitle: (m) => {
          const name = td(`modules.${m}.name`);
          return name === `modules.${m}.name` ? t('palette.group_commands') : name;
        },
      },
    ),
  );
  /** Rows in display order (arrow keys walk this). */
  const items = $derived(grouped.flat);
  const contexts = $derived(activeScreen === 'browse' ? ['browse'] : []);
  const bindings = $derived(bindingMap(shortcuts, contexts));

  const paletteOn = $derived(moduleKnownOn('palette'));

  async function loadEntries(on: boolean) {
    try {
      entries = on ? (await api.call('palette.list', {})).entries : [];
    } catch {
      entries = [];
    }
  }

  async function loadShortcuts() {
    try {
      shortcuts = (await api.call('shortcuts.list', {})).items;
    } catch {
      shortcuts = [];
    }
  }

  async function show() {
    if (!paletteOn) return;
    await loadEntries(true);
    open = true;
    query = '';
    cursor = 0;
    pending = null;
    await tick();
    input?.focus();
  }

  function close() {
    open = false;
    pending = null;
  }

  async function runMethod(entry: PaletteEntry, body: unknown) {
    const ok = await guarded(() => api.call(entry.method.replace(/^v1\./, '') as never, body as never));
    if (ok) close();
  }

  async function choose(item: PaletteItem | undefined) {
    if (!item) return;
    if (item.kind === 'screen') {
      screens.find((s) => s.id === item.screenId)?.open();
      close();
    } else if (item.kind === 'action') {
      close();
      await act(item.action);
    } else if (item.entry.direct) {
      await runMethod(item.entry, {});
    } else {
      pending = item.entry;
      params = '{}';
      paramsError = false;
    }
  }

  async function runPending() {
    if (!pending) return;
    let body: unknown;
    try {
      body = JSON.parse(params);
    } catch {
      body = null;
    }
    if (typeof body !== 'object' || body === null || Array.isArray(body)) {
      paramsError = true;
      return;
    }
    await runMethod(pending, body);
  }

  async function act(action: string) {
    const screen = (id: string) => screens.find((s) => s.id === id)?.open();
    switch (action) {
      case 'ui.palette':
        return show();
      case 'ui.search':
        return screen('browse');
      case 'ui.new_note':
        // Quick Add (v1.2; the shell falls back to the Create page).
        return void (openQuickAdd() || screen('add_note'));
      case 'ui.undo':
        return void guarded(() => api.oplog.undo());
      case 'ui.redo':
        return void guarded(() => api.oplog.redo());
    }
    const entry = entries.find((e) => e.method === action);
    if (entry?.direct) return runMethod(entry, {});
    window.dispatchEvent(new CustomEvent('fc:shortcut', { detail: action }));
  }

  function onKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.preventDefault();
      close();
      return;
    }
    if (open) return;
    const combo = comboOf(e);
    if (!combo) return;
    // Plain keys typed into a field belong to the field.
    if (typingTarget(e.target) && !e.ctrlKey && !e.metaKey && !e.altKey) return;
    const actions = bindings.get(combo);
    if (!actions?.length) return;
    e.preventDefault();
    void act(actions[0]!);
  }

  function onListKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cursor = Math.min(cursor + 1, items.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = Math.max(cursor - 1, 0);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void choose(items[cursor]);
    }
  }

  // The palette (and its entries) only while the `palette` module is on.
  $effect(() => {
    const on = paletteOn;
    if (!on) close();
    void loadEntries(on);
  });

  // The top bar's search field opens the palette too, and shows its (rebindable) keys.
  $effect(() => (paletteOn ? setPaletteHandler(() => void show()) : undefined));
  $effect(() => setPaletteKeys(shortcuts.find((s) => s.action === 'ui.palette')?.combo ?? null));

  // Keep the highlighted row in view while moving with the arrow keys.
  $effect(() => {
    const i = cursor;
    list?.querySelector<HTMLElement>(`[data-index="${i}"]`)?.scrollIntoView?.({ block: 'nearest' });
  });

  onMount(() => {
    trackModules();
    void loadShortcuts();
    const unsub = api.on(['setting_changed', 'module_toggled', 'undone', 'redone', 'data_changed'], () => {
      void loadShortcuts();
      void loadEntries(paletteOn);
    });
    return () => void unsub.then((u) => u());
  });
</script>

<svelte:window onkeydown={onKey} />

{#if open && paletteOn}
  <div class="backdrop" role="presentation" onclick={close}></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label={t('palette.title')}>
    {#if pending}
      <form onsubmit={(e) => (e.preventDefault(), void runPending())}>
        <p class="name">{td(pending.label_key) === pending.label_key ? pending.method : td(pending.label_key)}</p>
        <p class="method">{pending.method}</p>
        <Textarea class="params" label={t('palette.params')} rows={6} bind:value={params} spellcheck="false" error={paramsError ? t('palette.invalid_json') : null} />
        <div class="row">
          <Button size="sm" variant="ghost" onclick={() => (pending = null)}>{t('palette.back')}</Button>
          <Button size="sm" variant="primary" type="submit" kbd="Enter">{t('palette.run')}</Button>
        </div>
      </form>
    {:else}
      <div class="field">
        <Icon icon={Search} size={16} />
        <input
          bind:this={input}
          bind:value={query}
          oninput={() => (cursor = 0)}
          onkeydown={onListKey}
          placeholder={t('palette.placeholder')}
          aria-label={t('palette.title')}
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={items[cursor] ? `palette-opt-${cursor}` : undefined}
          spellcheck="false"
          autocomplete="off"
        />
        <Kbd keys={['Esc']} size="sm" />
      </div>
      <ul id="palette-list" role="listbox" bind:this={list} aria-label={t('palette.title')}>
        {#each grouped.groups as g (g.title)}
          <li class="group" role="presentation"><span>{g.title}</span></li>
          {#each g.items as item, j (item.id)}
            {@const i = g.start + j}
            {@const keys = item.kind === 'method' ? item.entry.shortcut : item.keys}
            <li id="palette-opt-{i}" role="option" aria-selected={i === cursor} data-index={i}>
              <button type="button" tabindex="-1" class:active={i === cursor} onclick={() => void choose(item)} onpointermove={() => (cursor = i)}>
                <span class="ico" class:flip={item.kind === 'action' && item.action === 'ui.redo'}>
                  <Icon
                    icon={item.kind === 'screen' ? LayoutGrid : item.kind === 'method' ? Terminal : item.action === 'ui.new_note' ? Plus : item.action === 'ui.search' ? Search : Undo2}
                    size={14}
                  />
                </span>
                <span class="label">{item.label}</span>
                {#if item.kind === 'screen'}<span class="sr-only">{t('palette.screen')}</span>{/if}
                {#if keys}<span class="keys"><Kbd {keys} size="sm" /></span>{/if}
              </button>
            </li>
          {/each}
        {:else}
          <li class="none">{t('palette.no_match')}</li>
        {/each}
      </ul>
      <footer class="hints" aria-hidden="true">
        <span><Kbd keys={['ArrowUp']} size="sm" /><Kbd keys={['ArrowDown']} size="sm" /> {t('palette.hint_move')}</span>
        <span><Kbd keys={['Enter']} size="sm" /> {t('palette.hint_open')}</span>
        <span><Kbd keys={['Esc']} size="sm" /> {t('palette.hint_close')}</span>
      </footer>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: var(--backdrop);
    animation: fc-palette-fade var(--motion-fast) var(--ease);
  }
  .palette {
    position: fixed;
    z-index: 151;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    width: min(600px, calc(100vw - 32px));
    max-height: 70vh;
    overflow: hidden;
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-float);
    color: var(--text-1);
    animation: fc-palette-in var(--motion) var(--ease);
  }
  .field {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: 44px;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--border);
    color: var(--text-3);
  }
  .field input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-1);
    font-size: var(--text-md);
    outline: none;
  }
  .field input::placeholder {
    color: var(--text-3);
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: var(--space-1);
    overflow: auto;
    list-style: none;
  }
  .group {
    display: flex;
    align-items: center;
    height: 26px;
    padding: var(--space-2) var(--space-2) 0;
    color: var(--text-2);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
  }
  .group:not(:first-child) {
    margin-top: var(--space-1);
    border-top: 1px solid var(--border);
  }
  li button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-1);
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  li button.active {
    background: var(--surface-3);
  }
  .ico {
    display: inline-flex;
    color: var(--text-3);
  }
  li button.active .ico {
    color: var(--accent-text);
  }
  .ico.flip {
    transform: scaleX(-1);
  }
  .keys {
    flex: none;
    display: inline-flex;
    margin-inline-start: auto;
    color: var(--text-2);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    padding: var(--space-6) var(--space-3);
    color: var(--text-2);
    text-align: center;
  }
  .hints {
    display: flex;
    gap: var(--space-4);
    flex: none;
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--surface-0);
    color: var(--text-2);
    font-size: var(--text-2xs);
  }
  .hints span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .name {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .method {
    margin: -8px 0 0;
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  form :global(.params textarea) {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  @keyframes fc-palette-in {
    from {
      opacity: 0;
      transform: translate(-50%, -4px);
    }
  }
  @keyframes fc-palette-fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .palette,
    .backdrop {
      animation: none;
    }
  }
</style>
