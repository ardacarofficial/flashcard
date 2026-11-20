import type { ScreenComponent, ScreenDef, ScreenModule } from './types';

/**
 * Every `features/<feature>/screen.ts` is picked up here: a new feature adds files, no
 * list to edit. Mock-backend handlers are collected the same way in `api/mock/transport.ts`.
 * The `screen.ts` registrations are small and eager; the screens themselves are mostly lazy
 * (`load`), so each feature's components land in their own chunk.
 */
const found = import.meta.glob<{ default: ScreenModule }>('./*/screen.ts', { eager: true });

export const SCREENS: ScreenDef[] = Object.keys(found)
  .sort()
  .flatMap((path) => found[path]!.default);

const loaded = new Map<ScreenDef, Promise<ScreenComponent>>();

/** The screen's component: bundled ones at once, lazy ones imported once and cached. */
export function resolveScreen(screen: ScreenDef): ScreenComponent | Promise<ScreenComponent> {
  if (screen.component) return screen.component;
  let p = loaded.get(screen);
  if (!p) {
    p = screen.load().then((m) => m.default);
    // A failed import (e.g. a missing chunk) can be retried on the next open.
    p.catch(() => loaded.delete(screen));
    loaded.set(screen, p);
  }
  return p;
}

export type { ScreenComponent, ScreenCtx, ScreenDef } from './types';
