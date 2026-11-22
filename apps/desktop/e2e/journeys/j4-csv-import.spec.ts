/**
 * J4 CSV import through the UI: Create → "From a file" (v1.3: importing lives on the add-cards
 * page), existing deck, choose a CSV file (the browser's file chooser), check the
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
  // A real file in the profile folder (a space in the name on purpose). In the browser build
  // the file's text is read by the page, so the path never reaches the core; headless
  // Chromium does not deliver a file whose path has non-ASCII letters to the chooser.
  const csv = join(env.profile, `kelimeler ${Date.now()}.csv`);
  writeFileSync(csv, rows.map(([a, b]) => `${a},${b}`).join('\n') + '\n', 'utf8');

  await openApp(page);
  await goToSection(page, 'shell.section.create');
  await page.getByRole('tab', { name: t('add.way.file'), exact: true }).click();
  const main = page.locator('#fc-main');
  await main.getByRole('radio', { name: t('create.mode_existing'), exact: true }).click();
  await choose(page, t('create.deck'), deckName, main);

  // Outside the desktop shell the browser's file chooser picks the file (its text goes to the core).
  const chooser = page.waitForEvent('filechooser');
  await main.getByRole('button', { name: t('io.file.choose') }).click();
  await (await chooser).setFiles(csv);
  await expect(main.getByText(/kelimeler/).first()).toBeVisible();
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
