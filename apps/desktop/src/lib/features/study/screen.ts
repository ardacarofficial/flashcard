// Bundled with the shell: study is the first screen the menu opens (order 5). Every other
// feature screen is lazy (`load`, see features/types.ts).
import StudyScreen from './StudyScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'study', component: StudyScreen, routes: ['/study'], deckPicker: true }] satisfies ScreenModule;
