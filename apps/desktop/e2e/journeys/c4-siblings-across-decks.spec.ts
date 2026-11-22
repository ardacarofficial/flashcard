/**
 * C4 (closing pass) A note's two cards in different decks: each deck alone offers its card,
 * but the whole collection's queue (Home → launcher → Enter) counts and serves only one of
 * them today when the decks bury siblings (the default); a deck whose preset does not bury
 * siblings keeps its card next to the other one.
 */
import { endOpenSessions, expect, openApp, press, rpc, sessionNow, studyOptions, test, uniq } from '../fixtures';

const CLASSIC_REVERSE = '01900000-0000-7000-8000-000000000002';

interface Counts { new: number; learning: number; review: number }

test('C4 sibling cards in two decks: one of them in the collection queue', async ({ page }) => {
  await endOpenSessions();
  const due = async (d: string | null) => (await studyOptions(d)).scopes.find((s) => s.scope === 'due')!.count;
  const before = await due(null);
  const name = uniq('C4');
  const a = await rpc<{ id: string }>('decks.create', { path: `${name} A`, parent_id: null });
  const b = await rpc<{ id: string }>('decks.create', { path: `${name} B`, parent_id: null });
  const note = await rpc<{ id: string; cards: { id: string }[] }>('notes.create', {
    notetype_id: CLASSIC_REVERSE,
    deck_id: a.id,
    fields: { front: `${name} dog`, back: 'köpek' },
    tags: [],
    attributes: [],
  });
  expect(note.cards).toHaveLength(2);
  await rpc('bulk.move_cards', { card_ids: [note.cards[1]!.id], deck_id: b.id });
  expect([await due(a.id), await due(b.id)]).toEqual([1, 1]);
  expect(await due(null)).toBe(before + 1);

  await openApp(page);
  await press(page, 'Enter');
  const launcher = page.getByTestId('study-launcher');
  await expect(launcher).toBeVisible();
  await expect(launcher.getByTestId('launcher-scope-due')).toContainText(String(before + 1));
  await press(page, 'Enter');
  await expect(page.getByTestId('question')).toBeVisible();
  const session = (await studyOptions(null)).resumable[0]!.session;
  const served = (await rpc<{ counts: Counts }>('study.session', { session_id: session.id })).counts;
  expect(served.new + served.learning + served.review).toBe(before + 1);
  expect((await sessionNow(session.id)).answered).toBe(0);
  await rpc('study.end', { session_id: session.id });

  // B does not bury siblings: both cards are today's.
  const preset = await rpc<{ id: string; name: string; config: Record<string, unknown> }>('presets.create', { name, config: null });
  await rpc('presets.update', { id: preset.id, name, config: { ...preset.config, bury_siblings: false } });
  await rpc('presets.assign', { deck_ids: [b.id], preset_id: preset.id });
  expect(await due(null)).toBe(before + 2);
});
