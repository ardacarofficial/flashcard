import MaintenanceScreen from './MaintenanceScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'app_maintenance', component: MaintenanceScreen, routes: ['/app-maintenance'] }] satisfies ScreenModule;
