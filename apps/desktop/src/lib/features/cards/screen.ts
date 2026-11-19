import NoteTypesScreen from './NoteTypesScreen.svelte';
import type { ScreenModule } from '../types';

export default [{ id: 'notetypes', component: NoteTypesScreen, routes: ['/notetypes'] }] satisfies ScreenModule;
