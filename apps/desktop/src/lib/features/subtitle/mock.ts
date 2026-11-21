/**
 * Dev-only mock of the `subtitle` module (see `api/mock/transport.ts`). Validation is minimal:
 * the real rules live in fc-app `modules/subtitle/` (keys hashed, captures as notes).
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { SubtitleKeyDto, SubtitleStatus } from '$lib/api/types';

const PORT = 28735;

export default {
  modules: [{ ...module('subtitle', [menu('subtitle.screen', 'nav.subtitle', '/subtitle', 83)]), enabled: false }],
  handlers: ({ now, id, emit, decks }: MockEnv) => {
    const keys: SubtitleKeyDto[] = [];
    let defaultDeck: string | null = null;
    const status = (): SubtitleStatus => {
      const deck = decks.find((d) => d.id === defaultDeck) ?? null;
      return {
        address: `http://127.0.0.1:${PORT}/subtitle`,
        port: PORT,
        default_deck_id: deck?.id ?? null,
        default_deck_path: deck?.path ?? null,
        fallback_deck: 'Subtitle',
        captures: 0,
        last_capture_at: null,
        active_keys: keys.filter((k) => k.revoked_at === null).length,
      };
    };
    return {
      'subtitle.keys.list': () => ({ keys: [...keys].reverse() }),
      'subtitle.keys.create': ({ name }) => {
        const n = name.trim();
        if (!n || n.length > 60) throw err('VALIDATION_SUBTITLE_KEY', 'errors.validation.subtitle_key', name);
        const secret = `sk_fc_${id('mock').replace(/[^a-z0-9]/g, '')}${'x'.repeat(30)}`;
        const key: SubtitleKeyDto = {
          id: id('subkey'),
          name: n,
          prefix: secret.slice(0, 10),
          created_at: now(),
          last_used_at: null,
          revoked_at: null,
        };
        keys.push(key);
        emit({ type: 'subtitle_changed' });
        return { key, secret };
      },
      'subtitle.keys.revoke': ({ id: kid }) => {
        const k = keys.find((x) => x.id === kid);
        if (!k) throw err('NOT_FOUND_SUBTITLE_KEY', 'errors.not_found.subtitle_key', kid);
        if (k.revoked_at !== null) throw err('CONFLICT_SUBTITLE_KEY_STATE', 'errors.conflict.subtitle_key_state', kid);
        k.revoked_at = now();
        emit({ type: 'subtitle_changed' });
        return k;
      },
      'subtitle.status': () => status(),
      'subtitle.default_deck.set': ({ deck_id }) => {
        if (deck_id && !decks.some((d) => d.id === deck_id)) throw err('NOT_FOUND_DECK', 'errors.not_found.deck', deck_id);
        defaultDeck = deck_id ?? null;
        emit({ type: 'subtitle_changed' });
        return status();
      },
    };
  },
} satisfies MockFeature;
