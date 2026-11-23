/**
 * v1.4 E1 — the launcher as the modes prototype draws it, on the real core:
 * (a) Sınav: the question types are check boxes from `study.options` (true/false off by
 *     default); none checked = Start off with the reason; "only matching, 2 questions" makes a
 *     paper of exactly that (read back with `modes.test.paper`), and the screen says it is practice.
 * (b) Test: Esc pauses — the session stays open in the core (not ended, resumable), Enter
 *     continues with the next word; the answer given before the pause is kept.
 */
import { cardHistory, cardsIn, endOpenSessions, expect, goToSection, makeDeck, openApp, press, rpc, sessionNow, studyOptions, t, test, uniq } from '../fixtures';

/** Words must be unique across the collection (duplicate check): each test gets its own. */
const pairs = (tag: string): [string, string][] => BASE.map(([w, m]) => [`${w} ${tag}`, m]);
const BASE: [string, string][] = [
  ['bargain', 'pazarlık'],
  ['borrow', 'ödünç almak'],
  ['come across', 'rastlamak'],
  ['figure out', 'çözmek'],
  ['get rid of', 'kurtulmak'],
  ['give up', 'vazgeçmek'],
];

interface Paper {
  session_id: string;
  questions: { index: number; kind: string }[];
}

async function openDeckLauncher(page: import('@playwright/test').Page, name: string, mode: 'test' | 'learn') {
  await goToSection(page, 'shell.section.library');
  await page.getByTestId('deck-row').filter({ hasText: name }).first().click();
  await page.getByTestId(`deck-mode-${mode}`).click();
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  return launcher;
}

test('E1a Sınav: question types from the API, none = no start, only matching → a matching paper', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('E1a');
  const deckId = await makeDeck(name, pairs(name));
  const exam = (await studyOptions(deckId)) as unknown as { modes: { kind: string; affects_schedule: boolean; default_question_kinds: string[] }[] };
  const mode = exam.modes.find((m) => m.kind === 'test')!;
  expect(mode.affects_schedule).toBe(false);
  expect(mode.default_question_kinds).not.toContain('true_false');

  await openApp(page);
  const launcher = await openDeckLauncher(page, name, 'test');
  await expect(launcher.getByRole('radio', { name: t('study.launcher.kind.test'), exact: true })).toHaveAttribute('aria-checked', 'true');
  await expect(launcher.getByTestId('launcher-mode-test')).toContainText(t('study.launcher.chip.practice'));
  const kinds = launcher.getByRole('group', { name: t('modes.test.kinds') });
  const kind = (k: string) => kinds.getByRole('checkbox', { name: new RegExp(`^${t(`modes.test.kind.${k}`)}`) });
  // The check boxes say what the API's defaults are.
  for (const k of ['multiple_choice', 'written', 'matching', 'true_false']) {
    if (mode.default_question_kinds.includes(k)) await expect(kind(k)).toBeChecked();
    else await expect(kind(k)).not.toBeChecked();
  }
  // None checked: Start is off and the footer says why.
  for (const k of mode.default_question_kinds) await kind(k).uncheck();
  await expect(page.getByTestId('launcher-start')).toBeDisabled();
  await expect(page.getByTestId('launcher-summary')).toContainText(t('study.launcher.exam_no_kinds'));
  // Only matching, two questions.
  await kind('matching').check();
  await launcher.getByRole('spinbutton').first().fill('2');
  await expect(page.getByTestId('launcher-summary')).toContainText(t('study.launcher.sched.practice_mode'));
  await page.getByTestId('launcher-start').click();
  await expect(launcher).toBeHidden();
  await expect.poll(async () => (await studyOptions(deckId)).resumable.some((r) => r.session.kind === 'test')).toBe(true);
  const session = (await studyOptions(deckId)).resumable.find((r) => r.session.kind === 'test')!.session;
  const paper = await rpc<Paper>('modes.test.paper', { session_id: session.id, with_key: null });
  expect(paper.questions.map((q) => q.kind)).toEqual(['matching', 'matching']);
  await expect(page.locator('[data-testid^="question-"]')).toHaveCount(2);
  // Hand it in blank: the result says the schedule did not change.
  await press(page, 'Control+Enter');
  await expect(page.getByTestId('test-score')).toBeVisible();
  await expect(page.getByTestId('test-schedule')).toContainText(t('modes.test.practice'));
});

test('E1b Test: Esc pauses (the session stays open), Enter continues; the answer before the pause is kept', async ({ page }) => {
  await endOpenSessions();
  const name = uniq('E1b');
  const deckId = await makeDeck(name, pairs(name));
  const cards = await cardsIn(deckId);

  await openApp(page);
  const launcher = await openDeckLauncher(page, name, 'learn');
  await launcher.getByRole('radio', { name: t('study.launcher.kind.quiz'), exact: true }).click();
  await page.getByTestId('launcher-start').click();
  await expect(launcher).toBeHidden();

  const question = page.getByTestId('question');
  const first = ((await question.textContent()) ?? '').trim();
  await press(page, '1');
  await expect(page.getByTestId('quiz-feedback')).toBeVisible();
  // Esc on the feedback: paused; the answer counted, the session is still open in the core.
  await press(page, 'Escape');
  await expect(page.getByTestId('quiz-paused')).toBeVisible();
  const sessions = new Set<string>();
  for (const c of cards) for (const r of (await cardHistory(c)).reviews) if (r.session_id) sessions.add(r.session_id);
  expect(sessions.size).toBe(1);
  const id = [...sessions][0]!;
  expect((await sessionNow(id)).ended_at).toBeNull();
  const progress = await rpc<{ answered: number; finished: boolean }>('modes.quiz.result', { session_id: id });
  expect(progress).toMatchObject({ answered: 1, finished: false });
  // Enter continues with the NEXT word.
  await press(page, 'Enter');
  await expect(question).toBeVisible();
  await expect(question).not.toHaveText(first);
  await expect(page.getByTestId('quiz-progress')).toContainText('1');
});
