/**
 * O2 (owner review) Leave in the middle and come back, keyboard only: Library → the deck
 * row's visible "Study" (no hover needed) → launcher → Enter → Space, 2 → Esc (pause) → the
 * focus is on "Continue" → Enter resumes the SAME session → Esc → Tab → "Done" → the deck
 * page, and the paused session is still there to continue.
 */
import { endOpenSessions, expect, focused, goToSection, makeDeck, openApp, press, sessionNow, studyOptions, t, test, uniq } from '../fixtures';

test('O2 pause → Enter continues the same session; Done goes back to the deck page', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('O2');
  const deckId = await makeDeck(name, [
    [`${name} one`, 'bir'],
    [`${name} two`, 'iki'],
    [`${name} three`, 'üç'],
  ]);

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  const row = page.getByTestId('deck-row').filter({ hasText: name }).first().locator('xpath=ancestor::li[1]');
  const study = row.getByRole('button', { name: t('library.study', { name }) });
  // The primary action is visible without hovering the row.
  await page.mouse.move(0, 0);
  await expect(study).toBeVisible();
  const opacity = await study.evaluate((el) => getComputedStyle(el.closest('.primary') ?? el).opacity);
  expect(opacity).toBe('1');
  await study.click();

  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();
  await expect.poll(async () => (await studyOptions(deckId)).resumable.length).toBe(1);
  const session = (await studyOptions(deckId)).resumable[0]!.session;

  await expect(page.getByTestId('study-end')).toBeVisible();
  await press(page, 'Space');
  await expect(page.locator('.answer-bar [role=group]')).toBeVisible();
  await press(page, '2');
  await expect.poll(async () => (await sessionNow(session.id)).answered).toBe(1);

  // Esc pauses: the summary's first stop is "Continue".
  await press(page, 'Escape');
  await expect(page.getByTestId('study-continue')).toBeVisible();
  await expect(focused(page)).toHaveAttribute('data-testid', 'study-continue');
  await press(page, 'Enter');
  await expect(page.getByTestId('study-end')).toBeVisible();
  const again = await sessionNow(session.id);
  expect(again.ended_at).toBeNull();
  expect(again.answered).toBe(1);
  expect((await studyOptions(deckId)).resumable.map((r) => r.session.id)).toEqual([session.id]);

  // Esc, then "Done": back on the deck page; the paused session stays open for "Continue".
  await press(page, 'Escape');
  await expect(focused(page)).toHaveAttribute('data-testid', 'study-continue');
  await press(page, 'Tab');
  await expect(focused(page)).toHaveText(t('study.summary.back'));
  await press(page, 'Enter');
  await expect(page.locator('#fc-main h1', { hasText: name })).toBeVisible();
  expect((await sessionNow(session.id)).ended_at).toBeNull();
});
