/**
 * O4 (owner review) A test paper with the keyboard only: deck page → Test → 6 questions →
 * A–D picks (and moves on), a typed answer + Enter moves on, the last one lands on "Submit",
 * Ctrl+Enter hands it in → every question was answered (no blanks in the graded paper).
 */
import { endOpenSessions, expect, focused, goToSection, makeDeck, openApp, press, rpc, studyOptions, test, uniq } from '../fixtures';

interface Paper {
  questions: { index: number; kind: string }[];
  result: { total: number; items: { index: number; verdict: string }[] } | null;
}

test('O4 test paper: A–D / typing + Enter answer and move on; Ctrl+Enter submits with no blanks', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('O4');
  const deckId = await makeDeck(
    name,
    Array.from({ length: 8 }, (_, i): [string, string] => [`${name} w${i}`, `anlam ${i}`]),
  );

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-mode-test').click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  await launcher.locator('#launch-count').fill('6');
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();
  const questions = page.locator('[data-testid^="question-"]');
  await expect(questions).toHaveCount(6);
  await expect.poll(async () => (await studyOptions(deckId)).resumable.some((r) => r.session.kind === 'test')).toBe(true);
  const session = (await studyOptions(deckId)).resumable.find((r) => r.session.kind === 'test')!.session;

  // The paper opens in its first answer; each answer moves the focus to the next question.
  for (let i = 0; i < 6; i++) {
    const q = questions.nth(i);
    await expect(focused(page)).toBeAttached();
    const inQuestion = await q.evaluate((li) => li.contains(document.activeElement));
    expect(inQuestion).toBe(true);
    if ((await q.getByRole('textbox').count()) > 0) {
      await page.keyboard.type(`anlam ${i}`);
      await press(page, 'Enter');
    } else {
      await press(page, 'a');
    }
  }
  await expect(focused(page)).toHaveAttribute('data-testid', 'test-submit');
  await press(page, 'Control+Enter');
  await expect(page.getByTestId('test-score')).toBeVisible();

  const graded = await rpc<Paper>('modes.test.paper', { session_id: session.id, with_key: true });
  expect(graded.result?.total).toBe(6);
  expect(graded.result!.items.filter((it) => it.verdict === 'blank')).toEqual([]);
});
