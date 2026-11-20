import InboxScreen from './InboxScreen.svelte';
import LocalApiScreen from './LocalApiScreen.svelte';
import type { ScreenModule } from '../types';

export default [
  { id: 'inbox', component: InboxScreen, routes: ['/inbox'] },
  { id: 'local_api', component: LocalApiScreen, routes: ['/local-api'] },
] satisfies ScreenModule;
