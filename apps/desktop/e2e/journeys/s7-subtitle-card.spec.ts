/**
 * S7 Subtitle card, the way the owner meets it (CONTRACT.md §2, gpt-review-2 §3 "went → gitmek"):
 *
 * a) a Test session's written question for word=go, surface=went: typing "gitmek" is graded
 *    correct (driven through the launcher and the paper, read back with the key);
 * b) the card back (Cards session, Space) shows meaning, example and source as separate,
 *    labelled parts (`fc-part-*`, labels from `cards.part.example` / `cards.part.source`);
 * c) the hint rule (X1): a hint may reveal context but never the answer. The Subtitle card
 *    asks with the word only (went, its dictionary form "go" beside it); its first hint (H) is
 *    the line with the word highlighted, never the meaning. A reverse card (a Vocabulary
 *    note's "production" card: shows the meaning, asks for the word) blanks the word in the
 *    same sentence: "I ___ home after the party.", never "go"/"went".
 *
 * Every part uses its own deck with its own Subtitle-type note (the seeded Subtitle deck is
 * left alone; `allow_duplicate` because the seed already has a "go").
 */
import type { Page } from '@playwright/test';
import { expect, goToSection, openApp, press, rpc, seed, t, test, uniq } from '../fixtures';

const GO = {
  word: 'go',
  surface: 'went',
  meaning: 'gitmek',
  sentence: 'I <b>went</b> home after the party.',
  translation: 'Partiden sonra eve gittim.',
  source: 'Journey fixture',
};

interface Paper {
  session_id: string;
  questions: { index: number; kind: string; key: { expected: string } | null }[];
  answers: { index: number; text: string | null }[] | null;
  result: { total: number; correct: number; items: { index: number; verdict: string }[] } | null;
}

/** A deck holding one Subtitle note go/went → gitmek (API setup, not the journey). */
async function subtitleDeck(prefix: string): Promise<{ name: string; deckId: string; noteId: string }> {
  const name = uniq(prefix);
  const deck = await rpc<{ id: string }>('decks.create', { path: name, parent_id: null });
  const note = await rpc<{ id: string }>('notes.create', {
    notetype_id: seed().subtitle_notetype_id,
    deck_id: deck.id,
    fields: GO,
    tags: [],
    attributes: [],
    allow_duplicate: true,
  });
  return { name, deckId: deck.id, noteId: note.id };
}

/** Library → the deck's "study" button (focus + Enter) → the study launcher. */
async function openLauncher(page: Page, deckName: string) {
  await goToSection(page, 'shell.section.library');
  const study = page.locator('#fc-main').getByRole('button', { name: t('library.study', { name: deckName }), exact: true });
  await study.focus();
  await press(page, 'Enter');
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  // The launcher has loaded its options (the mode switch is there).
  await expect(launcher.getByRole('radio', { name: t('study.launcher.kind.normal'), exact: true })).toBeVisible();
  return launcher;
}

/** Starts a Cards session on the deck from the launcher (Enter = start) and waits for the card. */
async function startCards(page: Page, deckName: string) {
  const launcher = await openLauncher(page, deckName);
  await expect(launcher.getByRole('radio', { name: t('study.launcher.kind.normal'), exact: true })).toBeChecked();
  await launcher.getByRole('radio', { name: t('study.launcher.kind.normal'), exact: true }).focus();
  await press(page, 'Enter');
  await expect(page.getByTestId('question')).toBeVisible();
}

const plain = (s: string) => s.replace(/\s+/g, ' ').trim();
const hasWord = (text: string, word: string) => new RegExp(`(^|[^\\p{L}])${word}([^\\p{L}]|$)`, 'iu').test(text);

