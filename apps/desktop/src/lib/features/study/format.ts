import type { MessageKey } from '$lib/i18n/index.svelte';

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** i18n key + count for a "next review in …" label ("10m", "3d", "2mo"). */
export function intervalLabel(ms: number): { key: MessageKey; n: number } {
  if (ms < MIN) return { key: 'study.interval.now', n: 0 };
  if (ms < HOUR) return { key: 'study.interval.minutes', n: Math.round(ms / MIN) };
  if (ms < DAY) return { key: 'study.interval.hours', n: Math.round(ms / HOUR) };
  const days = ms / DAY;
  if (days < 30) return { key: 'study.interval.days', n: Math.round(days) };
  if (days < 365) return { key: 'study.interval.months', n: Math.round((days / 30) * 10) / 10 };
  return { key: 'study.interval.years', n: Math.round((days / 365) * 10) / 10 };
}

/** Local UTC offset in minutes, as the backend expects (Istanbul: 180). */
export function tzOffsetMins(date = new Date()): number {
  return -date.getTimezoneOffset();
}
