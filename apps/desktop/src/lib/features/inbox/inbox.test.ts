/**
 * v0.9 inbox UI on the dev mock: review proposed cards (edit, pick a deck, accept / reject),
 * approve a program asking to connect to the local API, the clipboard suggestion, flashcard://
 * links routing, and the quick-add window.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import type { DeckTreeNode, EventEnvelope } from '$lib/api/types';
import InboxScreen from './InboxScreen.svelte';
import LocalApiScreen from './LocalApiScreen.svelte';
import OutsideHost from './OutsideHost.svelte';
import QuickAdd from './QuickAdd.svelte';
import { linkTarget, type LinkResult, type Shell } from './shell';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

function ctxWith(decks: DeckTreeNode[] = []): ScreenCtx {
  return {
    decks,
    selectedDeck: null,
    selectedPath: null,
    selectDeck: () => {},
    guarded: async (fn) => {
      try {
        await fn();
        return true;
      } catch {
        return false;
      }
    },
  };
}

/** A fake shell: tests push links / clipboard text through it. */
function fakeShell(links: LinkResult[] = [], prefill: string | null = null) {
  const listeners = new Map<string, (p: unknown) => void>();
  const shell: Shell = {
    takeDeepLinks: async () => links.splice(0),
    heldDeepLinks: async () => [],
    discardHeldDeepLinks: async () => {},
    prefill: async () => prefill,
    listen: async (event, cb) => {
      listeners.set(event, cb);
      return () => listeners.delete(event);
    },
    hide: vi.fn(async () => {}),
  };
  return { shell, fire: (event: string, payload: unknown) => listeners.get(event)?.(payload) };
}

async function deckNode(name: string): Promise<DeckTreeNode> {
  await api.deck.create(name);
  const tree = await api.deck.tree();
  return tree.roots.find((n) => n.deck.name === name)!;
}

