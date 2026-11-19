import { err, menu, module, settingsPage, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { DeckDto } from '$lib/api/types';

export default {
  modules: [module('decks', [menu('decks.tree', 'nav.decks', '/decks', 10)])],
  handlers: ({ now, emit, decks, notes, pathOf, addDeck, tree, ancestors, requireDeck, requireName, sameName }: MockEnv) => ({
  'decks.tree': () => ({ roots: tree(null) }),
  'decks.create': ({ path, parent_id }) => {
    const parts = path.split('::').map(requireName);
    let parent: DeckDto | null = parent_id ? requireDeck(parent_id) : null;
    for (const [i, name] of parts.entries()) {
      const existing: DeckDto | undefined = decks.find(
        (d) => d.parent_id === (parent?.id ?? null) && sameName(d.name, name),
      );
      if (existing && i === parts.length - 1) {
        throw err('CONFLICT_DECK_EXISTS', 'errors.conflict.deck_exists', path);
      }
      parent = existing ?? addDeck(name, parent);
      if (!existing) emit({ type: 'deck_created', data: { deck: parent } });
    }
    return { ...parent!, path: pathOf(parent!) };
  },
  'decks.rename': ({ id: deckId, name }) => {
    const d = requireDeck(deckId);
    d.name = requireName(name);
    d.updated_at = now();
    const dto = { ...d, path: pathOf(d) };
    emit({ type: 'deck_updated', data: { deck: dto } });
    return dto;
  },
  'decks.delete': ({ id: target }) => {
    requireDeck(target);
    const doomed = new Set(decks.filter((d) => ancestors(d.id).includes(target)).map((d) => d.id));
    for (let i = decks.length - 1; i >= 0; i--) if (doomed.has(decks[i]!.id)) decks.splice(i, 1);
    let removed = 0;
    for (let i = notes.length - 1; i >= 0; i--) {
      if (doomed.has(notes[i]!.home_deck_id)) {
        notes.splice(i, 1);
        removed++;
      }
    }
    const affected = { decks: doomed.size, notes: removed, cards: removed };
    emit({ type: 'deck_deleted', data: { id: target, affected } });
    return affected;
  },
  }),
} satisfies MockFeature;
