/**
 * v1.4 C1 — "Test" is the four-option quiz the owner expected: one word, four meanings, keys
 * 1–4 pick, the right answer shows at once, Enter goes on; at the end the score. The result
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
  await expect(page.getByTestId('launcher-mode-desc')).toContainText(t('study.launcher.kind_desc.quiz'));
  await expect(page.getByTestId('launcher-start')).toBeEnabled();
  await page.getByTestId('launcher-start').click();

  const meaning = new Map(PAIRS);
  let right = 0;
  for (let i = 0; i < PAIRS.length; i++) {
    const word = (await page.getByTestId('question').textContent())?.trim() ?? '';
    expect(meaning.has(word), `asks a word: ${word}`).toBe(true);
    // Four meanings of this deck, the word's own among them.
    const options = page.getByRole('group', { name: t('modes.learn.stage_choice') }).getByRole('button');
    await expect(options).toHaveCount(4);
    const texts = (await options.allTextContents()).join(' | ');
    expect(texts).toContain(meaning.get(word)!);
    // Key 1 picks the first option; the verdict and the right meaning show at once.
    await press(page, '1');
    const feedback = page.getByTestId('learn-feedback');
    await expect(feedback).toBeVisible();
    const ok = ((await feedback.textContent()) ?? '').includes(t('modes.learn.correct'));
    if (ok) right++;
    else await expect(feedback).toContainText(meaning.get(word)!);
    await press(page, 'Enter');
  }
  await expect(page.getByTestId('quiz-score')).toHaveText(t('modes.quiz.done', { correct: right, total: PAIRS.length }));

  // Read back what the core kept.
  const { reviews } = await cardHistory(cards[0]!);
  const sessionId = reviews[reviews.length - 1]?.session_id ?? null;
  const sessions = new Set<string>();
  for (const c of cards) for (const r of (await cardHistory(c)).reviews) if (r.session_id) sessions.add(r.session_id);
  expect(sessions.size).toBe(1);
  const result = await rpc<QuizResult>('modes.quiz.result', { session_id: sessionId ?? [...sessions][0] });
  expect(result).toMatchObject({ total: PAIRS.length, answered: PAIRS.length, correct: right, wrong: PAIRS.length - right, finished: true, affects_schedule: false });
  expect(result.missed.length).toBe(PAIRS.length - right);
  if (result.missed.length) await expect(page.getByTestId('quiz-missed-again')).toBeVisible();
  // Practice: one `trial_quiz` answer per word, schedules untouched.
  const modes = (await Promise.all(cards.map(async (c) => (await cardHistory(c)).reviews.map((r) => r.mode)))).flat();
  expect(modes).toEqual(PAIRS.map(() => 'trial_quiz'));
  expect(await Promise.all(cards.map((c) => scheduling(c)))).toEqual(before);
});
