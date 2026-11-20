/** Which optional modules are on (`modules.list`), kept fresh by `module_toggled` events. */
import { api } from '$lib/api';

const state = $state<{ enabled: Record<string, boolean>; loaded: boolean }>({ enabled: {}, loaded: false });
let started = false;

async function load() {
  try {
    const list = await api.modules.list();
    state.enabled = Object.fromEntries(list.modules.map((m) => [m.id, m.enabled]));
  } catch {
    /* keep previous state */
  }
  state.loaded = true;
}

/** Starts tracking (idempotent). */
export function trackModules(): void {
  if (started) return;
  started = true;
  void load();
  void api.on(['module_toggled', 'undone', 'redone', 'data_changed'], () => void load());
}

/** Whether module `id` is enabled (unknown modules count as on until loaded). */
export function moduleOn(id: string): boolean {
  return state.enabled[id] ?? !state.loaded;
}

/**
 * Whether module `id` is enabled according to a loaded `modules.list` (`false` until then).
 * Gate calls into a module's methods with this, so a switched-off module is never called;
 * reactive readers re-run once the list arrives.
 */
export function moduleKnownOn(id: string): boolean {
  return state.loaded && state.enabled[id] === true;
}
