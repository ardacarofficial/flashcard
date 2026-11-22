/**
 * J5 "advertised vs served" (gpt-review-1 §A). 21 new cards, the default 20 new cards a day,
 * 20 studied: Home must not advertise cards `study.start` cannot serve.
 *
 * KNOWN BUG, expected to FAIL until fixed: Home reads inventory counts from `decks.tree`
 * (`crates/fc-storage/src/sqlite/decks.rs`: every new card, ignoring the daily limit), while
 * `study.start` builds the real queue (`crates/fc-app/src/modules/study/session.rs`: limits,
 * study day). So Home says "1 new" and Start serves nothing. CONTRACT.md §1 (R1: `study.options`,
 * queue-based `today_*` counts) fixes it; then remove `test.fail` below.
 */
import { expect, makeDeck, openApp, rpc, RpcError, test, uniq } from '../fixtures';

interface Session {
  id: string;
  counts: { new: number; learning: number; review: number };
  current: { card_id: string } | null;
}

const tz = -new Date().getTimezoneOffset();

/** How many cards a normal session started now would serve (0 when it refuses: nothing to study). */
async function served(deckId: string): Promise<number> {
  try {
    const s = await rpc<Session>('study.start', { kind: 'normal', deck_id: deckId, tz_offset_mins: tz });
    const n = s.counts.new + s.counts.learning + s.counts.review;
    await rpc('study.end', { session_id: s.id }).catch(() => undefined);
    return n;
  } catch (e) {
    if (e instanceof RpcError && e.api.code === 'CONFLICT_NOTHING_TO_STUDY') return 0;
    throw e;
  }
}

test('J5 Home advertises only what study.start can serve (21 new, limit 20, 20 studied)', async ({ page }) => {
  const name = uniq('J5');
  const pairs = Array.from({ length: 21 }, (_, i): [string, string] => [`${name} word ${i + 1}`, `${name} anlam ${i + 1}`]);
  const deckId = await makeDeck(name, pairs);

  // Study 20 through the API (the study launcher UI is being rewritten in v1.3; a later journey
  // drives it). "Easy" graduates a new card at once, so nothing is left in learning.
  let s = await rpc<Session>('study.start', { kind: 'normal', deck_id: deckId, tz_offset_mins: tz });
  let answered = 0;
  while (s.current && answered < 20) {
    s = await rpc<Session>('study.answer', { session_id: s.id, card_id: s.current.card_id, rating: 'easy', duration_ms: 1500 });
    answered += 1;
  }
  expect(answered).toBe(20);
  // Persisted: the session recorded 20 answers.
  const summary = await rpc<{ answered: number }>('study.end', { session_id: s.id });
  expect(summary.answered).toBe(20);

  const canServe = await served(deckId);

  // What Home shows for this deck: its row's numbers (due · learning · new), 0 if not listed.
  await openApp(page);
  const rows = page.getByRole('listitem');
  await expect(rows.filter({ hasText: 'İngilizce' }).first()).toBeVisible();
  const row = rows.filter({ hasText: name });
  const advertised =
    (await row.count()) === 0
      ? 0
      : (await row.first().innerText())
          .replace(name, '')
          .split(/\s+/)
          .filter((w) => /^\d+$/.test(w))
          .slice(0, 3)
          .reduce((sum, w) => sum + Number(w), 0);

  // Preconditions held (the journey itself works); from here on the known bug decides.
  test.fail(true, 'Known bug (gpt-review-1 §A): Home advertises inventory, not the queue study.start serves');
  expect(advertised, `Home advertises ${advertised} for ${name}, study.start serves ${canServe}`).toBe(canServe);
});
