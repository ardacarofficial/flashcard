/**
 * S12 Typing in Quick Add (N): after one card is saved, typing the next one and pressing
 * Ctrl+A, then Ctrl+Z INSIDE the field edits that text only — it never touches saved cards:
 * the saved note still exists, the deck's note count is unchanged and the oplog was not undone
 * (one key = one job, COMMON.md; J7 covers the editor and a bare Ctrl+Z).
 */
import { choose, discardDrafts, expect, makeDeck, notesIn, openApp, press, rpc, t, test, uniq } from '../fixtures';

interface UndoStatus {
  can_undo: boolean;
  can_redo: boolean;
  undo_summary: { key: string; params: Record<string, string> } | null;
}

test('S12 Quick Add: Ctrl+A then Ctrl+Z inside the field never touch the saved card', async ({ page }) => {
  const deckName = uniq('S12');
  const deckId = await makeDeck(deckName, []);
  await discardDrafts();
  await openApp(page);

  await press(page, 'n');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await choose(page, t('quickadd.deck'), deckName, dialog);
  const term = dialog.getByRole('textbox').first();
  await term.focus();
  const word = `compass${deckName.split(' ')[1]}`;
  await page.keyboard.type(word);
  await press(page, 'Tab');
  await page.keyboard.type('pusula');
  await press(page, 'Control+Enter');
  await expect(term).toHaveValue('');
  await expect.poll(async () => (await notesIn(deckId)).length).toBe(1);
  const [saved] = await notesIn(deckId);
  const before = await rpc<UndoStatus>('oplog.status', {});
  expect(before.can_undo).toBe(true);
  const lastOps = await rpc<{ items: { id: string }[] }>('oplog.list', {});

  // Typing the next card: Ctrl+A selects the field's text, Ctrl+Z takes the typing back.
  await expect(term).toBeFocused();
  await page.keyboard.type('mapx');
  await press(page, 'Control+a');
  const selected = await term.evaluate((el) => {
    const f = el as HTMLInputElement | HTMLTextAreaElement;
    return f.value.slice(f.selectionStart ?? 0, f.selectionEnd ?? 0);
  });
  expect(selected, 'Ctrl+A selects the text of the field').toBe('mapx');
  await expect(dialog).toBeVisible();
  await press(page, 'Control+z');
  // The field's own undo: the typing is taken back, the focus stays in the field.
  await expect(term).toBeFocused();
  await expect(term).not.toHaveValue('mapx');
  await expect(dialog).toBeVisible();
  // Give a stray app-wide undo the time it would need to land.
  await page.waitForTimeout(1000);

  // Persisted: the saved note is there, unchanged; nothing was undone.
  const notes = await notesIn(deckId);
  expect(notes, 'the saved card must survive Ctrl+A / Ctrl+Z typed in a field').toHaveLength(1);
  expect(notes[0]?.id).toBe(saved?.id);
  expect([notes[0]?.fields.front, notes[0]?.fields.back]).toEqual([word, 'pusula']);
  const after = await rpc<UndoStatus>('oplog.status', {});
  expect(after.can_redo, 'no undo ran (nothing to redo)').toBe(false);
  expect(after.undo_summary).toEqual(before.undo_summary);
  const ops = await rpc<{ items: { id: string }[] }>('oplog.list', {});
  expect(ops.items[0]?.id, 'the oplog head is still the save').toBe(lastOps.items[0]?.id);
});
