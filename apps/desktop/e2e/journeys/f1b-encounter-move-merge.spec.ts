/**
 * F1b (v1.4 E3) Note editor › "Bu kelimeyi nerede gördün": two Subtitle notes made by the real
 * capture endpoint — "glimpse" seen twice (one note, two encounters) and "wander" seen once.
 *
 * 1. Move: on glimpse's second encounter "Başka karta taşı", the note picker (search "wander")
 *    → "Buraya taşı" → `subtitle.encounters.list` reads it on wander's note, glimpse keeps one.
 * 2. Forced merge: "Bir kartla birleştir" → wander → the core refuses (different words,
 *    `CONFLICT_SUBTITLE_MERGE_WORDS`) and the app asks; "Yine de birleştir" → one note kept
 *    (glimpse: equal reviews, the older one) with all three encounters, wander's note in the
 *    trash (gone from the deck's live notes).
 */
import { captureSubtitle, expect, noteByWord, notesIn, openApp, openNoteFromSearch, rpc, t, test, uniq } from '../fixtures';

interface Encounters {
  encounters: { capture_id: string; is_primary: boolean; facts: { sentence?: string | null } }[];
}
const captures = (noteId: string) =>
  rpc<Encounters>('subtitle.encounters.list', { note_id: noteId }).then((l) => l.encounters.map((e) => e.capture_id).sort());

test('F1b encounters: move one to another note, then merge two different words with "Merge anyway"', async ({ page }) => {
  const deck = await rpc<{ id: string }>('decks.create', { path: uniq('F1b'), parent_id: null });
  const [g1, g2, w1] = await captureSubtitle(deck.id, [
    { lemma: 'glimpse', surface: 'glimpse', meaning: 'göz ucuyla görmek', sentence: 'I caught a glimpse of her.' },
    { lemma: 'glimpse', surface: 'glimpse', meaning: 'göz ucuyla görmek', sentence: 'Just a glimpse of the sea.' },
    { lemma: 'wander', surface: 'wandered', meaning: 'dolaşmak', sentence: 'We wandered around the town.' },
  ]);
  const glimpse = await noteByWord(deck.id, 'glimpse');
  const wander = await noteByWord(deck.id, 'wander');
  expect(await captures(glimpse.id)).toEqual([g1, g2].sort());
  expect(await captures(wander.id)).toEqual([w1]);

  await openApp(page);
  const editor = await openNoteFromSearch(page, 'glimpse', /glimpse/);
  const panel = editor.getByTestId('note-encounters');
  const rows = panel.getByTestId('encounter-row');
  await expect(rows).toHaveCount(2);

  // 1. Move the second line to wander's note through the note picker.
  const second = rows.filter({ hasText: 'Just a glimpse of the sea.' });
  await expect(second).not.toContainText(t('subtitle.encounter_panel.primary'));
  await second.getByRole('button', { name: t('subtitle.encounter_panel.move'), exact: true }).click();
  const movePicker = page.getByRole('dialog', { name: t('subtitle.encounter_panel.move_title') });
  await expect(movePicker.getByRole('searchbox', { name: t('subtitle.encounter_panel.search') })).toBeFocused();
  await page.keyboard.type('wander');
  const target = movePicker.getByTestId('note-picker-results').getByRole('listitem').filter({ hasText: 'wander' });
  await expect(target).toHaveCount(1);
  await target.getByRole('button', { name: t('subtitle.encounter_panel.pick_move') }).click();
  await expect(movePicker).toBeHidden();
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText('I caught a glimpse of her.');

  await expect.poll(() => captures(wander.id)).toEqual([g2, w1].sort());
  expect(await captures(glimpse.id)).toEqual([g1]);

  // 2. Merge with wander: a different word, so the app asks first; "Yine de birleştir".
  await panel.getByRole('button', { name: t('subtitle.encounter_panel.merge'), exact: true }).click();
  const mergePicker = page.getByRole('dialog', { name: t('subtitle.encounter_panel.merge_title') });
  await expect(mergePicker.getByRole('searchbox', { name: t('subtitle.encounter_panel.search') })).toBeFocused();
  await page.keyboard.type('wander');
  const other = mergePicker.getByTestId('note-picker-results').getByRole('listitem').filter({ hasText: 'wander' });
  await other.getByRole('button', { name: t('subtitle.encounter_panel.pick_merge') }).click();
  const ask = page.getByRole('alertdialog', { name: t('subtitle.encounter_panel.force_title') });
  await expect(ask).toBeVisible();
  await expect(ask).toContainText(t('subtitle.encounter_panel.force_hint'));
  // Nothing merged while it asks.
  expect(await captures(wander.id)).toEqual([g2, w1].sort());
  await ask.getByRole('button', { name: t('subtitle.encounter_panel.force_confirm') }).click();
  await expect(ask).toBeHidden();

  // Kept: glimpse (no reviews on either: the older note) with all three encounters, one main.
  await expect(rows).toHaveCount(3);
  await expect.poll(() => captures(glimpse.id)).toEqual([g1, g2, w1].sort());
  const kept = await rpc<Encounters>('subtitle.encounters.list', { note_id: glimpse.id });
  expect(kept.encounters.filter((e) => e.is_primary).map((e) => e.capture_id)).toEqual([g1]);
  // wander's note is in the trash: not among the deck's live notes, listed by `browse.trash`.
  expect((await notesIn(deck.id)).map((n) => n.id)).toEqual([glimpse.id]);
  const trash = await rpc<{ notes: { id: string }[] }>('browse.trash', { limit: null });
  expect(trash.notes.map((n) => n.id)).toContain(wander.id);
});
