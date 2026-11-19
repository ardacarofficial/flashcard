import IoScreen from './IoScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'io', component: IoScreen, routes: ['/io'], deckPicker: true }] satisfies ScreenModule;
