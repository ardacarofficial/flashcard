import type { DeckTreeNode } from '$lib/api/types';

export interface DeckPath {
  id: string;
  path: string;
}

/** Flatten the tree into "Parent::Child" display paths (the backend's `path`), depth-first. */
export function deckPaths(nodes: DeckTreeNode[]): DeckPath[] {
  return nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...deckPaths(n.children)]);
}
