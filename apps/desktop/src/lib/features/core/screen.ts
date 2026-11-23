// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  {
    id: 'settings',
    load: () => import('./SettingsPage.svelte'),
    slot: { name: 'settings_page', label_key: 'nav.settings', order: Infinity },
  },
  // v1.4: pages of rows from several modules' settings (shown while one of them is on).
  {
    id: 'settings_words',
    load: () => import('./WordsSettings.svelte'),
    settingsPage: { route: '/settings/words', label_key: 'shell.page.words', modules: ['words', 'word_data'] },
  },
  {
    id: 'settings_adding',
    load: () => import('./AddingSettings.svelte'),
    settingsPage: { route: '/settings/adding', label_key: 'shell.page.adding', modules: ['quick_add', 'clipboard', 'attributes'] },
  },
] satisfies ScreenModule;
