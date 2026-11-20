// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'stats', load: () => import('./StatsScreen.svelte'), routes: ['/stats'] },
  { id: 'planning', load: () => import('./PlanningScreen.svelte'), routes: ['/planning'] },
  { id: 'goals', load: () => import('./GoalsScreen.svelte'), routes: ['/goals'] },
] satisfies ScreenModule;
