/**
 * S3 Test from the deck page: Library → the deck → Test (the launcher opens with Test chosen)
 * → 10 questions → answer every question → Ctrl+Enter submits → the score is shown, and no
 * card's scheduling fields changed (a test never touches the schedule).
 */
import { cardsIn, endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, scheduling, studyOptions, t, test, uniq } from '../fixtures';

interface Paper {
  session_id: string;
  questions: { index: number; kind: string }[];
  result: { total: number; correct: number } | null;
}

test('S3 deck page → Test → 10 questions → answer → submit: score, schedule untouched', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('S3');
  const deckId = await makeDeck(name, Array.from({ length: 12 }, (_, i): [string, string] => [`${name} term ${i}`, `anlam ${i}`]));
  const cards = await cardsIn(deckId);
  const before = await Promise.all(cards.map(scheduling));

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-mode-test').click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  // v1.4: the paper (kind `test`) is called "Sınav" ("Test" is the four-option quiz).
  await expect(launcher.getByRole('radio', { name: t('study.launcher.kind.test'), exact: true })).toHaveAttribute('aria-checked', 'true');
  const count = launcher.locator('input[type=number], input[inputmode=numeric]').first();
  await count.fill('10');
  await press(page, 'Enter');
  await expect(launcher).toBeHidden();
  await expect.poll(async () => (await studyOptions(deckId)).resumable.some((r) => r.session.kind === 'test')).toBe(true);
  const session = (await studyOptions(deckId)).resumable.find((r) => r.session.kind === 'test')!.session;

  // The paper: 10 questions; answer each (first option / "true" / a typed word).
  const questions = page.locator('[data-testid^="question-"]');
  await expect(questions).toHaveCount(10);
  for (let i = 0; i < 10; i++) {
    const q = questions.nth(i);
    const box = q.getByRole('textbox');
    if (await box.count()) {
      await box.fill(`anlam ${i}`);
    } else {
      await q.getByRole('radio').first().click();
    }
  }
  await press(page, 'Control+Enter');
  await expect(page.getByTestId('test-score')).toBeVisible();

  // Persisted: a graded 10-question paper, and every card's schedule exactly as before.
  const paper = await rpc<Paper>('modes.test.paper', { session_id: session.id, with_key: true });
  expect(paper.questions).toHaveLength(10);
  expect(paper.result?.total).toBe(10);
  await expect(page.getByTestId('test-score')).toContainText(String(paper.result!.correct));
  const after = await Promise.all(cards.map(scheduling));
  expect(after).toEqual(before);
});
