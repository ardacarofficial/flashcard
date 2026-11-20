import LearnScreen from './LearnScreen.svelte';
import TestScreen from './TestScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'learn', component: LearnScreen, routes: ['/learn'], deckPicker: true },
  { id: 'test', component: TestScreen, routes: ['/test'], deckPicker: true },
] satisfies ScreenModule;
