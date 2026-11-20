// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'add_note', load: () => import('./CreateScreen.svelte'), routes: ['/notes/new'] },
  { id: 'notes', load: () => import('./NoteListScreen.svelte'), routes: ['/notes'], deckPicker: true },
] satisfies ScreenModule;
