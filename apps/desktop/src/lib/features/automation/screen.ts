import AutomationScreen from './AutomationScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'automation', component: AutomationScreen, routes: ['/automation'] }] satisfies ScreenModule;
