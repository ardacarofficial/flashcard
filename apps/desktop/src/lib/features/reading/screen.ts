import ReadingScreen from './ReadingScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'reading', component: ReadingScreen, routes: ['/reading'], deckPicker: true }] satisfies ScreenModule;
