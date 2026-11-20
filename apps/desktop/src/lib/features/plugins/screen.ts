import PluginsScreen from './PluginsScreen.svelte';
import PluginViewScreen from './PluginViewScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'plugins', component: PluginsScreen, routes: ['/plugins'] },
  // Views and study modes contributed by plugins: `/plugins/view/<plugin id>/<view>`.
  { id: 'plugin-view', component: PluginViewScreen, routes: ['/plugins/view/*'], deckPicker: true },
] satisfies ScreenModule;
