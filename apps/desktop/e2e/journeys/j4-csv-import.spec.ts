/**
 * J4 CSV import through the UI: Create → "From a file" (v1.3: importing lives on the add-cards
 * page), existing deck, choose a CSV file (the browser build asks for its path), check the
 * mapped cards, import → the notes exist with the CSV's fields.
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { choose, env, expect, goToSection, makeDeck, notesIn, openApp, t, test, uniq } from '../fixtures';

test('J4 CSV import through the UI → notes exist', async ({ page }) => {
  const deckName = uniq('J4');
  const deckId = await makeDeck(deckName, []);
  const tag = deckName.split(' ')[1];
  const rows: [string, string][] = [
    [`ocean ${tag}`, 'okyanus'],
    [`shore ${tag}`, 'kıyı'],
    [`storm ${tag}`, 'fırtına'],
  ];
  // A real file in the profile folder (a space and Turkish letters in the name on purpose).
  const csv = join(env.profile, `kelimeler ğüşİ ${Date.now()}.csv`);
  writeFileSync(csv, rows.map(([a, b]) => `${a},${b}`).join('\n') + '\n', 'utf8');

  await openApp(page);
  await goToSection(page, 'shell.section.create');
  await page.getByRole('tab', { name: t('add.way.file'), exact: true }).click();
  const main = page.locator('#fc-main');
  await main.getByRole('radio', { name: t('create.mode_existing'), exact: true }).click();
  await choose(page, t('create.deck'), deckName, main);

  // Outside the desktop shell there is no native file dialog: the app asks for a path.
  page.once('dialog', (d) => void d.accept(csv));
  await main.getByRole('button', { name: t('io.file.choose') }).click();
  // The mapped preview is up: the import button names the 3 cards it will add.
  const start = main.getByTestId('import-start');
  await expect(start).toBeEnabled();
  await expect(start).toContainText('3');
  await start.click();
  await expect(main.getByTestId('added-summary')).toBeVisible();

  await expect.poll(async () => (await notesIn(deckId)).length).toBe(3);
  const fields = (await notesIn(deckId)).map((n) => [n.fields.front, n.fields.back]).sort();
  expect(fields).toEqual([...rows].sort());
});
