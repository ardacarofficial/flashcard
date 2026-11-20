/**
 * Dev/test mock of the v0.10 mode modules: their switches and menu entries, and inline card
 * creation over the mock notes. Learn/test/timed/confidence sessions are the real engine's
 * (fc-app `modes`); component tests stub those calls.
 */
import { CLASSIC_NOTETYPE_ID } from '$lib/api/client';
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { InlineCardDto } from '$lib/api/types';
import { parseInline } from './inline';

export default {
  modules: [
    module('learn', [menu('modes.learn', 'nav.learn', '/learn', 6)]),
    module('practice_test', [menu('modes.test', 'nav.test', '/test', 7)]),
    module('confidence', []),
    module('timed', []),
    module('focus', []),
    module('inline_cards', []),
  ],
  handlers: ({ notes, id, now }: MockEnv) => {
    const preview = (text: string): InlineCardDto[] =>
      parseInline(text).map((c) => ({
        ...c,
        duplicate: notes.some((n) => (n.fields.front ?? '').toLowerCase() === c.front.toLowerCase()),
      }));
    return {
      'modes.inline.preview': ({ text }) => ({ cards: preview(text), truncated: false }),
      'modes.inline.create': ({ text, deck_id, tags }) => {
        const cards = preview(text);
        if (cards.length === 0) throw err('VALIDATION_INLINE_CARDS', 'errors.validation.inline_cards', text);
        const created = cards.filter((c) => !c.duplicate);
        const note_ids = created.map((c) => {
          const noteId = id('note');
          const t = now();
          notes.push({
            id: noteId,
            notetype_id: CLASSIC_NOTETYPE_ID,
            home_deck_id: deck_id,
            fields: { front: c.front, back: c.back },
            sort_field: c.front,
            tags,
            attributes: [],
            cards: [{ id: id('card'), deck_id, template_key: 'forward', state: 'new', due: null, suspended: false }],
            created_at: t,
            updated_at: t,
          });
          return noteId;
        });
        return { note_ids, duplicates: cards.length - created.length, truncated: false };
      },
    };
  },
} satisfies MockFeature;
