/**
 * E3W My words (v1.4): open "Kelimelerim", find a word caught by the Subtitle extension (its
 * note has a language, so the word index has it), open it with the keyboard, "Zaten biliyorum"
 * → the index says known by the user's own mark (`words.lookup` read back). The mark is cleared
 * again at the end so later journeys see the seed as it was.
 */
import { expect, goToSection, openApp, press, rpc, seed, sidebarButton, t, test } from '../fixtures';

interface Lookup {
  forms: { key: string | null; status: string; word: { mark: string | null; notes: number } | null }[];
}

test('E3W my words: find a captured word, Enter, "Zaten biliyorum" → known by mark in the index', async ({ page }) => {
  const word = seed().subtitle.find((s) => s.lemma === 'forget')!;
  const before = await rpc<Lookup>('words.lookup', { language: 'en', forms: [word.lemma] });
  expect(before.forms[0]?.word?.notes).toBeGreaterThan(0);
  expect(before.forms[0]?.status).not.toBe('known');

  await openApp(page);
  // The page is in the sidebar once it is placed there; until then under Settings › Other.
  const direct = sidebarButton(page, 'nav.words');
  if (await direct.isVisible()) await direct.click();
  else {
    await goToSection(page, 'shell.settings');
    await page
      .locator('#fc-main')
      .getByRole('navigation', { name: t('shell.settings_nav') })
      .getByRole('button', { name: t('nav.words'), exact: true })
      .click();
  }
  const main = page.locator('#fc-main');
  await expect(main.getByRole('heading', { level: 1, name: t('words.screen.title') })).toBeVisible();

  // `/` → the search box; type, ↓ highlights the match, Enter opens it.
  await expect(main.getByTestId('word-row').first()).toBeVisible();
  await page.locator('body').click({ position: { x: 1, y: 1 } });
  await press(page, '/');
  // (Under Settings › Other the settings search is a second searchbox: take ours by name.)
  const box = main.getByRole('searchbox', { name: t('words.screen.search_label') });
  await expect(box).toBeFocused();
  await page.keyboard.type(word.lemma);
  await expect(main.getByTestId('word-row')).toHaveCount(1);
  await expect(main.getByTestId('word-row').first()).toContainText(word.lemma);
  await press(page, 'ArrowDown', 'Enter');

  const panel = main.getByTestId('word-detail');
  await expect(panel).toBeVisible();
  await expect(panel).toContainText(word.lemma);
  await panel.getByRole('button', { name: t('words.screen.detail.mark_known'), exact: true }).click();

  // Persisted: the index has the user's mark and calls the word known.
  await expect
    .poll(async () => {
      const r = await rpc<Lookup>('words.lookup', { language: 'en', forms: [word.lemma] });
      return [r.forms[0]?.status, r.forms[0]?.word?.mark];
    })
    .toEqual(['known', 'known']);
  // The screen says so in one sentence, and the chip count came from words.stats again.
  await expect(panel.getByTestId('word-say')).toHaveText(t('words.screen.detail.say.known_manual'));
  const stats = await rpc<{ total: { known: number }; languages: { language: string; counts: { known: number } }[] }>('words.stats', {});
  const known = stats.languages.find((l) => l.language === 'en')!.counts.known;
  expect(stats.total.known).toBeGreaterThanOrEqual(known);
  await expect(main.getByTestId('words-count-known')).toHaveText(String(known));

  // Cleared through the words module's own front door (`words.mark`, v1.4 F1) and read back.
  await rpc('words.mark', { language: 'en', lemmas: [word.lemma], mark: null });
  const after = await rpc<Lookup>('words.lookup', { language: 'en', forms: [word.lemma] });
  expect(after.forms[0]?.word?.mark).toBeNull();
});
