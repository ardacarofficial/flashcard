// Screens load on first open (v1.0 code splitting): each is its own chunk. Since v1.1 the
// shell opens on its own Home page, so study is lazy too.
import type { ScreenModule } from '../types';

export default [{ id: 'study', load: () => import('./StudyScreen.svelte'), routes: ['/study'], deckPicker: 'all' }] satisfies ScreenModule;
