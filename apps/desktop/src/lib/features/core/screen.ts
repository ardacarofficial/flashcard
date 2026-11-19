import SettingsPage from './SettingsPage.svelte';
import type { ScreenModule } from '../types';

export default [
  {
    id: 'settings',
    component: SettingsPage,
    slot: { name: 'settings_page', label_key: 'nav.settings', order: Infinity },
  },
] satisfies ScreenModule;
