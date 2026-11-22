/**
 * J1 Quick Add: N opens the dialog, type term and definition, Ctrl+Enter adds — twice. Both
 * notes exist, in the chosen deck, with exactly the typed fields.
 */
import { discardDrafts, choose, expect, focused, makeDeck, notesIn, openApp, press, t, test, uniq } from '../fixtures';

test('J1 quick add: N, type, Ctrl+Enter twice → two notes in the chosen deck', async ({ page }) => {
  const deckName = uniq('J1');
  const deckId = await makeDeck(deckName, []);
  await discardDrafts();
  await openApp(page);

  await press(page, 'n');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await choose(page, t('quickadd.deck'), deckName, dialog);

  // Unique per run: a word already in the collection would (rightly) stop at the duplicate check.
  const tag = deckName.split(' ')[1];
  const words: [string, string][] = [
    [`harbour ${tag}`, 'liman'],
    [`lighthouse ${tag}`, 'deniz feneri'],
  ];
  const term = dialog.getByRole('textbox').first();
  for (const [front, back] of words) {
    await term.focus();
    await expect(focused(page)).toHaveValue('');
    await page.keyboard.type(front);
    await press(page, 'Tab');
    await page.keyboard.type(back);
    await press(page, 'Control+Enter');
    // Added: the fields clear and the cursor goes back to the first one.
    await expect(term).toHaveValue('');
  }
  await expect(dialog.getByTestId('quick-add-count')).toContainText('2');

  await expect.poll(async () => (await notesIn(deckId)).length).toBe(2);
  const notes = await notesIn(deckId);
  const fields = notes.map((n) => [n.fields.front, n.fields.back]).sort();
  expect(fields).toEqual([...words].sort());
});
