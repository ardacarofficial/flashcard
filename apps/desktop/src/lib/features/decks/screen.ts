// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'decks', load: () => import('./LibraryScreen.svelte'), routes: ['/decks'] }] satisfies ScreenModule;
