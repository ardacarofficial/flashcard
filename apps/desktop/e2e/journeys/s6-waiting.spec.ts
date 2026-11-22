/**
 * S6 Waiting state: the only card left is a learning card that can be shown in a few minutes
 * → the session says "next card in …" and stays open (it does not close or show a summary).
 *
 * The core shows a waiting learning card up to 20 minutes EARLY when nothing else is left
 * (`fc_study::session::LEARN_AHEAD_MS`, Anki's "learn ahead"), so with the default 1 m / 10 m
 * steps the card simply comes back at once. The deck therefore gets a 25-minute learning step:
 * after "Again" the card is due in 25 min and can be shown in 5 — the waiting state.
 */
import { endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, sessionNow, studyOptions, test, uniq } from '../fixtures';

test('S6 a learning card due in minutes: "Sıradaki kart … sonra", the session stays open', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('S6');
  const deckId = await makeDeck(name, [[`${name} only`, 'tek']]);
  const preset = await rpc<{ id: string; name: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, learning_steps_mins: [25] } });
  await rpc('presets.assign', { deck_ids: [deckId], preset_id: preset.id });
  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-study').click();
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await press(page, 'Enter');
  await expect(page.getByTestId('study-launcher')).toBeHidden();
  await expect(page.getByTestId('study-end')).toBeVisible();
  const session = (await studyOptions(deckId)).resumable[0]!.session;

  // Flip, "1" (Again / still learning): the card goes back into learning for a few minutes.
  await press(page, 'Space');
  await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
  await press(page, '1');

  const waiting = page.getByTestId('study-waiting');
  await expect(waiting).toBeVisible();
  await expect(waiting.getByRole('heading')).toHaveText(/^Sıradaki kart .+/);
  const now = await sessionNow(session.id);
  expect(now.ended_at).toBeNull();
  expect(now.current).toBeNull();
  expect(now.next_due).not.toBeNull();
  // Shown again in ~5 minutes (due in 25, learn-ahead 20).
  expect(now.next_due! - Date.now()).toBeGreaterThan(3 * 60_000);
  expect(now.next_due! - Date.now()).toBeLessThan(6 * 60_000);
  await expect(waiting.getByRole('heading')).toHaveText(/^Sıradaki kart [3-6] dk sonra$/);
  // Still waiting a moment later: nothing closed it behind the learner's back.
  await page.waitForTimeout(1_500);
  await expect(waiting).toBeVisible();
  expect((await sessionNow(session.id)).ended_at).toBeNull();
  expect((await studyOptions(deckId)).resumable.map((r) => r.session.id)).toContain(session.id);
});
