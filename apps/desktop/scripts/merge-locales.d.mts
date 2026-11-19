/** A merged dictionary: nested groups of messages. */
export interface MergedDictionary {
  [key: string]: string | MergedDictionary;
}
/** Merged dictionaries by language. Throws on conflicts or mismatched feature files. */
export function mergeLocales(): Record<string, MergedDictionary>;
/** Writes `generated/<lang>.json`; returns the languages. */
export function writeLocales(): string[];
