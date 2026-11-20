// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'reading', load: () => import('./ReadingScreen.svelte'), routes: ['/reading'], deckPicker: true }] satisfies ScreenModule;
