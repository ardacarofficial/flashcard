// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'subtitle', load: () => import('./SubtitleScreen.svelte'), routes: ['/subtitle'] }] satisfies ScreenModule;
