// Screens load on first open (v1.0 code splitting): each is its own chunk.
import type { ScreenModule } from '../types';

export default [
  { id: 'plugins', load: () => import('./PluginsScreen.svelte'), routes: ['/plugins'] },
  // Views and study modes contributed by plugins: `/plugins/view/<plugin id>/<view>`.
  { id: 'plugin-view', load: () => import('./PluginViewScreen.svelte'), routes: ['/plugins/view/*'], deckPicker: true },
] satisfies ScreenModule;
