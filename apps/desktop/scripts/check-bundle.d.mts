/** One chunk of a Vite build manifest (the fields the budget reads). */
export interface ManifestChunk {
  file: string;
  isEntry?: boolean;
  isDynamicEntry?: boolean;
  imports?: string[];
  dynamicImports?: string[];
  css?: string[];
}
export type Manifest = Record<string, ManifestChunk>;
export const BUDGETS: { js: number; css: number };
export const LAZY_ONLY: RegExp[];
export function entryKey(manifest: Manifest): string;
export function initialChunks(manifest: Manifest): Set<string>;
export function initialFiles(manifest: Manifest): { js: string[]; css: string[] };
export function lazyScreens(featuresDir: string): { feature: string; key: string }[];
export function problems(
  manifest: Manifest,
  sizeOf: (file: string) => number,
  lazy: { key: string }[],
  budgets?: { js: number; css: number },
): string[];
