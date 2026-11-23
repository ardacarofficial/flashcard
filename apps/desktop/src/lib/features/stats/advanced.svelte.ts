/**
 * The one "Gelişmiş" switch (v1.4, D3 principle 1), as İlerleme reads it. The switch itself
 * lives in Settings (the shell's; E2). Until the shell exposes a shared store, this reads the
 * value the switch remembers — `localStorage['fc.ui.advanced'] = '1'` — and follows changes
 * made in another window (`storage`) or in this one (`fc:advanced` event). A per-viewer view
 * preference, not data: nothing is hidden for good, only moved out of the way.
 *
 * To flip the default or rewire to the shell's store: change {@link KEY} / {@link readAdvanced}.
 */
export const KEY = 'fc.ui.advanced';
export const EVENT = 'fc:advanced';

function readAdvanced(): boolean {
  try {
    return globalThis.localStorage?.getItem(KEY) === '1';
  } catch {
    return false;
  }
}

class Advanced {
  on = $state(readAdvanced());
}

export const advanced = new Advanced();

if (typeof window !== 'undefined') {
  const sync = () => (advanced.on = readAdvanced());
  window.addEventListener('storage', (e) => {
    if (e.key === KEY) sync();
  });
  window.addEventListener(EVENT, sync);
}

/** Sets the switch (Settings, tests). */
export function setAdvanced(on: boolean): void {
  try {
    globalThis.localStorage?.setItem(KEY, on ? '1' : '0');
  } catch {
    /* private window: the value still applies to this page */
  }
  advanced.on = on;
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(EVENT));
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
