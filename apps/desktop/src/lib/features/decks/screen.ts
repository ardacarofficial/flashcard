import DecksScreen from './DecksScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'decks', component: DecksScreen, routes: ['/decks'] }] satisfies ScreenModule;
