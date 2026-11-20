import { api } from '$lib/api';
import type { ModuleDto, UiText } from '$lib/api/types';
import { LOCALES, setRuntimeMessages, td } from '$lib/i18n/index.svelte';

/**
 * Loads every installed plugin's own translations into the i18n overlay, as
 * `plugin.<plugin id>.<key>` (menu labels from `modules.list`, UI texts, setting labels).
 */
export async function loadPluginTexts(): Promise<void> {
  for (const lang of LOCALES) {
    const { plugins } = await api.call('plugins.locales', { lang });
    const flat: Record<string, string> = {};
    for (const [id, map] of Object.entries(plugins)) {
      for (const [k, v] of Object.entries(map ?? {})) if (v !== undefined) flat[`plugin.${id}.${k}`] = v;
    }
    setRuntimeMessages(lang, 'plugin.', flat);
  }
}

/** Text of plugin UI: a key of the plugin's locale files, or literal content. */
export function uiText(pluginId: string, text: UiText | null | undefined): string {
  if (!text) return '';
  if ('text' in text) return text.text;
  const params: Record<string, string> = {};
  for (const [k, v] of Object.entries(text.params ?? {})) if (v !== undefined) params[k] = v;
  return td(`plugin.${pluginId}.${text.key}`, params);
}

/** `/plugins/view/<plugin id>/<view>` → its parts. */
export function parseViewRoute(route: string | null): { pluginId: string; view: string } | null {
  const m = /^\/plugins\/view\/([^/]+)\/([^/]+)$/.exec(route ?? '');
  return m ? { pluginId: m[1]!, view: m[2]! } : null;
}

/** Contributions of enabled modules in `slot`. */
export function contributions(modules: ModuleDto[], slot: string) {
  return modules
    .filter((m) => m.enabled)
    .flatMap((m) => m.ui)
    .filter((c) => c.slot === slot)
    .sort((a, b) => a.order - b.order);
}

/** `v1.plugin.<plugin id>.<command>` → its parts (command ids have no dots). */
export function parseCommandMethod(method: string | null): { pluginId: string; command: string } | null {
  const rest = method?.startsWith('v1.plugin.') ? method.slice('v1.plugin.'.length) : null;
  const dot = rest?.lastIndexOf('.') ?? -1;
  return rest && dot > 0 ? { pluginId: rest.slice(0, dot), command: rest.slice(dot + 1) } : null;
}
