import type { ScreenDef, ScreenModule } from './types';

/**
 * Every `features/<feature>/screen.ts` is picked up here: a new feature adds files, no
 * list to edit. Mock-backend handlers are collected the same way in `api/mock/transport.ts`.
 */
const found = import.meta.glob<{ default: ScreenModule }>('./*/screen.ts', { eager: true });

export const SCREENS: ScreenDef[] = Object.keys(found)
  .sort()
  .flatMap((path) => found[path]!.default);

export type { ScreenCtx, ScreenDef } from './types';
