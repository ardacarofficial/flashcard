import AddNoteScreen from './AddNoteScreen.svelte';
import NoteListScreen from './NoteListScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'add_note', component: AddNoteScreen, routes: ['/notes/new'], deckPicker: true },
  { id: 'notes', component: NoteListScreen, routes: ['/notes'], deckPicker: true },
] satisfies ScreenModule;
