<script lang="ts">
  /**
   * Shell-level keyboard handling and the command palette (Ctrl+K by default). Bindings come
   * from `shortcuts.list`, entries from `palette.list` (every public backend method); this
   * component only maps keys to them. Screen-specific actions (`browse.*`) are forwarded as a
   * `fc:shortcut` window event to the active screen.
   */
  import { onMount, tick } from 'svelte';
  import { api } from '$lib/api';
  import type { PaletteEntry, ShortcutDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { bindingMap, comboOf, filterItems, paletteItems, typingTarget, type PaletteItem } from './palette';

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

  const items = $derived(
    filterItems(
      paletteItems(entries, screens.map((s) => ({ id: s.id, label: s.label })), (k) => td(k)),
      query,
    ),
  );
  const contexts = $derived(activeScreen === 'browse' ? ['browse'] : []);
  const bindings = $derived(bindingMap(shortcuts, contexts));

  async function load() {
    try {
      entries = (await api.call('palette.list', {})).entries;
      shortcuts = (await api.call('shortcuts.list', {})).items;
    } catch {
      // Module turned off: no palette, no custom bindings.
      entries = [];
      shortcuts = [];
    }
  }

  async function show() {
    await load();
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
        return screen('add_note');
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

  onMount(() => {
    void load();
    const unsub = api.on(['setting_changed', 'module_toggled', 'undone', 'redone'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div class="backdrop" role="presentation" onclick={close}></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label={t('palette.title')}>
    {#if pending}
      <form onsubmit={(e) => (e.preventDefault(), void runPending())}>
        <p class="name">{td(pending.label_key) === pending.label_key ? pending.method : td(pending.label_key)}</p>
        <label>
          {t('palette.params')}
          <textarea rows="6" bind:value={params} spellcheck="false"></textarea>
        </label>
        {#if paramsError}<p class="error" role="alert">{t('palette.invalid_json')}</p>{/if}
        <div class="row">
          <button type="button" onclick={() => (pending = null)}>{t('common.cancel')}</button>
          <button type="submit">{t('palette.run')}</button>
        </div>
      </form>
    {:else}
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
      />
      <ul id="palette-list" role="listbox">
        {#each items as item, i (item.id)}
          <li role="option" aria-selected={i === cursor}>
            <button class:active={i === cursor} onclick={() => void choose(item)}>
              <span>{item.label}</span>
              {#if item.kind === 'screen'}
                <small>{t('palette.screen')}</small>
              {:else if item.entry.shortcut}
                <kbd>{item.entry.shortcut}</kbd>
              {/if}
            </button>
          </li>
        {:else}
          <li class="muted">{t('palette.no_match')}</li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.25);
  }
  .palette {
    position: fixed;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, 90vw);
    background: var(--fc-bg);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 50vh;
    overflow: auto;
  }
  li button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    border: none;
    background: transparent;
    text-align: left;
  }
  li button.active {
    background: var(--fc-selected);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: var(--fc-space-2);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  textarea {
    font-family: monospace;
  }
</style>
