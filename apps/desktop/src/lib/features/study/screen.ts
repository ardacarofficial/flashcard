import StudyScreen from './StudyScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'study', component: StudyScreen, routes: ['/study'], deckPicker: true }] satisfies ScreenModule;
