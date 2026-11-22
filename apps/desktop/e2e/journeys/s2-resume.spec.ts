/**
 * S2 Leave and come back: start Cards, answer 3, leave with Esc (a pause: the session stays
 * open), reopen the launcher → "Continue" is the first thing (focused) and resumes the SAME
 * session where it was.
 */
import { endOpenSessions, expect, goToSection, makeDeck, openApp, press, sessionNow, studyOptions, test, uniq } from '../fixtures';

test('S2 answer 3, Esc, reopen the launcher: Continue is first and resumes the same session', async ({ page }) => {
  await endOpenSessions();
  // Enough new cards of its own that the session cannot run dry in 3 answers.
  const name = uniq('S2');
  await makeDeck(name, Array.from({ length: 8 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]));
  await openApp(page);

  // Started from its own deck (the whole collection's queue may be used up by earlier
  // journeys today); the launcher is reopened from Home below.
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-study').focus();
  await press(page, 'Enter');
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await press(page, 'Enter');
  await expect(page.getByTestId('study-launcher')).toBeHidden();
  await expect.poll(async () => (await studyOptions(null)).resumable.length).toBeGreaterThan(0);
  const session = (await studyOptions(null)).resumable[0]!.session;

  await expect(page.getByTestId('study-end')).toBeVisible();
  for (let i = 1; i <= 3; i++) {
    await press(page, 'Space');
    await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
    await press(page, '2');
    await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(i);
    // The next card is up (front side) before the next key.
    await expect(page.locator('.answer-bar [role=group]')).toBeHidden();
  }
  const before = await sessionNow(session.id);

  // Esc leaves: what you did so far, and the session is still open.
  await press(page, 'Escape');
  await expect(page.getByTestId('study-continue')).toBeVisible();
  expect((await sessionNow(session.id)).ended_at).toBeNull();

  // Reopen the launcher from Home: "Continue" comes first and has the focus.
  await goToSection(page, 'shell.section.home');
  // (The sidebar button keeps the focus after the click; Enter on Home's "Start studying".)
  await page.getByTestId('home-start').focus();
  await press(page, 'Enter');
  const resume = page.getByTestId('launcher-resume').first();
  await expect(resume).toBeVisible();
  await expect(resume).toBeFocused();
  const opts = await studyOptions(null);
  expect(opts.resumable[0]!.session.id).toBe(session.id);
  expect(opts.resumable[0]!.answered).toBe(3);
  await press(page, 'Enter');
  await expect(page.getByTestId('study-launcher')).toBeHidden();

  // Same session, same place: the card it was on, and the next grade counts on it.
  await expect(page.getByTestId('study-end')).toBeVisible();
  const after = await sessionNow(session.id);
  expect(after.id).toBe(session.id);
  expect(after.answered).toBe(3);
  expect(after.current?.card_id).toBe(before.current?.card_id);
  await press(page, 'Space');
  await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
  await press(page, '2');
  await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(4);
  expect((await studyOptions(null)).resumable.filter((r) => r.session.kind === 'normal')).toHaveLength(1);
});
