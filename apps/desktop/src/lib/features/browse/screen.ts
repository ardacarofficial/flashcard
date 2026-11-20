// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'browse', load: () => import('./BrowseScreen.svelte'), routes: ['/browse'], width: 'wide' },
  { id: 'maintenance', load: () => import('./MaintenanceScreen.svelte'), routes: ['/maintenance'] },
  { id: 'shortcuts', load: () => import('./ShortcutsScreen.svelte'), routes: ['/shortcuts'] },
] satisfies ScreenModule;
