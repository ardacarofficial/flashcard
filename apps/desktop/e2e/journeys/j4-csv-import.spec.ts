/**
 * J4 CSV import through the UI: Create → Import/export, pick the deck, choose a CSV file (the
 * browser build asks for its path), preview, import → the notes exist with the CSV's fields.
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
  await page.getByRole('tab', { name: t('nav.io'), exact: true }).click();
  await choose(page, t('deck.current'), deckName, page.locator('#fc-main'));

  await page.getByRole('radio', { name: t('io.import.source_file') }).click();
  // Outside the desktop shell there is no native file dialog: the app asks for a path.
  page.once('dialog', (d) => void d.accept(csv));
  await page.getByRole('button', { name: t('io.choose_file') }).click();
  await expect(page.getByText(t('io.import.preview_heading')).first()).toBeVisible();

  await page.getByRole('button', { name: t('io.import.start', { deck: deckName }) }).click();
  await expect(page.getByText(t('io.import.done_heading'))).toBeVisible();

  await expect.poll(async () => (await notesIn(deckId)).length).toBe(3);
  const fields = (await notesIn(deckId)).map((n) => [n.fields.front, n.fields.back]).sort();
  expect(fields).toEqual([...rows].sort());
});
