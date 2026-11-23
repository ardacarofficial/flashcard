/**
 * Whether AI help can be offered at all: the `ai` module is on and at least one provider is
 * set up (`ai.providers.list`). Shared by every place that shows an AI action outside the AI
 * screen (the meaning-in-this-sentence entry of the note editor and the study menu), kept
 * fresh by `module_toggled` / `ai_config_changed` (and undo/redo, which can bring a provider
 * back). A provider that still lacks a key counts as set up: the preview says what is missing.
 */
import { api } from '$lib/api';

const state = $state<{ ready: boolean }>({ ready: false });
let started = false;
let seq = 0;

async function load() {
  const mine = ++seq;
  let ready = false;
  try {
    const mods = await api.modules.list();
    if (mods.modules.some((m) => m.id === 'ai' && m.enabled)) {
      ready = (await api.call('ai.providers.list', {})).providers.length > 0;
    }
  } catch {
    ready = false;
  }
  // An older answer never overwrites a newer one.
  if (mine === seq) state.ready = ready;
}

/** Starts tracking (idempotent) and re-reads now. */
export function trackAiReady(): void {
  if (!started) {
    started = true;
    void api.on(['module_toggled', 'ai_config_changed', 'undone', 'redone'], () => void load());
  }
  void load();
}

/** The `ai` module is on and a provider is set up (`false` until known). */
export function aiReady(): boolean {
  return state.ready;
}
