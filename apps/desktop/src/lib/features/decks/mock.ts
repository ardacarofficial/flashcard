import { err, menu, module, settingsPage, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import { deckCounts } from '$lib/api/mock/seed';
import type { DeckCounts, DeckDto, DeckSummary, Mastery, NoteTerm } from '$lib/api/types';

export default {
  // v1.4: the word index (`words.*`, module and handlers) is mocked in words/mock.ts.
  modules: [module('decks', [menu('decks.tree', 'nav.decks', '/decks', 10)])],
  handlers: ({ now, emit, decks, notes, attrs, cardFlags, pathOf, addDeck, tree, ancestors, requireDeck, requireName, sameName }: MockEnv) => ({
  'decks.tree': () => ({ roots: tree(null) }),
  // Plausible, fixed progress per deck (the real numbers come from fc-app).
  'decks.summaries': () => {
    const roots = tree(null);
    const flat = (ns: typeof roots): typeof roots => ns.flatMap((n) => [n, ...flat(n.children)]);
    const nodes = flat(roots);
    const sum = { new: 0, learning: 0, due: 0, total: 0 };
    for (const r of roots) {
      sum.new += r.counts.new;
      sum.learning += r.counts.learning;
      sum.due += r.counts.due;
      sum.total += r.counts.total;
    }
    const today: DeckCounts = deckCounts(sum);
    const items: DeckSummary[] = nodes.map((n, i) => {
      const total = n.counts.total;
      const mastered = Math.round(total * [0.62, 0.35, 0.48, 0.8, 0.22, 0.4, 0.55, 0.3, 0.05, 0.7][i % 10]!);
      const learning = Math.min(total - mastered, n.counts.learning + n.counts.due);
      const own = notes.filter((x) => ancestors(x.home_deck_id).includes(n.deck.id));
      const counts = new Map<string, number>();
      for (const note of own) {
        for (const a of note.attributes) {
          const def = attrs.find((d) => d.id === a.attr_id);
          const v = a.value;
          if (!def || v?.type !== 'choice') continue;
          const label = def.options.find((o) => o.id === v.value)?.label;
          if (label) counts.set(`${def.id}|${def.name}|${label}`, (counts.get(`${def.id}|${def.name}|${label}`) ?? 0) + 1);
        }
      }
      const chips = [...counts.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([k, c]) => {
          const [attr_id, name, label] = k.split('|') as [string, string, string];
          return { attr_id, name, label, notes: c };
        });
      return {
        deck_id: n.deck.id,
        new: total - mastered - learning,
        learning,
        mastered,
        mastered_pct: total ? Math.floor((mastered * 100) / total) : 0,
        last_studied_at: total && n.counts.due ? 1_700_000_000_000 - i * 3_600_000 : null,
        chips,
      };
    });
    const recent = nodes.filter((n) => n.children.length === 0 && n.counts.due > 0).slice(0, 6).map((n) => n.deck.id);
    return { today, decks: items, recent };
  },
  'notes.terms': ({ deck_id, include_subdecks, page }) => {
    const mine = notes.filter((n) =>
      !deck_id || (include_subdecks === false ? n.home_deck_id === deck_id : ancestors(n.home_deck_id).includes(deck_id)),
    );
    const start = Number(page.cursor ?? 0);
    const limit = page.limit ?? 50;
    const slice = mine.slice(start, start + limit);
    const LEVELS: Mastery[] = ['mastered', 'learning', 'new', 'mastered', 'learning'];
    const items: NoteTerm[] = slice.map((n, i) => ({
      note_id: n.id,
      notetype_id: n.notetype_id,
      card_ids: n.cards.map((c) => c.id),
      flags: n.cards.reduce((f, c) => f & (cardFlags.get(c.id) ?? 0), 0x7f),
      mastery: LEVELS[(start + i) % LEVELS.length]!,
      front: { key: 'front', value: n.fields.front ?? '', text: n.fields.front ?? '' },
      back: { key: 'back', value: n.fields.back ?? '', text: n.fields.back ?? '' },
      term_field: 'front',
      definition_field: 'back',
      ambiguous: false,
      chips: n.attributes.flatMap((a) => {
        const def = attrs.find((d) => d.id === a.attr_id);
        const v = a.value;
        const label = v?.type === 'choice' ? def?.options.find((o) => o.id === v.value)?.label : undefined;
        return def && label ? [{ attr_id: def.id, name: def.name, label }] : [];
      }),
    }));
    return { items, next_cursor: start + limit < mine.length ? String(start + limit) : null };
  },
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
