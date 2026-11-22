/**
 * Display formatting of numbers the backend computed (rates, durations). Presentation only:
 * no statistic is derived here. Thin names over `$lib/format` (the one locale-aware helper).
 */
import { formatDecimal, formatNumber, formatPercent, formatPercent100, formatShortDate, formatShortMonth, localeTag } from '$lib/format';

/** `0.873` → `87%` (`–` when there is no rate). */
export function pct(rate: number | null | undefined, digits = 0): string {
  return formatPercent(rate, digits);
}

/** A 0–100 value as `42%`. */
export function pct100(value: number, digits = 0): string {
  return formatPercent100(value, digits);
}

/** Milliseconds as seconds with one decimal. */
export function seconds(ms: number | null | undefined): string {
  return ms === null || ms === undefined ? '–' : formatDecimal(ms / 1000, 1);
}

/** Milliseconds as whole minutes. */
export function minutes(ms: number): number {
  return Math.round(ms / 60000);
}

/** A number with at most `digits` decimals, trailing zeros dropped. */
export function num(value: number, digits = 1): string {
  return formatNumber(value, digits);
}

/** Client UTC offset in minutes (Istanbul: 180), as the backend expects it. */
export function tzOffset(): number {
  return -new Date().getTimezoneOffset();
}

/** One bar of `BarChart`. */
export interface Bar {
  label: string;
  value: number;
  /** Tooltip / accessible text. */
  title: string;
}

/** `2025-11-04` → `4 Nov` / `4 Kas` (display only; the date comes from the backend). */
export function shortDate(date: string, locale: string): string {
  return formatShortDate(date, locale as 'tr' | 'en');
}

/** `2025-11-04` → `Nov` / `Kas` (heatmap month labels). */
export function shortMonth(date: string, locale: string): string {
  return formatShortMonth(date, locale as 'tr' | 'en');
}

/** Weekday of a `YYYY-MM-DD` date, Monday = 0 (heatmap rows). */
export function weekdayMon0(date: string): number {
  const d = new Date(`${date}T00:00:00Z`).getUTCDay();
  return (d + 6) % 7;
}

/** Intensity level 0–4 of `value` relative to the busiest day (colour steps of the heatmap). */
export function level(value: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (value <= 0 || max <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((value / max) * 4))) as 1 | 2 | 3 | 4;
}

/** `2025-11-04` → `Tue` / `Sal` (labels of a week chart; the date comes from the backend). */
export function shortWeekday(date: string, locale: string): string {
  return new Intl.DateTimeFormat(localeTag(locale), { weekday: 'short', timeZone: 'UTC' }).format(new Date(`${date}T00:00:00Z`));
}
