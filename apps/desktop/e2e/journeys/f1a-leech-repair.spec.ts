/**
 * F1a (v1.4 W3) Leech repair "Düzelt": a card answered wrong again and again (through the real
 * study API: one cramming session per lapse, the typed wrong answer kept) is a leech
 * (`stats.leeches`, lapses ≥ `stats.leech_threshold` = 8). İlerleme › Daha fazla › "sürekli
 * unuttukların" › Düzelt shows the evidence (lapses, what was typed) and the core's proposals;
 * applying the first one ("accept the other meanings too") is saved: `stats.leech_repairs.list`
 * lists it and `accepted_answers.list` has both senses, the note's fields untouched. One Ctrl+Z
 * undoes the repair and its record together.
 */
import { cardHistory, expect, goToSection, makeDeck, notesIn, openApp, press, rpc, t, test, uniq } from '../fixtures';

interface Leeches {
  threshold: number;
  items: { card_id: string; lapses: number; sort_field: string }[];
}
interface RepairReport {
  items: { repair: { kind: string; card_id: string; repair: { kind: string; answers?: string[] } } }[];
}
interface AcceptedAnswers {
  fields: { field: string; answers: string[] }[];
}

const all = { deck_id: null, tag: null, query: null };

/** One cramming session (custom, not due-only, scheduled): Easy while new/relearning, Again on review. */
async function lapseOnce(deckId: string, cardId: string): Promise<void> {
  let s = await rpc('study.start', { kind: 'custom', mode: 'scheduled', deck_id: deckId, filter: { due_only: false }, tz_offset_mins: 0 });
  for (let i = 0; s.current && i < 6; i++) {
    expect(s.current.card_id).toBe(cardId);
    const { card } = await cardHistory(cardId);
    const again = card.state === 'review';
    s = await rpc('study.answer', { session_id: s.id, card_id: cardId, rating: again ? 'again' : 'easy', duration_ms: 3000, typed_answer: again ? 'inatli' : null });
  }
  await rpc('study.end', { session_id: s.id }).catch(() => undefined);
}

test('F1a leech repair: Düzelt shows the evidence, the applied repair is saved and undoable', async ({ page }) => {
  const deckName = uniq('F1a');
  const front = `stubborn ${deckName.slice(-6)}`;
  const deckId = await makeDeck(deckName, [[front, 'inatçı, dik kafalı']]);
  const [note] = await notesIn(deckId);
  const cardId = (note as unknown as { cards: { id: string }[] }).cards[0]!.id;

  // 1 graduation + 8 lapses, each through study.start / study.answer as the study screen does.
  for (let round = 0; round < 9; round++) await lapseOnce(deckId, cardId);
  expect((await cardHistory(cardId)).card.lapses).toBe(8);
  const leeches = await rpc<Leeches>('stats.leeches', { scope: all, threshold: null, limit: 50 });
  expect(leeches.threshold).toBe(8);
  expect(leeches.items.map((l) => l.card_id)).toContain(cardId);

  await openApp(page);
  await goToSection(page, 'shell.section.stats');
  const toggle = page.getByTestId('stats-more-toggle');
  if ((await toggle.getAttribute('aria-expanded')) !== 'true') await toggle.click();
  const panel = page.getByTestId('stats-leeches');
  const row = panel.getByRole('listitem').filter({ hasText: front });
  await expect(row).toContainText(t('stats.leeches.lapses_n', { n: 8 }));
  await row.getByTestId('leech-fix').click();

  const dialog = page.getByRole('dialog', { name: t('stats.repair.title', { term: front }) });
  const repair = dialog.getByTestId('leech-repair');
  await expect(repair).toBeVisible();
  await expect(repair).toContainText(/\b8 kez unuttun/); // stats.repair.lapses (a plural message)
  await expect(repair.getByTestId('leech-typed').first()).toHaveText('inatli');
  // The first proposal: the meaning's two senses, previewed as the accepted answers after.
  const proposal = repair.getByTestId('leech-proposal').first();
  await expect(proposal).toContainText(t('stats.repair.kind.add_accepted_answers'));
  await expect(proposal).toContainText('inatçı; dik kafalı');
  await proposal.getByRole('button', { name: t('stats.repair.apply') }).click();
  await expect(repair.getByTestId('leech-repair-done')).toBeVisible();

  // Saved: the repair is recorded, the answers are accepted, the note's fields unchanged.
  await expect
    .poll(async () => (await rpc<RepairReport>('stats.leech_repairs.list', { card_id: cardId, limit: 10 })).items.map((i) => i.repair.kind))
    .toEqual(['add_accepted_answers']);
  const accepted = await rpc<AcceptedAnswers>('accepted_answers.list', { note_id: note!.id });
  expect(accepted.fields).toEqual([expect.objectContaining({ field: 'back', answers: ['inatçı', 'dik kafalı'] })]);
  expect((await rpc<{ fields: Record<string, string> }>('notes.get', { id: note!.id })).fields).toEqual({ front, back: 'inatçı, dik kafalı' });
  // The dialog lists it under "after earlier repairs".
  await expect(repair).toContainText(t('stats.repair.past'));

  // One Ctrl+Z (the shell's undo): the answers and the record go together.
  // (Closed with its × button: after "Uygula" the focus drops out of the dialog to <body> —
  // the applied proposal's button is gone after the re-read — so Esc does not reach it.)
  await dialog.locator('[data-dialog-close]').click();
  await expect(dialog).toBeHidden();
  await press(page, 'Control+z');
  await expect.poll(async () => (await rpc<RepairReport>('stats.leech_repairs.list', { card_id: cardId, limit: 10 })).items).toEqual([]);
  expect((await rpc<AcceptedAnswers>('accepted_answers.list', { note_id: note!.id })).fields.flatMap((f) => f.answers)).toEqual([]);
});
