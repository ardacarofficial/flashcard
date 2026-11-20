import { t } from '$lib/i18n/index.svelte';

/** Human size of a byte count (KB below 1 MB, MB above). */
export function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return t('app_maintenance.size.kb', { n: Math.max(1, Math.round(bytes / 1024)) });
  return t('app_maintenance.size.mb', { n: (bytes / (1024 * 1024)).toFixed(1) });
}

/** Default file name of an exported log bundle. */
export function bundleName(version: string, now = new Date()): string {
  const d = now.toISOString().slice(0, 10);
  return `flashcard-logs-${version}-${d}.zip`;
}
