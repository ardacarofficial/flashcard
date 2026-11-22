/**
 * O1 (owner review) Add cards into the deck you are in: Library → a deck → Create. With decks
 * in the collection the page starts on "Existing deck" with that deck chosen and the cursor in
 * the first term; type a row, Ctrl+Enter → the note is in THAT deck and no deck was created
 * (v1.2 started on "New deck": Ctrl+Enter only said "give the new deck a title").
 */
import { discardDrafts, expect, focused, goToSection, makeDeck, notesIn, openApp, press, rpc, t, test, uniq } from '../fixtures';

interface Node {
  children: Node[];
}
/** How many decks the collection has (every level). */
async function deckCount(): Promise<number> {
  const count = (ns: Node[]): number => ns.reduce((a, n) => a + 1 + count(n.children), 0);
  return count((await rpc<{ roots: Node[] }>('decks.tree', {})).roots);
}

test('O1 Library deck → Create: existing deck preselected, cursor in the first term, Ctrl+Enter saves there', async ({ page }) => {
  await discardDrafts();
  const name = uniq('O1');
  const deckId = await makeDeck(name, [[`${name} seed`, 'tohum']]);
  const decksBefore = await deckCount();

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await expect(page.locator('#fc-main h1', { hasText: name })).toBeVisible();
  await goToSection(page, 'shell.section.create');
  const main = page.locator('#fc-main');
  await main.getByRole('tab', { name: t('add.way.bulk'), exact: true }).click();

  await expect(main.getByRole('radio', { name: t('create.mode_existing'), exact: true })).toHaveAttribute('aria-checked', 'true');
  await expect(main.getByRole('combobox', { name: t('create.deck'), exact: true })).toContainText(name);

  // (Choosing the tab took the focus; O1b checks where the cursor is when the page opens.)
  await page.locator('[data-cell="0:0"]').focus();
  await expect(focused(page)).toHaveAttribute('data-cell', '0:0');
  await page.keyboard.type(`${name} heron`);
  await press(page, 'Tab');
  await page.keyboard.type('balıkçıl');
  await press(page, 'Control+Enter');

  await expect.poll(async () => (await notesIn(deckId)).length).toBe(2);
  const fronts = (await notesIn(deckId)).map((n) => n.fields.front).sort();
  expect(fronts).toEqual([`${name} heron`, `${name} seed`]);
  const decksAfter = await deckCount();
  expect(decksAfter).toBe(decksBefore);
  await expect(main.getByText(t('create.title_required'))).toHaveCount(0);
});

test('O1b Create from the sidebar puts the cursor in the first term', async ({ page }) => {
  await discardDrafts();
  await openApp(page);
  await goToSection(page, 'shell.section.create');
  const main = page.locator('#fc-main');
  // The bulk grid (the last way used may differ: choose it, then re-open the page).
  await main.getByRole('tab', { name: t('add.way.bulk'), exact: true }).click();
  await goToSection(page, 'shell.section.home');
  await goToSection(page, 'shell.section.create');
  await expect(focused(page)).toHaveAttribute('data-cell', '0:0');
});
