/**
 * What the app shell adds to a screen's header (v1.1 polish): the deck picker for screens
 * that work on one deck (shown with the header's actions, on the right) and the section's
 * tabs (under the title). The shell provides it with `setPageChrome`; `PageHeader` draws it,
 * so a page always starts with its title.
 */
import { getContext, setContext, type Snippet } from 'svelte';

export interface PageChrome {
  /** Header actions from the shell (the deck picker), drawn before the page's own. */
  readonly actions: Snippet | null;
  /** The section's tabs, drawn under the title. */
  readonly tabs: Snippet | null;
}

const KEY = Symbol('fc.page-chrome');

export function setPageChrome(chrome: PageChrome): void {
  setContext(KEY, chrome);
}

export function getPageChrome(): PageChrome | undefined {
  return getContext<PageChrome | undefined>(KEY);
}
