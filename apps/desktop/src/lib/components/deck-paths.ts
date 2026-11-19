import type { DeckNode } from '$lib/api/types';

export interface DeckPath {
  id: string;
  path: string;
}

/** Flatten the tree into "Parent::Child" display paths, depth-first. */
export function deckPaths(nodes: DeckNode[], prefix = ''): DeckPath[] {
  return nodes.flatMap((n) => {
    const path = prefix ? `${prefix}::${n.name}` : n.name;
    return [{ id: n.id, path }, ...deckPaths(n.children, path)];
  });
}
