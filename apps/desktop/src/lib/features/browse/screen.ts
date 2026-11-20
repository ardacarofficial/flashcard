import BrowseScreen from './BrowseScreen.svelte';
import MaintenanceScreen from './MaintenanceScreen.svelte';
import ShortcutsScreen from './ShortcutsScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'browse', component: BrowseScreen, routes: ['/browse'] },
  { id: 'maintenance', component: MaintenanceScreen, routes: ['/maintenance'] },
  { id: 'shortcuts', component: ShortcutsScreen, routes: ['/shortcuts'] },
] satisfies ScreenModule;
