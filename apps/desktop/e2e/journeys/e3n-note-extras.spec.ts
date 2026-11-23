/**
 * E3N Note editor "Bu kelime" (v1.4): open a seeded Subtitle note from Search (Enter, like J3),
 * see where the word was seen (its encounter, the word marked in the line), add an accepted
 * answer with Enter → the answer is saved in the collection (`accepted_answers.list`), and the
 * encounter is unchanged (`subtitle.encounters.list`).
 */
import { expect, goToSection, openApp, press, rpc, t, test } from '../fixtures';

interface Encounter {
  capture_id: string;
  is_primary: boolean;
  facts: { sentence?: string | null; title?: string | null };
}

test('E3N note extras: Subtitle note shows its encounter; an accepted answer added with Enter is saved', async ({ page }) => {
  const word = 'brought';
  const found = await rpc<{ items: { note_id: string }[] }>('search.cards', { query: word, notes_only: true });
  expect(found.items).toHaveLength(1);
  const noteId = found.items[0]!.note_id;

  await openApp(page);
  await goToSection(page, 'shell.section.search');
  const box = page.getByRole('searchbox').first();
  await expect(box).toBeFocused();
  await page.keyboard.type(word);
  await press(page, 'Enter');
  // One note (a Subtitle note may have more than one card: all of them are this note).
  await expect(page.getByTestId('browse-row').first()).toContainText(/bring|brought/);
  await press(page, 'ArrowDown', 'Enter');
  const editor = page.getByRole('dialog');
  await expect(editor).toBeVisible();

  // "Bu kelimeyi nerede gördün": the one encounter, the main example, the word marked.
  const extras = editor.getByTestId('note-extras');
  await expect(extras.getByRole('heading', { name: t('note_extras.title'), exact: true })).toBeVisible();
  await expect(extras.getByRole('heading', { name: t('subtitle.encounter_panel.title'), exact: true })).toBeVisible();
  const encounter = extras.getByTestId('encounter-row');
  await expect(encounter).toHaveCount(1);
  await expect(encounter).toContainText('Journey fixture');
  await expect(encounter).toContainText(t('subtitle.encounter_panel.primary'));
  await expect(encounter.locator('mark')).toHaveText(word);

  // Accepted answers: type, Enter → a chip, saved in the core.
  const answers = extras.getByTestId('note-answers');
  await expect(answers).toContainText(t('note_extras.answers_lead'));
  const input = answers.getByRole('textbox', { name: t('note_extras.answers_add') });
  await input.click();
  await page.keyboard.type('getirdi');
  await press(page, 'Enter');
  await expect(answers.getByText('getirdi', { exact: true })).toBeVisible();
  await expect(input).toHaveValue('');
  // Enter in the answer box adds the answer; it does not save and close the editor.
  await expect(editor).toBeVisible();

  await expect
    .poll(async () => (await rpc<{ fields: { answers: string[] }[] }>('accepted_answers.list', { note_id: noteId })).fields.flatMap((f) => f.answers))
    .toEqual(['getirdi']);
  const list = await rpc<{ encounters: Encounter[] }>('subtitle.encounters.list', { note_id: noteId });
  expect(list.encounters).toHaveLength(1);
  expect(list.encounters[0]!.is_primary).toBe(true);
  expect(list.encounters[0]!.facts.sentence).toBe('He brought his sister to the party.');
});
