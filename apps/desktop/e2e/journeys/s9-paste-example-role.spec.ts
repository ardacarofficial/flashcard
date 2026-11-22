/**
 * S9 Create → Paste into a note type with an example-role field: three lines
 * "word<TAB>meaning<TAB>sentence" become three notes whose EXAMPLE field holds the sentence
 * (CONTRACT.md §2: import mapping by role, captured context is not lost); ONE undo (the shell's
 * Ctrl+Z = `oplog.undo`) removes all three.
 *
 * Own deck (the paste goes to an existing deck: a new deck is its own undo step by design).
 */
import { choose, discardDrafts, expect, goToSection, makeDeck, notesIn, openApp, press, rpc, t, test, uniq } from '../fixtures';

/** The built-in "Vocabulary" note type: word (term), meaning, pronunciation, example (example) … */
const VOCABULARY_NOTETYPE_ID = '01900000-0000-7000-8000-00000000000a';

interface NoteType {
  id: string;
  name: string;
  fields: { key: string; role?: string | null }[];
}

test('S9 paste word⇥meaning⇥sentence ×3 → example field holds the sentence; one undo removes all', async ({ page }) => {
  const deckName = uniq('S9');
  const deckId = await makeDeck(deckName, []);
  const nt = await rpc<NoteType>('notetypes.get', { id: VOCABULARY_NOTETYPE_ID });
  const key = (role: string) => nt.fields.find((f) => f.role === role)?.key as string;
  expect(key('example'), 'the Vocabulary note type has an example-role field').toBeTruthy();
  const [termKey, meaningKey, exampleKey] = [key('term'), key('meaning'), key('example')];

  const tag = deckName.split(' ')[1];
  const lines: [string, string, string][] = [
    [`ferry${tag}`, 'vapur', 'We took the ferry to the island.'],
    [`harbour${tag}`, 'liman', 'The harbour was full of boats.'],
    [`lantern${tag}`, 'fener', 'She lit the lantern at dusk.'],
  ];

  await discardDrafts();
  await openApp(page);
  await goToSection(page, 'shell.section.create');
  const main = page.locator('#fc-main');
  await main.getByRole('tab', { name: t('add.way.paste'), exact: true }).click();
  await main.getByRole('radio', { name: t('create.mode_existing'), exact: true }).click();
  await choose(page, t('create.deck'), deckName, main);
  await choose(page, t('create.notetype'), nt.name, main);

  // Paste: the text arrives as one insertion, tabs and newlines included (as from a sheet).
  const box = main.getByRole('textbox', { name: t('io.paste.label'), exact: true });
  await box.focus();
  await page.keyboard.insertText(lines.map((l) => l.join('\t')).join('\n'));

  // The preview maps the third column to the example field (by role, unasked).
  const preview = main.getByRole('table');
  await expect(preview.getByRole('row')).toHaveCount(4);
  await expect(preview.getByRole('cell', { name: lines[0]![2], exact: true })).toBeVisible();

  // Add with Ctrl+Enter from the paste box.
  await box.focus();
  await press(page, 'Control+Enter');

  await expect.poll(async () => (await notesIn(deckId)).length).toBe(3);
  const notes = await notesIn(deckId);
  const got = notes.map((n) => [n.fields[termKey], n.fields[meaningKey], n.fields[exampleKey]]).sort();
  expect(got).toEqual([...lines].sort());
  for (const n of notes) expect(n.notetype_id).toBe(VOCABULARY_NOTETYPE_ID);

  // One undo step (Ctrl+Z with the focus outside any text field) removes all three.
  await page.locator('[data-testid="top-bar"]').click({ position: { x: 5, y: 5 } });
  await press(page, 'Control+z');
  await expect.poll(async () => (await notesIn(deckId)).length).toBe(0);
  // … and only them: the deck itself was there before and stays.
  const list = await rpc<{ items: { id: string }[] }>('notes.list', { deck_id: deckId, include_subdecks: false, page: { cursor: null, limit: null } });
  expect(list.items).toHaveLength(0);
});
