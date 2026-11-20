// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [{ id: 'app_maintenance', load: () => import('./MaintenanceScreen.svelte'), routes: ['/app-maintenance'] }] satisfies ScreenModule;
