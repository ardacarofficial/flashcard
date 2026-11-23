/**
 * J3 Search and fix: find a card in Search, open its note with Enter, fix the definition, save
 * with Ctrl+Enter → the field is changed in the collection (and nothing else is).
 */
import { expect, goToSection, makeDeck, notesIn, openApp, press, test, uniq } from '../fixtures';

test('J3 search and fix: find, Enter, edit, Ctrl+Enter → field changed in the DB', async ({ page }) => {
  const term = uniq('umbrella').replace(' ', '');
  const deckId = await makeDeck(uniq('J3'), [[term, 'yanlış anlam']]);
  await openApp(page);
  await goToSection(page, 'shell.section.search');

  // The search box has focus on arrival.
  const box = page.getByRole('searchbox').first();
  await expect(box).toBeFocused();
  await page.keyboard.type(term);
  await expect(box).toHaveValue(term);
  await press(page, 'Enter');
  const rows = page.getByTestId('browse-row');
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText(term);

  // Into the grid, open the note, Tab from the front to the back field.
  await press(page, 'ArrowDown', 'Enter');
  const editor = page.getByRole('dialog');
  await expect(editor).toBeVisible();
  // The back field by its name (not `textbox.nth(1)`: before the note type arrives the
  // dialog's second text box is the card's own note; F1).
  const back = editor.getByRole('textbox', { name: 'Arka', exact: true });
  await expect(back).toBeEditable();
  await back.focus();
  await expect(back).toBeFocused();
  // Select the field's text with the keyboard (End, Shift+Home: Ctrl+A is taken, see J7).
  await press(page, 'End', 'Shift+Home');
  await page.keyboard.type('şemsiye');
  await press(page, 'Control+Enter');
  await expect(editor).toBeHidden();

  await expect.poll(async () => (await notesIn(deckId))[0]?.fields.back).toBe('şemsiye');
  const [note] = await notesIn(deckId);
  expect(note?.fields.front).toBe(term);
});
