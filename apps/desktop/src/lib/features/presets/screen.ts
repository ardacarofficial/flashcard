import PresetsScreen from './PresetsScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'presets', component: PresetsScreen, routes: ['/presets'] }] satisfies ScreenModule;
