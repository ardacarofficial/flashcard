/**
 * Plain words for a plugin permission string (v1.3), shown next to the code at install time
 * and in the plugin's panel. Presentation only: which permissions exist, which are sensitive
 * and what they allow is decided in fc-plugins (`permission.rs`); an unknown form falls back
 * to a generic line.
 */
import { t, td } from '$lib/i18n/index.svelte';

/** `plugins.perm.<group>.<name>` if the dictionary has it, else `fallback`. */
function known(key: string, fallback: string, params?: Record<string, string>): string {
  const s = td(key, params);
  return s === key ? fallback : s;
}

export function describePermission(permission: string): string {
  const i = permission.indexOf(':');
  if (i < 0) return t('plugins.perm.unknown');
  const area = permission.slice(0, i);
  const rest = permission.slice(i + 1);
  const host = known(`plugins.perm.host.${permission.replace(':', '_')}`, '');
  if (host) return host;
  if (area === 'network') return t('plugins.perm.network', { host: rest });
  if (area === 'ui') return t('plugins.perm.ui', { point: known(`plugins.perm.point.${rest}`, rest) });
  if (area === 'hooks') {
    const [when = '', target = ''] = [rest.slice(0, rest.indexOf(':')), rest.slice(rest.indexOf(':') + 1)];
    return known(`plugins.perm.hook.${when}`, t('plugins.perm.unknown'), { target });
  }
  if (rest === 'read' || rest === 'write') {
    return t(`plugins.perm.${rest}`, { area: known(`plugins.perm.area.${area}`, area) });
  }
  return t('plugins.perm.unknown');
}
