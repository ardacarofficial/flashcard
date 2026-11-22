/**
 * C1 (closing pass) Match on Subtitle cards: the tiles are the word (`term` field) and its
 * meaning (`meaning` / the card's answer field), never the line, the translation or the
 * source. Playing the round pairs each word with its meaning and logs a practice answer per
 * card (`review_log.mode = match`), read back through the API.
 */
import { cardHistory, endOpenSessions, expect, goToSection, openApp, press, rpc, seed, t, test, uniq } from '../fixtures';

test('C1 match on subtitle cards: word ↔ meaning tiles, round played, answers logged as match', async ({ page }) => {
  await endOpenSessions();
  const s = seed();
  const name = uniq('C1');
  const deck = await rpc<{ id: string }>('decks.create', { path: name, parent_id: null });
  const words = s.subtitle.slice(0, 3);
  const cards: string[] = [];
  for (const w of words) {
    const note = await rpc<{ cards: { id: string }[] }>('notes.create', {
      notetype_id: s.subtitle_notetype_id,
      deck_id: deck.id,
      fields: {
        word: w.lemma,
        surface: w.surface,
        meaning: w.meaning,
        sentence: w.sentence.replace(w.surface, `<b>${w.surface}</b>`),
        translation: w.translation,
        source: 'Film · 1:23',
      },
      tags: [],
      attributes: [],
      allow_duplicate: true,
    });
    cards.push(note.cards[0]!.id);
  }

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-mode-match').click();
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await press(page, 'Enter');
  const board = page.getByTestId('match-board');
  await expect(board).toBeVisible();

  const texts = async (side: string) =>
    (await board.locator(`[data-testid=match-tile][data-side=${side}]`).allTextContents()).map((x) => x.trim()).sort();
  expect(await texts('term')).toEqual(words.map((w) => w.lemma).sort());
  expect(await texts('definition')).toEqual(words.map((w) => w.meaning).sort());
  for (const tile of await board.getByTestId('match-tile').allTextContents()) {
    expect(tile).not.toMatch(/Film|party|train|car|\./);
  }

  for (const w of words) {
    await board.getByRole('button', { name: t('modes.match.term', { text: w.lemma }), exact: true }).click();
    await board.getByRole('button', { name: t('modes.match.definition', { text: w.meaning }), exact: true }).click();
  }
  await expect(page.getByTestId('match-result')).toBeVisible();
  for (const card of cards) {
    await expect.poll(async () => (await cardHistory(card)).reviews.map((r) => r.mode)).toEqual(['match']);
  }
});
