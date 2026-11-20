import AiScreen from './AiScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'ai', component: AiScreen, routes: ['/ai'], deckPicker: true }] satisfies ScreenModule;
