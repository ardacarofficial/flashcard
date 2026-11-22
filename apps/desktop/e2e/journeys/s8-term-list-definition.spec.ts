/**
 * S8 Deck page term list (`features/decks/TermList.svelte`): editing the "Definition" cell of
 * a Subtitle note changes its MEANING field — never the surface form ("went"), which v1.2 showed
 * and overwrote as the definition (gpt-review-1 §B, CONTRACT.md §2 `notes.terms`).
 *
 * Own deck, own Subtitle-type note (the seeded Subtitle deck is left alone).
 */
import { expect, goToSection, openApp, press, rpc, seed, t, test, uniq, type NoteRow } from '../fixtures';

test('S8 term list: editing the Definition of a subtitle note changes meaning, not surface', async ({ page }) => {
  const deckName = uniq('S8');
  const deck = await rpc<{ id: string }>('decks.create', { path: deckName, parent_id: null });
  const fields = {
    word: 'go',
    surface: 'went',
    meaning: 'gitmek',
    sentence: 'I <b>went</b> home after the party.',
    translation: 'Partiden sonra eve gittim.',
    source: 'Journey fixture',
  };
  const note = await rpc<{ id: string }>('notes.create', {
    notetype_id: seed().subtitle_notetype_id,
    deck_id: deck.id,
    fields,
    tags: [],
    attributes: [],
    allow_duplicate: true,
  });

  // Library → open the deck (focus its name, Enter).
  await openApp(page);
  await goToSection(page, 'shell.section.library');
  const openDeck = page.locator('#fc-main').getByRole('button', { name: t('library.open', { name: deckName }), exact: true });
  await openDeck.focus();
  await press(page, 'Enter');

  // The row reads term | definition = go | gitmek (not go | went).
  const list = page.getByTestId('term-list');
  const row = list.getByRole('listitem').filter({ hasText: 'go' });
  await expect(row).toHaveCount(1);
  await expect(row.locator('.front')).toHaveText('go');
  await expect(row.locator('.back')).toHaveText('gitmek');

  // The row's edit button (focus + Enter) turns it into two fields; Tab to the definition.
  await row.getByRole('button', { name: t('deck_page.edit', { term: 'go' }), exact: true }).focus();
  await press(page, 'Enter');
  const term = list.getByRole('textbox', { name: t('deck_page.edit_front'), exact: true });
  const definition = list.getByRole('textbox', { name: t('deck_page.edit_back'), exact: true });
  await expect(term).toBeFocused();
  await expect(term).toHaveValue('go');
  await expect(definition).toHaveValue('gitmek');
  await press(page, 'Tab');
  await expect(definition).toBeFocused();
  await press(page, 'Control+a');
  await page.keyboard.type('gitmek, varmak');
  await press(page, 'Control+Enter');
  await expect(definition).toBeHidden();
  await expect(list.getByRole('listitem').filter({ hasText: 'go' }).locator('.back')).toHaveText('gitmek, varmak');

  // Persisted: meaning changed; the word, the surface form and everything else did not.
  await expect.poll(async () => (await rpc<NoteRow>('notes.get', { id: note.id })).fields.meaning).toBe('gitmek, varmak');
  const after = await rpc<NoteRow>('notes.get', { id: note.id });
  expect(after.fields).toMatchObject({ ...fields, meaning: 'gitmek, varmak' });
  expect(after.fields.surface).toBe('went');
});
