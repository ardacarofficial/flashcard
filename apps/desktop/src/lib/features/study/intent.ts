/**
 * "Start studying" from Home or a deck page (v1.2 flow 2): one keystroke starts the session.
 * The caller marks the intent, then navigates to `/study`; the Cards screen takes it once on
 * mount and starts at once instead of showing its start panel (which stays reachable from the
 * deck page's "Cards" mode button and the palette, for options and custom sessions).
 */
let pending = false;

export function studyNow(): void {
  pending = true;
}

/** Read-once: true if the screen was opened to start right away. */
export function takeStudyNow(): boolean {
  const p = pending;
  pending = false;
  return p;
}
