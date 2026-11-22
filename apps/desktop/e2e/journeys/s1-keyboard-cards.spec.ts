/**
 * S1 Keyboard-only Cards session (v1.3 one key = one job): Home → Enter opens the study
 * launcher → Enter starts Cards on today's queue → Space flips (three times: front, back,
 * front, back) and grades NOTHING → 2 grades exactly one card and the next one is shown →
 * holding 2 down (key repeat) grades exactly one card.
 *
 * Runs in file order like every journey: the whole-collection queue (Home's Start,
 * `deck_id: null`) is the union of each deck's own queue under its own limits (X1), so what
 * earlier journeys studied in their decks never empties it for the others.
 */
import { cardHistory, endOpenSessions, expect, openApp, press, sessionNow, studyOptions, test } from '../fixtures';

test('S1 Home → Enter → Enter → Space×3 grades nothing; 2 grades one; holding 2 grades one', async ({ page }) => {
  await endOpenSessions();
  await openApp(page);

  await press(page, 'Enter');
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  const start = page.getByTestId('launcher-start');
  await expect(start).toBeEnabled();
  await expect(page.getByTestId('launcher-scope-due').getByRole('radio')).toHaveAttribute('aria-checked', 'true');
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();

  // The session the launcher started (the newest open one) and its first card.
  await expect.poll(async () => (await studyOptions(null)).resumable.length).toBeGreaterThan(0);
  const session = (await studyOptions(null)).resumable[0]!.session;
  expect(session.kind).toBe('normal');
  expect(session.mode).toBe('scheduled');
  const first = (await sessionNow(session.id)).current!.card_id;
  const reviewsIn = async (card: string) => (await cardHistory(card)).reviews.filter((r) => r.session_id === session.id).length;

  // Space × 3: the card turns (the answer bar appears with the back), nothing is graded.
  await expect(page.getByTestId('study-end')).toBeVisible();
  await press(page, 'Space', 'Space', 'Space');
  await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
  expect(await reviewsIn(first)).toBe(0);
  expect((await sessionNow(session.id)).answered).toBe(0);

  // 2 grades this card once; the next card is shown.
  await press(page, '2');
  await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(1);
  expect(await reviewsIn(first)).toBe(1);
  const second = (await sessionNow(session.id)).current?.card_id;
  expect(second).toBeTruthy();
  expect(second).not.toBe(first);
  await expect(page.locator('.answer-bar [role=group]')).toBeHidden();

  // Holding 2 (auto-repeat keydowns) on the answer side grades exactly one card.
  await press(page, 'Space');
  await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
  await page.keyboard.down('2');
  await page.keyboard.down('2');
  await page.keyboard.down('2');
  await page.keyboard.down('2');
  await page.keyboard.up('2');
  await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(2);
  await page.waitForTimeout(500);
  expect((await sessionNow(session.id)).answered).toBe(2);
  expect(await reviewsIn(second!)).toBe(1);
});
