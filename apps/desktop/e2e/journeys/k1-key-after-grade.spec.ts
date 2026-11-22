/**
 * K1 A key pressed right after a grade never lands on the graded card (v1.3 Y1, "one key = one
 * job"). Four answer buttons; four cards in a row, each graded with a different digit (1–4)
 * and Space pressed IMMEDIATELY after it, with no wait. Saving an answer is made slow on
 * purpose (the page's `study.answer` call is held back 400 ms, as a slow disk would), so that
 * Space always arrives while the graded card is still on screen ("leaving"). Half the cards are
 * turned with Space (focus on the page), half by clicking the card (focus on the card — the card
 * itself listens to Space there).
 *
 * Wanted: while the answer is being saved the graded card keeps showing its back (no flip back
 * to the front), the Space does nothing to it and is not kept for the next card; the next card
 * comes up on its FRONT. Persisted: each card has exactly one review in the session, with the
 * pressed grade, and the session counts exactly four answers.
 */
import { cardHistory, endOpenSessions, expect, goToSection, makeDeck, notesIn, openApp, press, rpc, sessionNow, studyOptions, test, uniq } from '../fixtures';

const BUTTONS = 'study.answer_buttons';
const SLOW_SAVE_MS = 400;
const GRADES = [
  { key: '1', rating: 'again' },
  { key: '2', rating: 'hard' },
  { key: '3', rating: 'good' },
  { key: '4', rating: 'easy' },
] as const;

test('K1 grade + Space with no wait, four cards: one review each with the pressed grade, the next card on its front', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('K1');
  const deckId = await makeDeck(name, Array.from({ length: 6 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]));
  // One learning step of a day: no graded card comes back in this session, so every card on
  // screen is a new one and "exactly one review per card" is exact.
  const preset = await rpc<{ id: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, learning_steps_mins: [1440] } });
  await rpc('presets.assign', { deck_ids: [deckId], preset_id: preset.id });
  const front = new Map<string, string>();
  for (const n of await notesIn(deckId)) {
    for (const c of (n as unknown as { cards: { id: string }[] }).cards) front.set(c.id, n.fields.front!);
  }

  // Every answer is saved slowly: the next key surely lands while the card is leaving.
  await page.route('**/rpc', async (route) => {
    const body = route.request().postDataJSON() as { method?: string } | null;
    if (body?.method && /(^|\.)study\.answer$/.test(body.method)) await new Promise((r) => setTimeout(r, SLOW_SAVE_MS));
    await route.continue();
  });

  await rpc('settings.set', { scope: { kind: 'app' }, key: BUTTONS, value: 'four' });
  try {
    await openApp(page);
    await goToSection(page, 'shell.section.library');
    await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
    await page.getByTestId('deck-study').click();
    await expect(page.getByTestId('launcher-start')).toBeEnabled();
    await press(page, 'Enter');
    await expect(page.getByTestId('study-launcher')).toBeHidden();
    await expect.poll(async () => (await studyOptions(deckId)).resumable.length).toBeGreaterThan(0);
    const session = (await studyOptions(deckId)).resumable[0]!.session;

    const card = page.locator('.fc-flashcard');
    const question = page.getByTestId('question');
    const answers = page.locator('.answer-bar [role=group]');
    const graded: { id: string; rating: string }[] = [];

    for (const [i, g] of GRADES.entries()) {
      // A card on its front.
      const id = (await sessionNow(session.id)).current!.card_id;
      expect(graded.map((x) => x.id)).not.toContain(id);
      await expect(question).toContainText(front.get(id)!);
      await expect(card).not.toHaveClass(/\bflipped\b/);
      await expect(answers).toBeHidden();

      // Turn it: Space on the page, or a click (the card keeps the focus).
      if (i % 2 === 0) await press(page, 'Space');
      else await card.click();
      await expect(card).toHaveClass(/\bflipped\b/);
      await expect(answers).toBeVisible();
      const back = (await page.getByTestId('answer').textContent())?.trim();

      // Grade, then Space at once.
      await press(page, g.key, 'Space');
      graded.push({ id, rating: g.rating });

      // While the answer is saved, the graded card keeps its back: no flip to its front.
      expect((await sessionNow(session.id)).answered).toBe(i);
      await expect(card).toHaveClass(/\bflipped\b/);
      await expect(page.getByTestId('answer')).toHaveText(back!);

      // Saved: the next card, on its FRONT — the Space was not kept for it.
      await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(i + 1);
      const next = (await sessionNow(session.id)).current!.card_id;
      await expect(question).toContainText(front.get(next)!);
      await page.waitForTimeout(300);
      await expect(card).not.toHaveClass(/\bflipped\b/);
      await expect(answers).toBeHidden();
      await expect(page.getByTestId('answer')).toHaveCount(0);
    }

    // Persisted: four answers, one review per card, each with the pressed grade.
    const now = await sessionNow(session.id);
    expect(now.answered).toBe(GRADES.length);
    for (const g of graded) {
      const rows = (await cardHistory(g.id)).reviews.filter((r) => r.session_id === session.id);
      expect(rows.map((r) => r.rating)).toEqual([g.rating]);
    }
    // The card on screen is the session's current one, on its front.
    await expect(question).toContainText(front.get(now.current!.card_id)!);
    await expect(card).not.toHaveClass(/\bflipped\b/);
  } finally {
    await rpc('settings.reset', { scope: { kind: 'app' }, key: BUTTONS });
    await page.unroute('**/rpc');
    await endOpenSessions();
  }
});
