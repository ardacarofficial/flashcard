// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'starters', load: () => import('./StartersScreen.svelte'), routes: ['/starters'] }] satisfies ScreenModule;
