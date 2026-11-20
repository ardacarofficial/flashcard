// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'learn', load: () => import('./LearnScreen.svelte'), routes: ['/learn'], deckPicker: true },
  { id: 'test', load: () => import('./TestScreen.svelte'), routes: ['/test'], deckPicker: true },
] satisfies ScreenModule;
