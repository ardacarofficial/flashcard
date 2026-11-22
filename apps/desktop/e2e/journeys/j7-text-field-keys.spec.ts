/**
 * J7 Text-field keys belong to the field (found while writing J3, v1.3 bridge work).
 *
 * Fixed in v1.3: the shell's shortcut host (`apps/desktop/src/lib/features/browse/ShortcutHost.svelte`
 * `onKey`, helper `$lib/keyboard` `isEditableTarget`) lets no app-wide binding
 * (`crates/fc-app/src/modules/palette.rs` `UI_ACTIONS`: `Ctrl+A` = select all rows,
 * `Ctrl+Z` = `oplog.undo`) run while a text field has focus — only the palette key, on a combo
 * that edits nothing. One key = one job (COMMON.md): inside a text field these keys edit text.
 */
import { discardDrafts, expect, goToSection, makeDeck, notesIn, openApp, press, test, uniq } from '../fixtures';

test('J7a Ctrl+A in the note editor selects the field text', async ({ page }) => {
  const term = uniq('pencil').replace(' ', '');
  await makeDeck(uniq('J7a'), [[term, 'kurşun kalem']]);
  await discardDrafts();
  await openApp(page);
  await goToSection(page, 'shell.section.search');
  const box = page.getByRole('searchbox').first();
  await expect(box).toBeFocused();
  await page.keyboard.type(term);
  await press(page, 'Enter');
  await expect(page.getByTestId('browse-row')).toHaveCount(1);
  await press(page, 'ArrowDown', 'Enter');
  const back = page.getByRole('dialog').getByRole('textbox').nth(1);
  await back.focus();
  await expect(back).toHaveValue('kurşun kalem');

  await press(page, 'Control+a');
  const selected = await back.evaluate((el) => {
    const t = el as HTMLTextAreaElement;
    return t.value.slice(t.selectionStart, t.selectionEnd);
  });
  expect(selected).toBe('kurşun kalem');
});

test('J7b Ctrl+Z while typing in Quick Add does not undo the card saved before', async ({ page }) => {
  const deckName = uniq('J7b');
  const deckId = await makeDeck(deckName, []);
  await discardDrafts();
  await openApp(page);
  await press(page, 'n');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('combobox').first().click();
  await page.getByRole('option', { name: deckName, exact: true }).click();
  const term = dialog.getByRole('textbox').first();
  await term.focus();
  await page.keyboard.type(`compass ${deckName.split(' ')[1]}`);
  await press(page, 'Tab');
  await page.keyboard.type('pusula');
  await press(page, 'Control+Enter');
  await expect.poll(async () => (await notesIn(deckId)).length).toBe(1);

  // Typing the next card, the user takes back a typo with Ctrl+Z.
  await expect(term).toBeFocused();
  await page.keyboard.type('mapx');

  await press(page, 'Control+z');
  await page.waitForTimeout(1000);
  expect((await notesIn(deckId)).length, 'the saved card must survive a Ctrl+Z typed in a field').toBe(1);
});
