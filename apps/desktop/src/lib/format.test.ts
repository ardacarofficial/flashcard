import { describe, expect, it } from 'vitest';
import {
  formatCount,
  formatCurrency,
  formatDate,
  formatDecimal,
  formatDuration,
  formatMegabytes,
  formatMinutes,
  formatNumber,
  formatPercent,
  formatPercent100,
  formatRelative,
  formatSeconds,
  formatShortDate,
  localeTag,
} from './format';

describe('format (Intl, active locale)', () => {
  it('percentages follow the language: %35 / 35%', () => {
    expect(formatPercent(0.35, 0, 'tr')).toBe('%35');
    expect(formatPercent(0.35, 0, 'en')).toBe('35%');
    expect(formatPercent(0.8734, 1, 'tr')).toBe('%87,3');
    expect(formatPercent(0.8734, 1, 'en')).toBe('87.3%');
    expect(formatPercent100(61, 0, 'tr')).toBe('%61');
    expect(formatPercent100(100, 0, 'en')).toBe('100%');
    expect(formatPercent(null)).toBe('–');
  });

  it('decimals, counts and plain numbers', () => {
    expect(formatDecimal(15, 1, 'tr')).toBe('15,0');
    expect(formatDecimal(15, 1, 'en')).toBe('15.0');
    expect(formatNumber(2.5, 1, 'tr')).toBe('2,5');
    expect(formatNumber(2, 1, 'en')).toBe('2');
    expect(formatCount(1234, 'tr')).toBe('1.234');
    expect(formatCount(1234, 'en')).toBe('1,234');
  });

  it('durations: 15,0 sn / 15.0 s', () => {
    expect(formatSeconds(15000, 1, 'tr')).toBe('15,0 sn');
    expect(formatSeconds(15000, 1, 'en')).toBe('15.0 s');
    expect(formatMinutes(30 * 60_000, 'tr')).toBe('30 dk');
    expect(formatMinutes(30 * 60_000, 'en')).toBe('30 min');
    expect(formatDuration(45_000, 'en')).toBe('45.0 s');
    expect(formatDuration(90 * 60_000, 'tr')).toBe('1,5 sa');
    expect(formatSeconds(null)).toBe('–');
  });

  it('sizes and money', () => {
    expect(formatMegabytes(2_500_000, 'tr')).toBe('2,5 MB');
    expect(formatCurrency(1.5, 'USD', 'en')).toBe('$1.50');
    expect(formatCurrency(0.0012, 'USD', 'en')).toBe('$0.0012');
  });

  it('dates', () => {
    const ms = Date.UTC(2026, 10, 20, 12);
    expect(formatDate(ms, 'tr')).toBe('20.11.2026');
    expect(formatDate(ms, 'en')).toBe('11/20/2026');
    expect(formatShortDate('2025-11-04', 'tr')).toBe('4 Kas');
    expect(formatShortDate('2025-11-04', 'en')).toBe('Nov 4');
    expect(formatRelative(ms - 3 * 86_400_000, ms, 'en')).toBe('3 days ago');
    expect(formatRelative(ms + 86_400_000, ms, 'tr')).toBe('yarın');
    expect(localeTag('tr')).toBe('tr-TR');
  });
});
