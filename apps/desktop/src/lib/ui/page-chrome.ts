/**
 * What the app shell adds to a screen's header (v1.1 polish) and what a screen tells the
 * shell (v1.2): the deck picker for screens that work on one deck (shown with the header's
 * actions, on the right), the section's tabs (under the title), and the top bar's trail.
 * The shell provides it with `setPageChrome`; `PageHeader` draws the first two, so a page
 * always starts with its title.
 *
 * Top bar trail: the shell shows the place (section › page) on its own. A screen that opens a
 * sub-page (a deck, a card) appends crumbs with `getPageChrome()?.setTrail([...], up)`, e.g.
 * `$effect(() => chrome?.setTrail(deck ? [{ label: deck.name }] : null, closeDeck))`; `up`
 * turns the page's own crumb into a link back. The shell clears them on every page change.
 * `PageHeader` does this by itself for a header with `back` (its title, `back.onclick`).
 */
import { getContext, setContext, type Snippet } from 'svelte';
import type { Crumb } from './types';

export interface PageChrome {
  /** Header actions from the shell (the deck picker), drawn before the page's own. */
  readonly actions: Snippet | null;
  /** The section's tabs, drawn under the title. */
  readonly tabs: Snippet | null;
  /**
   * Crumbs after the shell's own (section › page) in the top bar; `null` clears them. `up`
   * makes the page crumb a link (back from the sub-page). A crumb with `onclick` is a link.
   */
  setTrail(crumbs: Crumb[] | null, up?: () => void): void;
}

const KEY = Symbol('fc.page-chrome');

export function setPageChrome(chrome: PageChrome): void {
  setContext(KEY, chrome);
}

export function getPageChrome(): PageChrome | undefined {
  return getContext<PageChrome | undefined>(KEY);
}
