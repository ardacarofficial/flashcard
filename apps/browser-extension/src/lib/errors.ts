/** User-facing text for an `ApiError`: a specific message if we have one, else its category's. */
import type { ApiError } from './contract';
import { t } from './i18n';

const CATEGORIES = ['VALIDATION', 'NOT_FOUND', 'CONFLICT', 'PERMISSION'] as const;

/** Message key for an error code (`NOT_FOUND_DECK` → `error_not_found_deck`, else `error_not_found`). */
export function errorKeys(code: string): string[] {
  const specific = `error_${code.toLowerCase()}`;
  const cat = CATEGORIES.find((c) => code.startsWith(`${c}_`) || code === c);
  return [specific, cat ? `error_${cat.toLowerCase()}` : 'error_other'];
}

export function errorText(err: Pick<ApiError, 'code'>): string {
  for (const key of errorKeys(err.code)) {
    const text = t(key);
    if (text !== key) return text;
  }
  return t('error_other');
}
