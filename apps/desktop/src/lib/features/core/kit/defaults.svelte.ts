/**
 * Setting defaults for the kit's "default" marker and "Reset to default" (v1.3). The backend
 * owns defaults (`settings.schema`) and resetting (`settings.reset` removes the override, an
 * undoable command); this only asks and compares.
 */
import { api } from '$lib/api';
import type { SettingScope } from '$lib/api/types';

let schema: Promise<Map<string, unknown>> | null = null;
const state = $state<{ defaults: Map<string, unknown> | null }>({ defaults: null });

/** Loads `settings.schema` once (settings of enabled modules) and keeps the defaults. */
export function loadDefaults(): Promise<Map<string, unknown>> {
  schema ??= api
    .call('settings.schema', {})
    .then((s) => new Map(s.settings.map((e) => [e.def.key, e.def.default] as [string, unknown])))
    .catch(() => new Map<string, unknown>());
  void schema.then((m) => (state.defaults = m));
  return schema;
}

/** Forgets the cache (tests; a module switched on adds settings). */
export function resetDefaultsCache(): void {
  schema = null;
  state.defaults = null;
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** Default of `key` (`undefined` while loading or when the key is unknown). */
export function defaultOf(key: string): unknown {
  return state.defaults?.get(key);
}

/**
 * `true`/`false` when the default of `key` is known, else `undefined` (no marker shown):
 * pass to `FormRow isDefault`.
 */
export function isDefault(key: string, value: unknown): boolean | undefined {
  const d = state.defaults;
  if (!d || !d.has(key)) return undefined;
  return same(d.get(key), value);
}

/** Removes the override of `key` at `scope`; resolves to the value now in effect. */
export async function resetSetting(key: string, scope: SettingScope = { kind: 'app' }): Promise<unknown> {
  const r = await api.call('settings.reset', { scope, key });
  return r.value;
}