test('S7a subtitle card, written test question: "gitmek" for go/went is correct', async ({ page }) => {
  const { name, deckId } = await subtitleDeck('S7a');
  await openApp(page);
  const launcher = await openLauncher(page, name);

  // Mode: Test (keyboard: focus the radio, Space); only written questions.
  await launcher.getByRole('radio', { name: t('study.launcher.kind.test'), exact: true }).focus();
  await press(page, 'Space');
  await expect(launcher.getByRole('radio', { name: t('study.launcher.kind.test'), exact: true })).toBeChecked();
  await launcher.getByRole('button', { name: t('study.launcher.options') }).focus();
  await press(page, 'Enter');
  const kinds = launcher.getByRole('group', { name: t('modes.test.kinds') });
  for (const k of ['true_false', 'multiple_choice'] as const) {
    await kinds.getByRole('button', { name: t(`modes.test.kind.${k}`), exact: true }).focus();
    await press(page, 'Space');
    await expect(kinds.getByRole('button', { name: t(`modes.test.kind.${k}`), exact: true })).toHaveAttribute('aria-pressed', 'false');
  }
  await expect(kinds.getByRole('button', { name: t('modes.test.kind.written'), exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByTestId('launcher-start').focus();
  // The session the UI starts (its own `study.start` call through the bridge).
  const started = page.waitForResponse((r) => r.url().endsWith('/rpc') && /(^|\.)study\.start$/.test(r.request().postDataJSON()?.method ?? ''));
  await press(page, 'Enter');
  const reply = (await (await started).json()) as { ok: boolean; result?: { id: string; kind: string; deck_id: string | null } };
  expect(reply.ok).toBe(true);
  expect(reply.result?.kind).toBe('test');
  expect(reply.result?.deck_id).toBe(deckId);
  const sessionId = reply.result?.id;

  // The paper: one written question about "go" in its subtitle line.
  const question = page.getByTestId('question-0');
  await expect(question).toBeVisible();
  await expect(question).toContainText(t('modes.test.kind.written'));
  // The question is the word (as seen, with its dictionary form), not the whole line.
  await expect(question).toContainText('went');
  await expect(question).not.toContainText('after the party');
  const answer = question.getByRole('textbox', { name: t('modes.test.your_answer') });
  // Keyboard-first: the paper opens with the first answer field focused (no mouse, no Tab hunt).
  await expect(answer).toBeFocused();
  await page.keyboard.type('gitmek');
  await press(page, 'Control+Enter');
  await expect(page.getByTestId('test-score')).toBeVisible();

  // Persisted: the session's graded paper says "correct" and the key is the meaning alone.
  const graded = await rpc<Paper>('modes.test.paper', { session_id: sessionId, with_key: true });
  expect(graded.questions).toHaveLength(1);
  expect(graded.questions[0]?.kind).toBe('written');
  expect(graded.questions[0]?.key?.expected).toBe('gitmek');
  expect(graded.answers?.[0]?.text).toBe('gitmek');
  expect(graded.result?.items[0]?.verdict).toBe('correct');
  expect(graded.result?.correct).toBe(1);
});

test('S7b subtitle card back: meaning, example and source as labelled parts', async ({ page }) => {
  const { name } = await subtitleDeck('S7b');
  await openApp(page);
  await startCards(page, name);

  await press(page, 'Space');
  const back = page.getByTestId('answer');
  await expect(back).toBeVisible();
  const meaning = back.locator('.fc-part-meaning');
  const example = back.locator('.fc-part-example');
  const source = back.locator('.fc-part-source');
  await expect(meaning).toHaveText('gitmek');
  await expect(example.locator('.fc-part-label')).toHaveText(t('cards.part.example'));
  await expect(example.locator('.fc-example-text')).toHaveText('I went home after the party.');
  await expect(example.locator('.fc-example-translation')).toHaveText('Partiden sonra eve gittim.');
  await expect(source.locator('.fc-part-label')).toHaveText(t('cards.part.source'));
  await expect(source).toContainText('Journey fixture');
  // In this order, as separate parts (not one run-on answer): meaning, example, source.
  const order = await back.locator('.fc-part').evaluateAll((els) => els.map((e) => [...e.classList].find((c) => c.startsWith('fc-part-'))));
  expect(order).toEqual(['fc-part-meaning', 'fc-part-example', 'fc-part-source']);
});

test('S7c hint rule: the Subtitle card shows its line as context (word highlighted, no meaning); a reverse card blanks the word', async ({ page }) => {
  const { name } = await subtitleDeck('S7c');
  await openApp(page);
  await startCards(page, name);

  // The front: the word only — "went" large, "go" beside it — not the line.
  const front = page.getByTestId('question');
  await expect(front).toContainText('went');
  await expect(front.locator('.fc-lemma')).toHaveText('go');
  await expect(front).not.toContainText('after the party');

  await press(page, 'h');
  const hint = page.getByTestId('hint');
  await expect(hint).toBeVisible();
  await expect(hint).toHaveClass(/context/);
  const shown = plain(await hint.innerText());
  expect(shown).toContain(t('study.hint_context'));
  expect(shown).toContain('I went home after the party.');
  await expect(hint.locator('.fc-context-word')).toHaveText('went');
  // Never the answer.
  expect(shown).not.toContain(GO.meaning);
  // Next level: letters of the meaning.
  await press(page, 'h');
  await expect(hint).not.toHaveClass(/context/);
  await expect(hint).toHaveText('g_____');

  // Reverse direction: a Vocabulary note whose "production" card shows the meaning and asks
  // for the word (its "recognition" sibling suspended so the session shows the reverse card).
  const types = await rpc<{ notetypes: { id: string; builtin: string | null }[] }>('notetypes.list', {});
  const vocabulary = types.notetypes.find((n) => n.builtin === 'vocabulary')!.id;
  const rname = uniq('S7c reverse');
  const deck = await rpc<{ id: string }>('decks.create', { path: rname, parent_id: null });
  const note = await rpc<{ id: string; cards: { id: string; template_key: string }[] }>('notes.create', {
    notetype_id: vocabulary,
    deck_id: deck.id,
    fields: { word: 'go', meaning: 'gitmek', example: 'I <b>went</b> home after the party.' },
    tags: [],
    attributes: [],
    allow_duplicate: true,
  });
  const recognition = note.cards.find((c) => c.template_key === 'recognition')!.id;
  await rpc('study.suspend', { card_ids: [recognition], suspended: true });
  // Fresh start of the app (the library opens on its list, not on the last deck page).
  await openApp(page);
  await startCards(page, rname);
  await expect(page.getByTestId('question')).toContainText('gitmek');
  await press(page, 'h');
  await expect(hint).toHaveClass(/context/);
  const blanked = plain(await hint.innerText());
  expect(blanked).toContain('I ___ home after the party.');
  expect(hasWord(blanked, 'go'), `hint shows the word: ${blanked}`).toBe(false);
  expect(hasWord(blanked, 'went'), `hint shows the surface: ${blanked}`).toBe(false);
});
