import GoalsScreen from './GoalsScreen.svelte';
import PlanningScreen from './PlanningScreen.svelte';
import StatsScreen from './StatsScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'stats', component: StatsScreen, routes: ['/stats'] },
  { id: 'planning', component: PlanningScreen, routes: ['/planning'] },
  { id: 'goals', component: GoalsScreen, routes: ['/goals'] },
] satisfies ScreenModule;
