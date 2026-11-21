// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'learn', load: () => import('./LearnScreen.svelte'), routes: ['/learn'], deckPicker: 'all' },
  { id: 'test', load: () => import('./TestScreen.svelte'), routes: ['/test'], deckPicker: 'all' },
  { id: 'match', load: () => import('./MatchScreen.svelte'), routes: ['/match'], deckPicker: 'all' },
  // Settings → "Reading and modes" (navigation.ts).
  { id: 'mode-settings', load: () => import('./ModeSettingsScreen.svelte'), routes: ['/mode-settings'] },
] satisfies ScreenModule;
