/**
 * C3 (closing pass) A settings change never takes away the card on screen: a learning card
 * due in 10 min is shown at once under the default learn-ahead window (20 min); the window is
 * set to 0 while it is on screen (API, as Settings would); Space + "Biliyorum" (2) is saved (review
 * read back) — no "no longer in the session". The next card is chosen with the new value: the
 * other learning card waits for its step ("Sıradaki kart … sonra").
 */
import { cardHistory, endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, sessionNow, studyOptions, test, tz, uniq } from '../fixtures';

const LEARN_AHEAD = 'study.learn_ahead_mins';

test('C3 learn-ahead lowered while a learning card is on screen: its answer is saved, the next card waits', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('C3');
  const deckId = await makeDeck(name, [[`${name} a`, 'bir'], [`${name} b`, 'iki']]);
  // Both cards into their 10-min step: one "Again" each in a first session (step 1 = 10 min).
  const preset = await rpc<{ id: string; name: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, learning_steps_mins: [10] } });
  await rpc('presets.assign', { deck_ids: [deckId], preset_id: preset.id });
  try {
    await rpc('settings.set', { scope: { kind: 'app' }, key: LEARN_AHEAD, value: 0 });
    const first = await rpc<{ id: string; current: { card_id: string } | null }>('study.start', { kind: 'normal', deck_id: deckId, scope: 'due', tz_offset_mins: tz });
    let s = first;
    while (s.current) {
      s = await rpc('study.answer', { session_id: first.id, card_id: s.current.card_id, rating: 'again', duration_ms: 1000 });
    }
    await rpc('study.end', { session_id: first.id });
    await rpc('settings.reset', { scope: { kind: 'app' }, key: LEARN_AHEAD });

    await openApp(page);
    await goToSection(page, 'shell.section.library');
    await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
    await page.getByTestId('deck-study').click();
    await expect(page.getByTestId('launcher-start')).toBeEnabled();
    await press(page, 'Enter');
    await expect(page.getByTestId('question')).toBeVisible();
    const session = (await studyOptions(deckId)).resumable[0]!.session;
    const shown = (await sessionNow(session.id)).current!.card_id;

    await rpc('settings.set', { scope: { kind: 'app' }, key: LEARN_AHEAD, value: 0 });
    await press(page, 'Space');
    await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
    await press(page, '2');
    await expect.poll(async () => (await cardHistory(shown)).reviews.length).toBe(2);
    await expect(page.getByTestId('study-waiting')).toBeVisible();
    await expect(page.getByTestId('study-waiting').getByRole('heading')).toHaveText(/^Sıradaki kart .+/);
    const now = await sessionNow(session.id);
    expect(now.answered).toBe(1);
    expect(now.ended_at).toBeNull();
    expect(now.current).toBeNull();
  } finally {
    await rpc('settings.reset', { scope: { kind: 'app' }, key: LEARN_AHEAD });
  }
});
