/**
 * Display formatting of numbers the backend computed (rates, durations). Presentation only:
 * no statistic is derived here.
 */

/** `0.873` → `87%` (`–` when there is no rate). */
export function pct(rate: number | null | undefined, digits = 0): string {
  return rate === null || rate === undefined ? '–' : `${(rate * 100).toFixed(digits)}%`;
}

/** A 0–100 value as `42%`. */
export function pct100(value: number, digits = 0): string {
  return `${value.toFixed(digits)}%`;
}

/** Milliseconds as seconds with one decimal. */
export function seconds(ms: number | null | undefined): string {
  return ms === null || ms === undefined ? '–' : (ms / 1000).toFixed(1);
}

/** Milliseconds as whole minutes. */
export function minutes(ms: number): number {
  return Math.round(ms / 60000);
}

/** A number with at most `digits` decimals, trailing zeros dropped. */
export function num(value: number, digits = 1): string {
  return String(Number(value.toFixed(digits)));
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
