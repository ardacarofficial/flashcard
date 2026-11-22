/**
 * J6 Subtitle card, typed test: the expected answer of a Subtitle note is its MEANING only.
 *
 * KNOWN BUG, expected to FAIL until fixed (gpt-review-1 §B.1): the written-answer key is the
 * whole answer side's text (`crates/fc-app/src/modules/study/engine.rs` `answer_text`: meaning +
 * translation + source), so typing just "gitmek" for "went" is marked wrong. CONTRACT.md §2
 * (R2: field roles, the template's `answer_field`) fixes it; then remove `test.fail` below.
 */
import { expect, rpc, seed, test } from '../fixtures';

interface Paper {
  session_id: string;
  questions: { index: number; card_id: string; kind: string; prompt_html: string; key: { expected: string } | null }[];
  result: { total: number; correct: number; items: { index: number; verdict: string }[] } | null;
}

const text = (html: string) => html.replace(/<[^>]+>/g, '').replace(/&#39;|&apos;/g, "'").replace(/&amp;/g, '&').replace(/\s+/g, ' ');

test('J6 subtitle card typed test: typing the meaning only is correct', async () => {
  const { decks, subtitle } = seed();
  const s = await rpc<{ id: string }>('study.start', {
    kind: 'test',
    deck_id: decks.subtitle,
    modes: { test: { questions: subtitle.length, kinds: ['written'] } },
  });
  const paper = await rpc<Paper>('modes.test.paper', { session_id: s.id });
  expect(paper.questions).toHaveLength(subtitle.length);

  // Each question shows the word in its subtitle line: answer with that capture's meaning.
  const answers = paper.questions.map((q) => {
    expect(q.kind).toBe('written');
    const shown = text(q.prompt_html);
    const capture = subtitle.find((c) => shown.includes(c.sentence));
    expect(capture, `no seeded capture in prompt: ${shown}`).toBeTruthy();
    return { index: q.index, text: capture?.meaning ?? '' };
  });
  await rpc('modes.test.submit', { session_id: s.id, answers, duration_ms: 30_000 });

  // The graded paper, read back from the collection.
  const graded = await rpc<Paper>('modes.test.paper', { session_id: s.id, with_key: true });
  expect(graded.result?.total).toBe(subtitle.length);

  test.fail(true, 'Known bug (gpt-review-1 §B.1): the written answer key of a Subtitle card is the whole back, not the meaning');
  const wrong = (graded.result?.items ?? [])
    .filter((i) => i.verdict !== 'correct')
    .map((i) => `${answers.find((a) => a.index === i.index)?.text} → expected ${JSON.stringify(graded.questions.find((q) => q.index === i.index)?.key?.expected)}`);
  expect(wrong, 'typed the meaning, graded wrong').toEqual([]);
  expect(graded.result?.correct).toBe(subtitle.length);
});
