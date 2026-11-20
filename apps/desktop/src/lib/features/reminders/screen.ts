// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'reminders', load: () => import('./RemindersScreen.svelte'), routes: ['/reminders'] }] satisfies ScreenModule;
