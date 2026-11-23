// Kelimelerim (v1.4): the word index as a page; loads on first open.
import type { ScreenModule } from '../types';

export default [
  { id: 'words', load: () => import('./WordsScreen.svelte'), routes: ['/words'], width: 'wide' },
] satisfies ScreenModule;
