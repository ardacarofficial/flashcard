/**
 * S4 "Struggled with today" is practice: two cards graded Again in a normal session → the
 * launcher offers that scope with count 2 → the session it starts is practice (`trial`):
 * its review-log rows say so, and the cards' scheduling rows are unchanged afterwards.
 */
import { cardHistory, cardsIn, endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, scheduling, sessionNow, studyOptions, test, tz, uniq, type SessionRow } from '../fixtures';

test('S4 struggled today: count 2, practice session, schedule untouched', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('S4');
  const deckId = await makeDeck(name, Array.from({ length: 4 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]));

  // A normal session (setup): the first two cards Again, the other two Easy.
  let s = await rpc<SessionRow>('study.start', { kind: 'normal', deck_id: deckId, tz_offset_mins: tz });
  const struggled: string[] = [];
  const seen = new Set<string>();
  while (s.current && seen.size < 4) {
    const id = s.current.card_id;
    const again = struggled.length < 2 && !seen.has(id);
    seen.add(id);
    if (again) struggled.push(id);
    s = await rpc<SessionRow>('study.answer', { session_id: s.id, card_id: id, rating: again ? 'again' : 'easy', duration_ms: 1000 });
    if (seen.size === 4) break;
  }
  await rpc('study.end', { session_id: s.id });
  expect(struggled).toHaveLength(2);
  const cards = await cardsIn(deckId);
  const before = await Promise.all(cards.map(scheduling));

  // The launcher on the deck shows exactly 2 for "struggled with today".
  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-study').click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  const row = page.getByTestId('launcher-scope-struggled_today');
  await expect(row).toContainText('2');
  expect((await studyOptions(deckId)).scopes.find((o) => o.scope === 'struggled_today')).toMatchObject({ count: 2, mode: 'trial' });
  await row.getByRole('radio').focus();
  await press(page, 'Space');
  await expect(row.getByRole('radio')).toHaveAttribute('aria-checked', 'true');
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();

  await expect.poll(async () => (await studyOptions(deckId)).resumable.length).toBeGreaterThan(0);
  const practice = (await studyOptions(deckId)).resumable[0]!.session;
  expect(practice.mode).toBe('trial');
  await expect(page.getByTestId('study-end')).toBeVisible();
  for (let i = 1; i <= 2; i++) {
    await press(page, 'Space');
    await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
    await press(page, '2');
    await expect.poll(async () => (await sessionNow(practice.id)).answered).toBe(i);
  }

  // Persisted: the two answers are practice rows on exactly the struggled cards…
  const rows = (await Promise.all(cards.map(async (c) => (await cardHistory(c)).reviews))).flat().filter((r) => r.session_id === practice.id);
  expect(rows.map((r) => r.card_id).sort()).toEqual([...struggled].sort());
  for (const r of rows) expect(r.mode).toBe('trial');
  // …and no card's schedule moved.
  expect(await Promise.all(cards.map(scheduling))).toEqual(before);
});
