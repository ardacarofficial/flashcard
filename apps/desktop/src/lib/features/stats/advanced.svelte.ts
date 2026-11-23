/**
 * The one "Gelişmiş" switch (v1.4, D3 principle 1), as İlerleme reads it: the app-wide switch
 * of the shell (`$lib/shell/advanced.svelte`, core setting `app.show_advanced`), which Settings,
 * the sidebar and the study launcher share. A view preference, not data: nothing is hidden
 * for good, only moved out of the way.
 */
import { advanced } from '$lib/shell/advanced.svelte';

export { advanced };

/** Sets the switch for this window only (tests); Settings saves it through `advanced.set`. */
export function setAdvanced(on: boolean): void {
  advanced.on = on;
}

/** Per-viewer: whether "Daha fazla" is open (remembered; a convenience only). */
const MORE_KEY = 'fc.stats.more';
export function readMore(): boolean {
  try {
    return globalThis.localStorage?.getItem(MORE_KEY) === '1';
  } catch {
    return false;
  }
}
export function writeMore(on: boolean): void {
  try {
    globalThis.localStorage?.setItem(MORE_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}
