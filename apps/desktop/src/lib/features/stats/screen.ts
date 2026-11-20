// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'stats', load: () => import('./StatsScreen.svelte'), routes: ['/stats'], width: 'wide' },
  { id: 'planning', load: () => import('./PlanningScreen.svelte'), routes: ['/planning'], width: 'wide' },
  { id: 'goals', load: () => import('./GoalsScreen.svelte'), routes: ['/goals'], width: 'wide' },
] satisfies ScreenModule;
