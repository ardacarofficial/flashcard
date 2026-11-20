/**
 * Dev-only mock of the `inbox`, `local_api`, `deep_links` and `clipboard` modules (see
 * `api/mock/transport.ts`). Minimal: the real rules live in fc-app.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ApiClientDto, InboxItemDto, InboxSource } from '$lib/api/types';

const SCOPES = [
  'inbox:write',
  'inbox:read',
  'decks:read',
  'decks:write',
  'notetypes:read',
  'notes:read',
  'notes:write',
  'attributes:read',
  'tags:read',
  'search:read',
];

export default {
  modules: [
    module('inbox', [menu('inbox.screen', 'nav.inbox', '/inbox', 25)]),
    // Off by default, like the real module (the server does not listen until switched on).
    { ...module('local_api', [menu('local_api.screen', 'nav.local_api', '/local-api', 82)]), enabled: false },
  ],
  handlers: ({ now, id, emit }: MockEnv) => {
    const items: InboxItemDto[] = [];
    // One browser extension asking to pair, so the screen has something to show.
    const t0 = now();
    const clients: ApiClientDto[] = [
      {
        id: id('client'),
        name: 'Flashcard for Chrome',
        kind: 'browser_extension',
        origin: 'chrome-extension://abcdefghijklmnop',
        requested_scopes: ['inbox:write', 'decks:read'],
        scopes: [],
        status: 'pending',
        created_at: t0,
        updated_at: t0,
        last_used_at: null,
      },
    ];
    const seed = (front: string, back: string, source: InboxSource, tags: string[]) => {
      const t = now();
      items.push({
        id: id('inbox'),
        source,
        status: 'pending',
        item: { front, back, fields: {}, tags, attributes: {}, via: null, client_ref: null },
        note_ids: [],
        created_at: t,
        updated_at: t,
      });
    };
    if (import.meta.env.MODE === 'mock') {
    seed('serendipity', 'tesadüfen güzel bir şey bulma', { kind: 'other', actor: 'Flashcard for Chrome', id: null }, ['web']);
    seed('ephemeral', 'kısa ömürlü', { kind: 'clipboard' }, []);
    seed('Mitokondri nedir?', 'Hücrenin enerji santrali', { kind: 'user' }, ['yks']);
    }
    const pending = () => items.filter((i) => i.status === 'pending').length;
    const changed = () => emit({ type: 'inbox_changed', data: { pending: pending() } });
    const find = (iid: string) => {
      const i = items.find((x) => x.id === iid);
      if (!i) throw err('NOT_FOUND_INBOX_ITEM', 'errors.not_found.inbox_item', iid);
      if (i.status !== 'pending') {
        throw err('CONFLICT_INBOX_ITEM_STATE', 'errors.conflict.inbox_item_state', iid);
      }
      return i;
    };
    const client = (cid: string) => {
      const c = clients.find((x) => x.id === cid);
      if (!c) throw err('NOT_FOUND_API_CLIENT', 'errors.not_found.api_client', cid);
      return c;
    };
    return {
      'inbox.add': (req) => {
        if (![req.front, req.back, ...Object.values(req.fields ?? {})].some((v) => v?.trim())) {
          throw err('VALIDATION_INBOX_ITEM', 'errors.validation.inbox_item', 'content');
        }
        const t = now();
        const source: InboxSource =
          req.via === 'clipboard'
            ? { kind: 'clipboard' }
            : req.via === 'deep_link'
              ? { kind: 'deep_link' }
              : { kind: 'user' };
        const item: InboxItemDto = {
          id: id('inbox'),
          source,
          status: 'pending',
          item: { ...req, via: null, client_ref: null },
          note_ids: [],
          created_at: t,
          updated_at: t,
        };
        items.unshift(item);
        changed();
        return item;
      },
      'inbox.list': ({ status }) => ({
        items: items.filter((i) => i.status === (status ?? 'pending')),
        next_cursor: null,
        pending: pending(),
      }),
      'inbox.count': () => ({ pending: pending() }),
      'inbox.accept': ({ items: picked }) => {
        const all = picked.map((p, index) => {
          const i = find(p.id);
          if (!(p.deck_id ?? i.item.deck_id)) {
            const e = err('VALIDATION_INBOX_ITEM', 'errors.validation.inbox_item', 'deck');
            e.details = { field: 'deck', item_index: index };
            throw e;
          }
          return i;
        });
        const accepted = all.map((i) => {
          const note_id = id('note');
          Object.assign(i, { status: 'accepted', note_ids: [note_id], updated_at: now() });
          return { id: i.id, note_id };
        });
        changed();
        return { accepted };
      },
      'inbox.reject': ({ ids }) => {
        const all = ids.map(find);
        for (const i of all) Object.assign(i, { status: 'rejected', updated_at: now() });
        changed();
        return { changed: all.length };
      },
      'clipboard.check': ({ text }) => {
        const v = text.trim();
        return v.length >= 2 ? { suggest: true, text: v, reason: null } : { suggest: false, text: '', reason: 'too_short' };
      },
      'local_api.clients.list': () => ({ clients: [...clients], grantable_scopes: SCOPES, port: 28735 }),
      'local_api.methods': () => ({ methods: [] }),
      'local_api.clients.approve': ({ id: cid, scopes }) => {
        const c = client(cid);
        if (c.status !== 'pending') {
          throw err('CONFLICT_API_CLIENT_STATE', 'errors.conflict.api_client_state', cid);
        }
        Object.assign(c, { status: 'approved', scopes: scopes ?? c.requested_scopes, updated_at: now() });
        emit({ type: 'local_api_clients_changed' });
        return c;
      },
      'local_api.clients.deny': ({ id: cid }) => {
        const c = client(cid);
        Object.assign(c, { status: 'denied', scopes: [], updated_at: now() });
        emit({ type: 'local_api_clients_changed' });
        return c;
      },
      'local_api.clients.revoke': ({ id: cid }) => {
        const c = client(cid);
        Object.assign(c, { status: 'revoked', scopes: [], updated_at: now() });
        emit({ type: 'local_api_clients_changed' });
        return c;
      },
      'local_api.clients.remove': ({ id: cid }) => {
        const c = client(cid);
        clients.splice(clients.indexOf(c), 1);
        emit({ type: 'local_api_clients_changed' });
        return c;
      },
    };
  },
} satisfies MockFeature;
