// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  {
    id: 'settings',
    load: () => import('./SettingsPage.svelte'),
    slot: { name: 'settings_page', label_key: 'nav.settings', order: Infinity },
  },
] satisfies ScreenModule;
