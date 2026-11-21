/**
 * The one place that turns numbers and times into display text (docs/11 §4). Everything is
 * built on `Intl.NumberFormat` / `Intl.DateTimeFormat` / `Intl.RelativeTimeFormat` for the
 * active UI language, so TR shows `%35`, `15,0 sn`, `1.234` and EN `35%`, `15.0 s`, `1,234`.
 * Presentation only: values come computed from the backend. Screens never call `toFixed` or
 * glue `%` on themselves (lint: tests/number-format.test.ts).
 *
 * Every function takes an optional `locale` (tests); by default the current `i18n.locale`.
 */
import { translate } from '$lib/i18n/core';
import { i18n, type Locale } from '$lib/i18n/index.svelte';

const TAGS: Record<string, string> = { tr: 'tr-TR', en: 'en-US' };

/** BCP 47 tag of a UI language (`tr` → `tr-TR`). */
export function localeTag(locale: string = i18n.locale): string {
  return TAGS[locale] ?? locale;
}

const numberCache = new Map<string, Intl.NumberFormat>();
function nf(locale: string | undefined, opts: Intl.NumberFormatOptions): Intl.NumberFormat {
  const tag = localeTag(locale ?? i18n.locale);
  const key = `${tag}|${JSON.stringify(opts)}`;
  let f = numberCache.get(key);
  if (!f) numberCache.set(key, (f = new Intl.NumberFormat(tag, opts)));
  return f;
}

const DASH = '–';
const missing = (v: number | null | undefined): v is null | undefined => v === null || v === undefined || Number.isNaN(v);

/** A whole count with grouping: `1.234` / `1,234`. */
export function formatCount(n: number, locale?: Locale): string {
  return nf(locale, { maximumFractionDigits: 0 }).format(n);
}

/** A number with up to `max` decimals (trailing zeros dropped): `2,5` / `2.5`. */
export function formatNumber(v: number, max = 1, locale?: Locale): string {
  return nf(locale, { maximumFractionDigits: max }).format(v);
}

/** A number with exactly `digits` decimals: `15,0` / `15.0`. */
export function formatDecimal(v: number, digits = 1, locale?: Locale): string {
  return nf(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
}

/** A 0–1 ratio as a percentage: `0.35` → `%35` / `35%` (`–` when missing). */
export function formatPercent(ratio: number | null | undefined, digits = 0, locale?: Locale): string {
  if (missing(ratio)) return DASH;
  return nf(locale, { style: 'percent', minimumFractionDigits: digits, maximumFractionDigits: digits }).format(ratio);
}

/** A 0–100 value as a percentage: `35` → `%35` / `35%`. */
export function formatPercent100(value: number | null | undefined, digits = 0, locale?: Locale): string {
  return missing(value) ? DASH : formatPercent(value / 100, digits, locale);
}

const unit = (key: 'seconds' | 'minutes' | 'hours' | 'megabytes', n: string, locale?: Locale) =>
  translate(locale ?? i18n.locale, `ui.format.${key}`, { n });

/** Milliseconds as seconds: `15,0 sn` / `15.0 s`. */
export function formatSeconds(ms: number | null | undefined, digits = 1, locale?: Locale): string {
  return missing(ms) ? DASH : unit('seconds', formatDecimal(ms / 1000, digits, locale), locale);
}

/** Milliseconds as whole minutes: `30 dk` / `30 min`. */
export function formatMinutes(ms: number, locale?: Locale): string {
  return unit('minutes', formatCount(Math.round(ms / 60_000), locale), locale);
}

/** A duration in its natural unit: `45,0 sn`, `12 dk`, `1,5 sa` (EN: `45.0 s`, `12 min`, `1.5 h`). */
export function formatDuration(ms: number | null | undefined, locale?: Locale): string {
  if (missing(ms)) return DASH;
  if (ms < 60_000) return formatSeconds(ms, 1, locale);
  if (ms < 3_600_000) return formatMinutes(ms, locale);
  return unit('hours', formatNumber(ms / 3_600_000, 1, locale), locale);
}

/** Bytes as megabytes with one decimal: `2,5 MB`. */
export function formatMegabytes(bytes: number | bigint, locale?: Locale): string {
  return unit('megabytes', formatDecimal(Number(bytes) / 1_000_000, 1, locale), locale);
}

/** Money (AI usage costs): small amounts get 4 decimals. */
export function formatCurrency(value: number, currency = 'USD', locale?: Locale): string {
  const digits = Math.abs(value) > 0 && Math.abs(value) < 0.01 ? 4 : 2;
  return nf(locale, { style: 'currency', currency, minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
}

/** A timestamp (ms since the epoch) as a date: `20.11.2026` / `11/20/2026`. */
export function formatDate(ms: number | null | undefined, locale?: Locale): string {
  if (missing(ms)) return DASH;
  return new Date(ms).toLocaleDateString(localeTag(locale ?? i18n.locale));
}

/**
 * A timestamp as date and time (minutes): `20.11.2026 14:05` / `11/20/26, 2:05 PM`. `options`
 * picks other parts (e.g. weekday and time).
 */
export function formatDateTime(
  ms: number | null | undefined,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'short', timeStyle: 'short' },
  locale?: Locale,
): string {
  if (missing(ms)) return DASH;
  return new Date(ms).toLocaleString(localeTag(locale ?? i18n.locale), options);
}

/** A backend `YYYY-MM-DD` day as `4 Kas` / `Nov 4` (no time-zone shift). */
export function formatShortDate(date: string, locale?: Locale): string {
  const d = new Date(`${date}T00:00:00Z`);
  if (Number.isNaN(d.getTime())) return date;
  return d.toLocaleDateString(localeTag(locale ?? i18n.locale), { day: 'numeric', month: 'short', timeZone: 'UTC' });
}

/** The month of a backend `YYYY-MM-DD` day as `Kas` / `Nov` (chart axis labels). */
export function formatShortMonth(date: string, locale?: Locale): string {
  const d = new Date(`${date}T00:00:00Z`);
  if (Number.isNaN(d.getTime())) return date;
  return d.toLocaleDateString(localeTag(locale ?? i18n.locale), { month: 'short', timeZone: 'UTC' });
}

const STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['second', 60],
  ['minute', 60],
  ['hour', 24],
  ['day', 7],
  ['week', 4.35],
  ['month', 12],
  ['year', Infinity],
];

/** A timestamp relative to `now`: `3 gün önce` / `3 days ago`, `yarın` / `tomorrow`. */
export function formatRelative(ms: number, now = Date.now(), locale?: Locale): string {
  const rtf = new Intl.RelativeTimeFormat(localeTag(locale ?? i18n.locale), { numeric: 'auto' });
  let v = (ms - now) / 1000;
  for (const [u, size] of STEPS) {
    if (Math.abs(v) < size) return rtf.format(Math.round(v), u);
    v /= size;
  }
  return rtf.format(Math.round(v), 'year');
}
