/**
 * E4a (v1.4) Deck language: a deck without `words.language` says so on its page; picking
 * "İngilizce" there stores `en` on the DECK scope (read back with `settings.get`) and its notes
 * leave `words.stats.unassigned_notes` (the word index now counts them). One Ctrl+Z… is the
 * settings command's undo (not asserted here; `settings.set` is an undoable command).
 *
 * E4b İlerleme: the first view's "İlerliyor muyum?" sentence is the word index's number
 * (`words.stats`), and "Daha fazla" opens the details.
 */
import { choose, expect, goToSection, makeDeck, openApp, press, rpc, t, test, uniq } from '../fixtures';

interface WordStats {
  threshold_days: number;
  languages: { language: string; counts: { new: number; learning: number; known: number; ignored: number } }[];
  unassigned_notes: number;
}

test('E4a deck language: picked on the deck page, stored on the deck, its notes counted', async ({ page }) => {
  const name = uniq('E4a');
  const words = ['reluctant', 'thorough', 'eventually'];
  const deckId = await makeDeck(name, words.map((w, i): [string, string] => [`${w} ${name.slice(-6)}`, `anlam ${i}`]));
  const before = await rpc<WordStats>('words.stats', {});
  expect(before.unassigned_notes).toBeGreaterThanOrEqual(3);

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  const openDeck = page.locator('#fc-main').getByRole('button', { name: t('library.open', { name }), exact: true });
  await openDeck.focus();
  await press(page, 'Enter');

  const line = page.getByTestId('deck-language');
  await expect(line).toContainText(t('decks.language.unknown_hint'));
  await choose(page, t('decks.language.picker', { deck: name }), 'İngilizce', line);
  await expect(line).toContainText(t('decks.language.label'));

  // Persisted: the value is on the deck's own scope, and the index counts its notes now.
  await expect
    .poll(async () => (await rpc<{ values: { key: string; value: unknown }[] }>('settings.get', { scope: { kind: 'deck', id: deckId }, keys: ['words.language'] })).values[0]?.value)
    .toBe('en');
  await expect.poll(async () => (await rpc<WordStats>('words.stats', {})).unassigned_notes).toBe(before.unassigned_notes - 3);
});

test('E4b İlerleme: "am I getting better" is the word index’s number; More opens the details', async ({ page }) => {
  const stats = await rpc<WordStats>('words.stats', {});
  await openApp(page);
  await goToSection(page, 'shell.section.stats');
  const say = page.getByTestId('growing-say');
  const main = stats.languages[0];
  const want = !main || main.counts.known === 0 ? t('stats.q.growing.none') : t('stats.q.growing.say', { n: main.counts.known });
  await expect(say).toHaveText(want);
  await expect(page.getByTestId('stats-today')).toBeVisible();
  await expect(page.getByTestId('stats-ahead')).toBeVisible();
  const toggle = page.getByTestId('stats-more-toggle');
  if ((await toggle.getAttribute('aria-expanded')) !== 'true') await toggle.click();
  await expect(page.getByTestId('stats-struggles')).toBeVisible();
});