describe('<InboxScreen>', () => {
  it('reviews proposals: edit, choose a deck, accept; reject; tabs', async () => {
    const node = await deckNode('Inbox deck');
    await api.call('inbox.add', {
      front: 'badger',
      back: 'porsuk',
      fields: {},
      tags: ['web'],
      attributes: { Seviye: 'B1' },
      context: { url: 'https://example.com/a', title: 'Badgers', sentence: 'The badger dug.' },
    });
    await api.call('inbox.add', { front: 'otter', fields: {}, tags: [], attributes: {} });
    render(InboxScreen, { ctx: ctxWith([node]) });
    // v1.2: rows show a preview; Edit (or E on the row) opens the fields.
    expect(await screen.findByText('badger')).toBeTruthy();
    expect(screen.getByText('Seviye: B1')).toBeTruthy();
    expect(screen.getByText('#web')).toBeTruthy();
    expect(screen.getByText('Sentence: “The badger dug.”')).toBeTruthy();
    expect(screen.getByText('From: Badgers')).toBeTruthy();

    // No deck yet: nothing is sent (v1.3: the row asks first instead of failing); the row's
    // deck picker says why and takes the focus.
    const accepts = vi.spyOn(api, 'call');
    const accept = screen.getAllByRole('button', { name: 'Accept' });
    await fireEvent.click(accept[0]!);
    expect(await screen.findByText('Choose a deck first.')).toBeTruthy();
    await waitFor(() => expect(document.activeElement).toBe(screen.getAllByRole('combobox', { name: 'Deck' })[0]));
    expect((await api.call('inbox.count', {})).pending).toBe(2);
    expect(accepts.mock.calls.some(([m]) => m === 'inbox.accept')).toBe(false);
    accepts.mockRestore();

    // Pick the deck, edit the back, accept.
    // v1.1: the deck is the design-system Select (combobox + listbox).
    await fireEvent.click(screen.getAllByRole('combobox', { name: 'Deck' })[1]!);
    await fireEvent.click(await screen.findByRole('option', { name: 'Inbox deck' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Edit' })[1]!);
    await fireEvent.input(await screen.findByDisplayValue('porsuk'), { target: { value: 'porsuk (hayvan)' } });
    const spy = vi.spyOn(api, 'call');
    await fireEvent.click(screen.getAllByRole('button', { name: 'Accept' })[1]!);
    expect(await screen.findByText('1 card added')).toBeTruthy();
    const req = spy.mock.calls.find(([m]) => m === 'inbox.accept')![1] as { items: { deck_id: string; fields: unknown }[] };
    expect(req.items[0]!.deck_id).toBe(node.deck.id);
    expect(req.items[0]!.fields).toEqual({ front: 'badger', back: 'porsuk (hayvan)' });
    spy.mockRestore();

    // Reject the other one; it moves to the Rejected tab.
    await waitFor(() => expect(screen.queryByText('badger')).toBeNull());
    await fireEvent.click(screen.getByRole('button', { name: 'Reject' }));
    expect(await screen.findByText('Nothing waiting.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('tab', { name: 'Rejected' }));
    expect(await screen.findByText('otter', { exact: false })).toBeTruthy();
    await fireEvent.click(screen.getByRole('tab', { name: 'Accepted' }));
    expect(await screen.findByText('badger')).toBeTruthy();
  });

  it('J from a plain button outside the list (the sidebar item) enters the list; R keeps the focus in it', async () => {
    await api.call('inbox.add', { front: 'kestrel', fields: {}, tags: [], attributes: {} });
    await api.call('inbox.add', { front: 'heron', fields: {}, tags: [], attributes: {} });
    const outside = document.createElement('button');
    document.body.append(outside);
    render(InboxScreen, { ctx: ctxWith([]) });
    expect(await screen.findByText('kestrel')).toBeTruthy();
    outside.focus();
    await fireEvent.keyDown(outside, { key: 'j' });
    const first = await waitFor(() => {
      const row = document.activeElement as HTMLElement;
      expect(row.getAttribute('data-row')).toBe('0');
      return row;
    });
    const [gone, kept] = first.textContent?.includes('kestrel') ? ['kestrel', 'heron'] : ['heron', 'kestrel'];
    await fireEvent.keyDown(first, { key: 'r' });
    await waitFor(() => expect(screen.queryByText(gone)).toBeNull());
    await waitFor(() => expect((document.activeElement as HTMLElement).getAttribute('data-row')).toBe('0'));
    expect(document.activeElement?.textContent).toContain(kept);
    outside.remove();
  });
});

describe('<LocalApiScreen>', () => {
  it('approves a pending program with fewer permissions, then revokes it', async () => {
    await api.call('modules.set_enabled', { id: 'local_api', enabled: true });
    render(LocalApiScreen, { ctx: ctxWith() });
    expect(await screen.findByText('Flashcard for Chrome')).toBeTruthy();
    expect(screen.getByText(/127\.0\.0\.1:28735/)).toBeTruthy();
    expect(screen.getByText('Waiting for you')).toBeTruthy();
    // Untick "See your decks": only the inbox permission is granted.
    await fireEvent.click(screen.getByLabelText('See your decks'));
    await fireEvent.click(screen.getByRole('button', { name: 'Allow' }));
    expect(await screen.findByText('Allowed')).toBeTruthy();
    expect(screen.getByText('Permissions: Propose cards to the inbox')).toBeTruthy();
    const [c] = (await api.call('local_api.clients.list', {})).clients;
    expect(c!.scopes).toEqual(['inbox:write']);
    await fireEvent.click(screen.getByRole('button', { name: 'Revoke' }));
    expect(await screen.findByText('Revoked')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(await screen.findByText('No programs yet.')).toBeTruthy();
    await api.call('modules.set_enabled', { id: 'local_api', enabled: false });
  });
});

function pairingEvent(): EventEnvelope {
  return {
    seq: 1,
    at: 0,
    batch_id: null,
    event: {
      type: 'local_api_pairing',
      data: {
        client: {
          id: 'c1',
          name: 'Flashcard for Chrome',
          kind: 'browser_extension',
          origin: 'chrome-extension://abc',
          requested_scopes: ['inbox:write', 'decks:read'],
          scopes: [],
          status: 'pending',
          created_at: 0,
          updated_at: 0,
          last_used_at: null,
        },
      },
    },
  } as EventEnvelope;
}

async function openPairing() {
  const on = vi.spyOn(api, 'on');
  const call = vi.spyOn(api, 'call').mockResolvedValue({} as never);
  render(OutsideHost, { open: () => {}, selectDeck: () => {}, shell: fakeShell().shell });
  await waitFor(() => expect(on).toHaveBeenCalled());
  on.mock.calls.find(([p]) => p === 'local_api_pairing')![1](pairingEvent());
  expect(await screen.findByText('Flashcard for Chrome wants to connect to Flashcard.')).toBeTruthy();
  return {
    call,
    done: () => {
      on.mockRestore();
      call.mockRestore();
    },
  };
}

describe('<OutsideHost>', () => {
  it('asks the user when a program wants to connect, everything ticked to begin with', async () => {
    const { call, done } = await openPairing();
    expect(screen.getByLabelText('Propose cards to the inbox')).toBeTruthy();
    expect(screen.getByLabelText('See your decks')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Allow' }));
    expect(call).toHaveBeenCalledWith('local_api.clients.approve', { id: 'c1', scopes: ['inbox:write', 'decks:read'] });
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    done();
  });

  it('lets the user narrow the permissions before allowing, and never allows none', async () => {
    const { call, done } = await openPairing();
    await fireEvent.click(screen.getByLabelText('See your decks'));
    await fireEvent.click(screen.getByRole('button', { name: 'Allow' }));
    expect(call).toHaveBeenCalledWith('local_api.clients.approve', { id: 'c1', scopes: ['inbox:write'] });
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    done();
  });

  it('keeps Allow off while nothing is ticked, and refusing sends no permissions', async () => {
    const { call, done } = await openPairing();
    await fireEvent.click(screen.getByLabelText('See your decks'));
    await fireEvent.click(screen.getByLabelText('Propose cards to the inbox'));
    expect((screen.getByRole('button', { name: 'Allow' }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Refuse' }));
    expect(call).toHaveBeenCalledWith('local_api.clients.deny', { id: 'c1' });
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    done();
  });

  it('offers copied text and adds it to the inbox only when asked', async () => {
    const { shell, fire } = fakeShell();
    render(OutsideHost, { open: () => {}, selectDeck: () => {}, shell });
    await waitFor(() => fire('fc://clipboard', { text: 'porsuk' }));
    expect(await screen.findByText('Add “porsuk” as a card?')).toBeTruthy();
    const before = (await api.call('inbox.count', {})).pending;
    await fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect((await api.call('inbox.count', {})).pending).toBe(before);
    fire('fc://clipboard', { text: 'porsuk' });
    await fireEvent.click(await screen.findByRole('button', { name: 'Add to inbox' }));
    expect(await screen.findByText('Added to the inbox')).toBeTruthy();
    const list = await api.call('inbox.list', { status: null, page: { cursor: null, limit: null } });
    expect(list.items[0]!.source).toEqual({ kind: 'clipboard' });
    expect(list.pending).toBe(before + 1);
  });

  it('routes links handled before it listened (cold start) and later ones', async () => {
    const opened: string[] = [];
    const selected: (string | null)[] = [];
    const { shell, fire } = fakeShell([
      { url: 'flashcard://deck/d1', ok: { kind: 'navigate', target: { kind: 'deck', deck_id: 'd1' } } },
    ]);
    render(OutsideHost, { open: (r: string) => opened.push(r), selectDeck: (d: string | null) => selected.push(d), shell });
    await waitFor(() => expect(opened).toEqual(['/decks']));
    expect(selected).toEqual(['d1']);
    const links: LinkResult[] = [
      {
        url: 'flashcard://wipe',
        error: { code: 'VALIDATION_DEEP_LINK', message_key: 'errors.validation.deep_link', message: '', details: {}, retryable: false },
      },
    ];
    shell.takeDeepLinks = async () => links.splice(0);
    fire('fc://deep-link', null);
    expect(await screen.findByText('Could not open the link. This link cannot be opened.')).toBeTruthy();
  });

  it('maps link outcomes to screens', () => {
    expect(linkTarget({ kind: 'navigate', target: { kind: 'note', note_id: 'n' } })).toEqual({ route: '/notes', deck: null, proposed: false });
    expect(linkTarget({ kind: 'navigate', target: { kind: 'study', deck_id: 'd', query: null } })).toEqual({ route: '/study', deck: 'd', proposed: false });
    expect(linkTarget({ kind: 'navigate', target: { kind: 'study', deck_id: null, query: 'is:due' } }).route).toBe('/browse');
    expect(linkTarget({ kind: 'inbox', item: {} as never })).toEqual({ route: '/inbox', deck: null, proposed: true });
  });
});

describe('<QuickAdd>', () => {
  it('prefills from the clipboard, adds a note, hides on Escape', async () => {
    const node = await deckNode('Quick deck');
    const { shell } = fakeShell([], 'porsuk');
    render(QuickAdd, { shell });
    expect(await screen.findByDisplayValue('porsuk')).toBeTruthy();
    await fireEvent.click(screen.getByRole('combobox', { name: 'Deck' }));
    await fireEvent.click(await screen.findByRole('option', { name: node.deck.name }));
    await fireEvent.input(screen.getByLabelText('Back'), { target: { value: 'badger' } });
    const spy = vi.spyOn(api.note, 'createClassic');
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Card added')).toBeTruthy();
    expect(spy).toHaveBeenCalledWith(node.deck.id, 'porsuk', 'badger', []);
    // Ctrl+Enter adds from a field too, and the front field is ready for the next card.
    const front = screen.getByLabelText('Front');
    await fireEvent.input(front, { target: { value: 'tilki' } });
    await fireEvent.keyDown(screen.getByLabelText('Back'), { key: 'Enter', ctrlKey: true });
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(node.deck.id, 'tilki', '', []));
    await vi.waitFor(() => expect(document.activeElement).toBe(front));
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(shell.hide).toHaveBeenCalled();
    spy.mockRestore();
  });
});
