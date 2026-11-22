/**
 * S6 Waiting state on a realistic step (learning steps 1 m / 10 m, Anki's defaults):
 *
 * b) With the window at 0 the learner really has to wait: "Sıradaki kart … sonra", the
 *    session stays open, `next_due` is the moment the card is served (its due time), and when
 *    the countdown ends the card comes back by itself.
 * a) Then with the default window (`study.learn_ahead_mins` = 20) a card answered "Again"
 *    (due in 1 min) is the only one left and is within the window: it is served again at
 *    once — no waiting screen for a pointless minute.
 *
 * The setting is changed through the API and reset afterwards (one shared profile).
 */
import { endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, sessionNow, studyOptions, test, uniq } from '../fixtures';

const LEARN_AHEAD = 'study.learn_ahead_mins';

test('S6 1 m step: with learn-ahead 0 the wait is real and ends on time; within the default window the card comes straight back', async ({ page }) => {
  test.setTimeout(180_000);
  await endOpenSessions();
  const name = uniq('S6');
  const deckId = await makeDeck(name, [[`${name} only`, 'tek']]);
  const preset = await rpc<{ id: string; name: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, learning_steps_mins: [1, 10] } });
  await rpc('presets.assign', { deck_ids: [deckId], preset_id: preset.id });
  try {
    // b) first, with no learn-ahead (set before the session, so no card is ever shown under
    // the other window): "Again" → the learner has to wait the whole minute.
    await rpc('settings.set', { scope: { kind: 'app' }, key: LEARN_AHEAD, value: 0 });
    await openApp(page);
    await goToSection(page, 'shell.section.library');
    await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
    await page.getByTestId('deck-study').click();
    await expect(page.getByTestId('launcher-start')).toBeEnabled();
    await press(page, 'Enter');
    await expect(page.getByTestId('study-launcher')).toBeHidden();
    await expect(page.getByTestId('study-end')).toBeVisible();
    const session = (await studyOptions(deckId)).resumable[0]!.session;
    const card = (await sessionNow(session.id)).current!.card_id;
    const waiting = page.getByTestId('study-waiting');
    const answerBar = page.locator('.answer-bar [role=group]');

    await press(page, 'Space');
    await expect(answerBar).toBeVisible();
    await press(page, '1');
    await expect(waiting).toBeVisible();
    await expect(waiting.getByRole('heading')).toHaveText(/^Sıradaki kart .+/);
    const now = await sessionNow(session.id);
    expect(now.ended_at).toBeNull();
    expect(now.current).toBeNull();
    // The countdown's end is the step's due time (1 min after the answer), not 20 min before it.
    const left = now.next_due! - Date.now();
    expect(left).toBeGreaterThan(40_000);
    expect(left).toBeLessThanOrEqual(60_000);
    expect((await studyOptions(deckId)).resumable.map((r) => r.session.id)).toContain(session.id);

    // It ends on time: the card is back by itself (no click), not before `next_due`.
    await expect(waiting).toBeHidden({ timeout: 90_000 });
    expect(Date.now()).toBeGreaterThanOrEqual(now.next_due! - 1_000);
    const after = await sessionNow(session.id);
    expect(after.current?.card_id).toBe(card);
    expect(after.ended_at).toBeNull();

    // a) The default window (20 min): "Again" (due in 1 min) is the only card left and within
    // the window → straight back, no waiting screen for a pointless minute.
    await rpc('settings.reset', { scope: { kind: 'app' }, key: LEARN_AHEAD });
    await press(page, 'Space');
    await expect(answerBar).toBeVisible();
    await press(page, '1');
    await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(2);
    await expect(answerBar).toBeHidden();
    const back = await sessionNow(session.id);
    expect(back.current?.card_id).toBe(card);
    expect(back.next_due).toBeNull();
    await expect(waiting).toBeHidden();
    await expect(page.getByTestId('question')).toBeVisible();
  } finally {
    await rpc('settings.reset', { scope: { kind: 'app' }, key: LEARN_AHEAD });
  }
});
