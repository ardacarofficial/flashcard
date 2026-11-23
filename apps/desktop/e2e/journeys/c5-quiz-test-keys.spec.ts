/**
 * v1.4 C1 — "Test" is the four-option quiz the owner expected: one word, four meanings, keys
 * 1–4 pick, the right answer shows at once, Enter goes on (a right pick goes on by itself),
 * Space does nothing; at the end the score. The result
 * is read back from the core (`modes.quiz.result`): every word answered, the score the screen
 * showed, practice reviews (`trial_quiz`) and an untouched schedule.
 */
import { cardHistory, cardsIn, endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, scheduling, t, test, uniq } from '../fixtures';

const PAIRS: [string, string][] = [
  ['badger', 'porsuk'],
  ['otter', 'su samuru'],
  ['heron', 'balıkçıl'],
  ['lynx', 'vaşak'],
  ['wolverine', 'porsuk kurdu'],
];

interface QuizResult {
  session_id: string;
  total: number;
  answered: number;
  correct: number;
  wrong: number;
  finished: boolean;
  affects_schedule: boolean;
  missed: { card_id: string; term: string; expected: string; picked: string | null }[];
}

test('C5 Test (quiz): keys 1–4 pick, the answer shows at once, the score is persisted', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('C5');
  const deckId = await makeDeck(name, PAIRS);
  const cards = await cardsIn(deckId);
  const before = await Promise.all(cards.map((c) => scheduling(c)));

  await openApp(page);
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId('deck-mode-learn').click();
  // The launcher lists the mode as "Test" (its own id: quiz) next to "Sınav" (the paper).
  await page.getByRole('radio', { name: t('study.launcher.kind.quiz'), exact: true }).click();
  // v1.4: one line under the mode says what it does; the footer says it is practice.
  await expect(page.getByTestId('launcher-mode-desc')).toContainText(t('study.mode.quiz.description'));
  await expect(page.getByTestId('launcher-schedule')).toContainText(t('study.launcher.sched.practice_mode'));
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await page.getByTestId('launcher-start').click();

  const meaning = new Map(PAIRS);
  let right = 0;
  for (let i = 0; i < PAIRS.length; i++) {
    const question = page.getByTestId('question');
    const word = (await question.textContent())?.trim() ?? '';
    expect(meaning.has(word), `asks a word: ${word}`).toBe(true);
    // Four meanings of this deck, the word's own among them.
    const options = page.getByTestId('quiz-options').getByRole('button');
    await expect(options).toHaveCount(4);
    const texts = (await options.allTextContents()).map((x) => x.trim());
    // Exact meaning (the option text starts with its key number): "porsuk" must not match "porsuk kurdu".
    const at = texts.findIndex((x) => x.replace(/^\d+\s*/, '') === meaning.get(word)!);
    expect(at, texts.join(' | ')).toBeGreaterThanOrEqual(0);
    // Space never answers here (Space only flips cards).
    await press(page, 'Space');
    await expect(page.getByTestId('quiz-feedback')).toHaveCount(0);
    // Every other word: the right key; else a wrong one. The verdict shows at once.
    const pickRight = i % 2 === 0;
    const key = pickRight ? at : (at + 1) % 4;
    await press(page, String(key + 1));
    const feedback = page.getByTestId('quiz-feedback');
    if (pickRight) {
      right++;
      // A right pick goes on by itself: wait for the next word (the last one: the result).
      if (i < PAIRS.length - 1) await expect(question).not.toHaveText(word);
    } else {
      // A wrong pick waits for Enter with the right meaning shown.
      await expect(feedback).toHaveAttribute('data-correct', 'false');
      await expect(feedback).toContainText(meaning.get(word)!);
      await press(page, 'Enter');
    }
  }
  await expect(page.getByTestId('quiz-score')).toHaveText(t('modes.quiz.done', { correct: right, total: PAIRS.length }));
  await expect(page.getByTestId('summary-schedule')).toContainText(t('modes.summary.practice'));

  // Read back what the core kept.
  const { reviews } = await cardHistory(cards[0]!);
  const sessionId = reviews[reviews.length - 1]?.session_id ?? null;
  const sessions = new Set<string>();
  for (const c of cards) for (const r of (await cardHistory(c)).reviews) if (r.session_id) sessions.add(r.session_id);
  expect(sessions.size).toBe(1);
  const result = await rpc<QuizResult>('modes.quiz.result', { session_id: sessionId ?? [...sessions][0] });
  expect(result).toMatchObject({ total: PAIRS.length, answered: PAIRS.length, correct: right, wrong: PAIRS.length - right, finished: true, affects_schedule: false });
  expect(result.missed.length).toBe(PAIRS.length - right);
  if (result.missed.length) await expect(page.getByTestId('summary-repeat')).toBeVisible();
  // The missed list on the screen is the core's.
  for (const m of result.missed) await expect(page.getByTestId('summary-missed')).toContainText(m.term);
  // Practice: one `trial_quiz` answer per word, schedules untouched.
  const modes = (await Promise.all(cards.map(async (c) => (await cardHistory(c)).reviews.map((r) => r.mode)))).flat();
  expect(modes).toEqual(PAIRS.map(() => 'trial_quiz'));
  expect(await Promise.all(cards.map((c) => scheduling(c)))).toEqual(before);
});
