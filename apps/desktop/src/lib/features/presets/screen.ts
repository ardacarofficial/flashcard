// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  // The full editor of settings packages (v1.4: Advanced › Studying).
  { id: 'presets', load: () => import('./PresetsScreen.svelte'), routes: ['/presets'] },
  // v1.4 Basic › "Daily pace": the everyday questions; writes through `planning.apply_pace`,
  // so it is there while the planning module is on.
  {
    id: 'settings_pace',
    load: () => import('./PaceSettings.svelte'),
    settingsPage: { route: '/settings/pace', label_key: 'shell.page.pace', modules: ['planning'] },
  },
] satisfies ScreenModule;
