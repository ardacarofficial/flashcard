// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'inbox', load: () => import('./InboxScreen.svelte'), routes: ['/inbox'] },
  { id: 'local_api', load: () => import('./LocalApiScreen.svelte'), routes: ['/local-api'] },
] satisfies ScreenModule;
