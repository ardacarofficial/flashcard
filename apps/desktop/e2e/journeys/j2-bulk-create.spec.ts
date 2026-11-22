/**
 * J2 Bulk create: the Create page, a new deck, 3 rows, Ctrl+Enter → a deck with 3 notes; ONE
 * undo (Ctrl+Z, the app's `oplog.undo`) removes all three (and the new deck).
 */
import { discardDrafts, deckIdByPath, expect, goToSection, notesIn, openApp, press, rpc, t, test, uniq } from '../fixtures';

test('J2 bulk create: 3 rows, Ctrl+Enter → deck with 3 notes; one undo removes all', async ({ page }) => {
  const deckName = uniq('J2');
  await discardDrafts();
  await openApp(page);
  await goToSection(page, 'shell.section.create');

  const title = page.getByRole('textbox', { name: t('create.title') });
  await title.click();
  await page.keyboard.type(deckName);

  const tag = deckName.split(' ')[1];
  const rows: [string, string][] = [
    [`anchor ${tag}`, 'çapa'],
    [`sail ${tag}`, 'yelken'],
    [`wave ${tag}`, 'dalga'],
  ];
  // Tab moves field to field across rows (term → definition → next term …).
  await page.locator('[data-cell="0:0"]').click();
  for (const [i, [term, def]] of rows.entries()) {
    await page.keyboard.type(term);
    await press(page, 'Tab');
    await page.keyboard.type(def);
    if (i < rows.length - 1) await press(page, 'Tab');
  }
  await press(page, 'Control+Enter');

  await expect.poll(() => deckIdByPath(deckName)).not.toBeNull();
  const deckId = (await deckIdByPath(deckName)) as string;
  await expect.poll(async () => (await notesIn(deckId)).length).toBe(3);
  const fields = (await notesIn(deckId)).map((n) => [n.fields.front, n.fields.back]).sort();
  expect(fields).toEqual([...rows].sort());

  // One undo step (the shell's Ctrl+Z = `oplog.undo`): the whole save, the new deck included.
  await page.locator('[data-testid="top-bar"]').click({ position: { x: 5, y: 5 } });
  await press(page, 'Control+z');
  await expect.poll(() => deckIdByPath(deckName)).toBeNull();
  const after = await rpc<{ items: unknown[] }>('notes.list', {
    deck_id: deckId,
    include_subdecks: false,
    page: { cursor: null, limit: null },
  }).catch(() => ({ items: [] }));
  expect(after.items).toHaveLength(0);
});
